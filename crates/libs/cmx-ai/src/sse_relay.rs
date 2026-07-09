//! OpenCode → 前端 SSE 事件中继（全局单连接多路复用）。
//!
//! 维护**一条**到 OpenCode `GET /event` 的 SSE 长连接，按事件载荷的 `properties.sessionID`
//! 将事件分发到对应 session 的所有前端订阅（经 [`SessionRegistry::broadcast`]），并把
//! OpenCode 原生事件翻译为简化的 cmx-ai 事件。
//!
//! # 翻译规则（基于 OpenCode 实测事件，非文档描述的 v1 名称）
//!
//! | OpenCode 原生事件 | cmx-ai 事件 |
//! |------|------|
//! | `message.part.delta`（`field:"text"`）| `text_delta` |
//! | `message.part.delta`（`field:"reasoning"`）| `reasoning_delta` |
//! | `message.part.updated`（`part.type:"tool"`）| `tool_call` |
//! | `question.v2.asked` | `ask_user`（登记 pending question）|
//! | `permission.v2.asked` | `require_approval`（登记 pending permission）|
//! | `session.status`（`status.type:"idle"`）| `result` + `done`（从累积文本提取产物）|
//! | `session.status`（`status.type:"error"`）| `error` + `done` |
//! | `server.connected/heartbeat` | 不下发前端（健康检查用）|
//! | 其它（`plugin.added`/`catalog.updated` 等）| 忽略 |

use std::collections::HashMap;

use futures::StreamExt;
use tokio::time::{sleep, Duration};

use crate::opencode_client::OpenCodeClient;
use crate::session_registry::{AiSseEvent, SessionRegistry};
use crate::types::*;

/// 重连初始退避（秒）。
const RECONNECT_INITIAL_BACKOFF_SECS: u64 = 1;
/// 重连最大退避（秒）。
const RECONNECT_MAX_BACKOFF_SECS: u64 = 30;

/// 启动全局 SSE relay task。
///
/// 幂等：内部循环永久重连，连接断开时按指数退避重试。需在 registry 初始化后调用。
///
/// # Panics
/// 不会 panic：所有错误都被捕获并记录，之后重连。
pub async fn start_global_relay(client: OpenCodeClient) {
    let registry = match crate::registry() {
        Some(r) => r,
        None => {
            tracing::error!("启动 SSE relay 失败：SessionRegistry 尚未初始化（请先调 init_ai_subsystem）");
            return;
        }
    };

    tokio::spawn(async move {
        let mut backoff = RECONNECT_INITIAL_BACKOFF_SECS;
        loop {
            tracing::info!("建立到 OpenCode 的 SSE 长连接（GET /event）...");
            match run_relay_loop(&client, registry).await {
                Ok(()) => {
                    // 正常退出（理论上不会，除非流主动结束）。
                    tracing::info!("OpenCode SSE 流正常结束");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "OpenCode SSE 连接异常，将重连");
                }
            }
            // 指数退避重连。
            sleep(Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(RECONNECT_MAX_BACKOFF_SECS);
        }
    });
}

/// 运行单次 SSE 连接的读取+分发循环，直到流结束或出错。
async fn run_relay_loop(
    client: &OpenCodeClient,
    registry: &SessionRegistry,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stream = client.stream_events().await?;
    tokio::pin!(stream);

    // 每个 session 的文本累积缓冲（用于在 idle 时提取最终产物）。
    // key = sessionID，value = 累积的完整文本。
    let mut text_buffers: HashMap<String, String> = HashMap::new();
    // 每个 session 的 JSON 流式切分状态（用于 json_chunk 事件）。
    let mut json_streams: HashMap<String, JsonStreamState> = HashMap::new();
    // SSE 帧累积缓冲（按 \n\n 分帧）。
    let mut frame_buf = String::new();
    // 跟踪每个 session 当前 assistant 消息的最新 text part 文本（message.part.updated 带完整文本）。
    let mut last_full_text: HashMap<String, String> = HashMap::new();
    // partID → part.type 映射（"text" / "reasoning"）。
    // 用于 message.part.delta 时区分文本/推理（OpenCode 的 delta.field 恒为 "text"，
    // 必须靠前置 message.part.updated 登记的 part.type 判定，详见源码核实）。
    let mut part_types: HashMap<String, String> = HashMap::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let bytes = chunk.as_ref();
        // SSE 帧以 \n\n 分隔；累积到帧缓冲后逐帧处理。
        if let Ok(text) = std::str::from_utf8(bytes) {
            frame_buf.push_str(text);
            while let Some(idx) = frame_buf.find("\n\n") {
                let frame: String = frame_buf.drain(..idx + 2).collect();
                process_sse_frame(
                    &frame,
                    client,
                    registry,
                    &mut text_buffers,
                    &mut json_streams,
                    &mut last_full_text,
                    &mut part_types,
                );
            }
        }
    }

    Ok(())
}

/// 解析并处理单个 SSE 帧（可能包含多行）。
fn process_sse_frame(
    frame: &str,
    client: &OpenCodeClient,
    registry: &SessionRegistry,
    text_buffers: &mut HashMap<String, String>,
    json_streams: &mut HashMap<String, JsonStreamState>,
    last_full_text: &mut HashMap<String, String>,
    part_types: &mut HashMap<String, String>,
) {
    // 提取 data: 行内容（OpenCode 每帧只有一行 data）。
    let data_line = match frame.lines().find_map(|line| line.strip_prefix("data:").map(|s| s.trim())) {
        Some(d) => d,
        None => return, // 非 data 帧（如注释）忽略。
    };
    if data_line.is_empty() {
        return;
    }

    let event: serde_json::Value = match serde_json::from_str(data_line) {
        Ok(v) => v,
        Err(e) => {
            tracing::trace!(error = %e, data = data_line, "SSE 帧 JSON 解析失败，跳过");
            return;
        }
    };

    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let props = event.get("properties").cloned().unwrap_or(serde_json::Value::Null);
    let session_id = props.get("sessionID").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        // ── 文本流式增量（真正的打字机效果来源 + json_chunk 切分）──
        "message.part.delta" => {
            handle_part_delta(
                &props,
                session_id,
                text_buffers,
                json_streams,
                part_types,
                registry,
            );
        }
        // ── 完整 part 更新（登记 partID→type + 累积文本 + 工具状态）──
        "message.part.updated" => {
            handle_part_updated(&props, session_id, last_full_text, part_types, registry);
        }
        // ── 询问用户（兼容 V1 `question.asked` 与 V2 `question.v2.asked`）──
        "question.asked" | "question.v2.asked" => {
            handle_question_asked(&props, session_id, client, registry);
        }
        // ── 权限审批（兼容 V1 `permission.asked` 与 V2 `permission.v2.asked`）──
        "permission.asked" | "permission.v2.asked" => {
            handle_permission_asked(&props, session_id, client, registry);
        }
        // ── 会话状态变更（idle = 生成完成）──
        "session.status" => {
            handle_session_status(
                &props,
                session_id,
                text_buffers,
                json_streams,
                last_full_text,
                registry,
            );
        }
        // ── 会话错误 ──
        "session.error" => {
            // OpenCode session.error 的 properties.error 是 {name, data} 结构（AssistantError 联合），
            // discriminator 为 name，data 大多含 message。原英文名/message 直接透传给用户不友好，
            // 这里按 name 归类为中文文案；细节 message 仅作为辅助信息附在括号里。
            let err = props.get("error");
            let name = err.and_then(|e| e.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            let detail = err
                .and_then(|e| e.get("data"))
                .and_then(|d| d.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("");

            // MessageAbortedError（用户主动中断）特殊处理，对齐 opencode TUI：
            // 若本轮已产出过正文/工具内容，视为正常完成，仅发 done；否则提示已中断。
            if name == "MessageAbortedError" {
                let has_content = last_full_text
                    .get(session_id)
                    .map(|t| !t.trim().is_empty())
                    .unwrap_or(false);
                if has_content {
                    tracing::info!(session_id, "用户中断但已有内容，按正常完成处理");
                } else {
                    // code=499 标识"客户端中断"(对齐 nginx 499 Client Closed Request)，
                    // 前端据此用柔和样式展示，区别于真实错误的红框。
                    registry.broadcast(session_id, AiSseEvent::error("已中断", Some(499)));
                }
            } else {
                let msg = friendly_error_message(name, detail);
                registry.broadcast(session_id, AiSseEvent::error(&msg, None));
            }
            registry.broadcast(session_id, AiSseEvent::done());
            // 生成异常结束：释放活跃锁。
            registry.release_session(session_id);
        }
        // ── 控制帧（健康检查，不下发前端）──
        "server.connected" => {
            tracing::debug!("OpenCode SSE 已连接（server.connected）");
        }
        "server.heartbeat" => {
            tracing::trace!("OpenCode SSE 心跳");
        }
        "server.instance.disposed" => {
            tracing::warn!("OpenCode 实例已销毁（server.instance.disposed），relay 将重连");
        }
        // ── 其它事件（plugin.added / catalog.updated / session.updated 等）──
        _ => {
            tracing::trace!(event_type, session_id, "忽略非关键 OpenCode 事件");
        }
    }
}

/// 处理 `message.part.delta`：流式文本/推理增量 + JSON 边界切分（json_chunk）。
fn handle_part_delta(
    props: &serde_json::Value,
    session_id: &str,
    text_buffers: &mut HashMap<String, String>,
    json_streams: &mut HashMap<String, JsonStreamState>,
    part_types: &mut HashMap<String, String>,
    registry: &SessionRegistry,
) {
    if session_id.is_empty() {
        return;
    }
    // OpenCode 的 delta.field 恒为 "text"（文本/推理都是），不能用它区分类型；
    // 必须靠前置 message.part.updated 登记的 partID → part.type 映射判定（源码核实结论）。
    let part_id = props.get("partID").and_then(|v| v.as_str()).unwrap_or("");
    let delta = props.get("delta").and_then(|v| v.as_str()).unwrap_or("");
    if delta.is_empty() {
        return;
    }

    // 查 part.type：登记过则用登记值，未登记默认按 text 处理（兜底）。
    let part_type = part_types
        .get(part_id)
        .map(String::as_str)
        .unwrap_or("text");

    match part_type {
        "reasoning" => {
            // 推理过程：直接广播 reasoning_delta，不累积（不作为最终产物，不切 JSON）。
            registry.broadcast(session_id, AiSseEvent::reasoning_delta(delta));
        }
        _ => {
            // text（及其它未知类型，兜底按文本处理）：
            // 1. 累积完整文本（idle 时提取最终产物用）。
            text_buffers
                .entry(session_id.to_string())
                .or_default()
                .push_str(delta);

            // 2. JSON 边界检测：识别 ```json 围栏或裸 JSON（连续 {/[ 起始），
            //    切分为 json_chunk 事件供前端渐进预览；同时仍下发 text_delta。
            let state = json_streams
                .entry(session_id.to_string())
                .or_default();
            if let Some(chunk_event) = state.feed(delta) {
                registry.broadcast(session_id, AiSseEvent::json_chunk(chunk_event));
            }

            // 3. 普通文本增量（前端打字机效果）。
            registry.broadcast(session_id, AiSseEvent::text_delta(delta));
        }
    }
}

/// 处理 `message.part.updated`：登记 partID→type + 累积完整文本 + 工具调用状态。
fn handle_part_updated(
    props: &serde_json::Value,
    session_id: &str,
    last_full_text: &mut HashMap<String, String>,
    part_types: &mut HashMap<String, String>,
    registry: &SessionRegistry,
) {
    if session_id.is_empty() {
        return;
    }
    let part = match props.get("part") {
        Some(p) => p,
        None => return,
    };
    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let part_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    // 登记 partID → part.type，供后续 message.part.delta 查表区分 text/reasoning。
    // （OpenCode delta.field 恒为 "text"，无法直接区分，必须靠此处登记。）
    if !part_id.is_empty() {
        match part_type {
            "text" | "reasoning" => {
                part_types.insert(part_id.clone(), part_type.to_string());
            }
            _ => {}
        }
    }
    match part_type {
        "text" => {
            // 记录最新的完整文本（用于 idle 时提取产物）。
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                last_full_text.insert(session_id.to_string(), text.to_string());
            }
        }
        "tool" => {
            let tool = part.get("tool").and_then(|v| v.as_str()).unwrap_or("unknown");
            let state_obj = part.get("state").unwrap_or(&serde_json::Value::Null);
            let state = state_obj
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("running");
            // 统一从 state 对象抽取 input/output/metadata（对齐 opencode ToolState 结构）。
            // skill 工具的 input.name 是前端渲染技能名标题的依据；
            // 各工具 completed 态的 output 是前端展示详情的依据。
            let input = state_obj.get("input").cloned().filter(|v| !v.is_null());
            let metadata = state_obj.get("metadata").cloned().filter(|v| !v.is_null());
            let output = state_obj
                .get("output")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // question/permission 工具：pending/running 时走专用通道（ask_user / require_approval 弹层），
            // 不下发 tool_call；completed/error 时下发完整 tool_call（带 input/metadata），
            // 让前端把"已回答"作为工具卡片留在消息流里（对齐 opencode）。
            if tool == "question" || tool == "permission" {
                if state != "completed" && state != "error" {
                    return;
                }
            }
            registry.broadcast(
                session_id,
                AiSseEvent::tool_call_full(ToolCallEvent {
                    tool: tool.to_string(),
                    part_id: part_id.clone(),
                    state: state.to_string(),
                    input,
                    output,
                    metadata,
                }),
            );
        }
        _ => {}
    }
}

/// 处理 `question.v2.asked`：翻译为 `ask_user` 事件 + 登记 pending question + 启动超时定时器。
fn handle_question_asked(
    props: &serde_json::Value,
    session_id: &str,
    _client: &OpenCodeClient,
    registry: &SessionRegistry,
) {
    let question_id = props.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if question_id.is_empty() || session_id.is_empty() {
        return;
    }

    // 登记待处理 question（仅记录 id 供 answer 接口转发；无超时 —— 对齐 OpenCode 原生行为：
    // question 无限等待直到用户回答/会话结束，不自动 reject）。
    registry.register_pending_question(session_id, question_id);

    let raw_questions = match props.get("questions").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return,
    };
    // 遍历整个 questions 数组（对齐 OpenCode：一次 ask 携带多问，前端统一呈现）。
    let questions: Vec<AskUserQuestion> = raw_questions
        .iter()
        .filter_map(|q| {
            let multiple = q.get("multiple").and_then(|v| v.as_bool()).unwrap_or(false);
            // custom 默认 true（OpenCode V1 语义：默认允许自定义文本答案，与选项并存）。
            let custom = q.get("custom").and_then(|v| v.as_bool()).unwrap_or(true);
            let options: Vec<AskUserOption> = q
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|o| {
                            Some(AskUserOption {
                                label: o.get("label").and_then(|v| v.as_str())?.to_string(),
                                description: o
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            // type 仅由「有无选项 + 是否多选」决定；custom 作为额外能力透传前端，
            // 不再因 custom:true 把有选项的问题强制变 text（那会丢失选项）。
            let question_type = if options.is_empty() {
                "text".to_string()
            } else if multiple {
                "multi_choice".to_string()
            } else {
                "single_choice".to_string()
            };
            Some(AskUserQuestion {
                question_type,
                title: q.get("header").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                message: q.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                multiple,
                custom,
                options,
            })
        })
        .collect();
    if questions.is_empty() {
        return;
    }

    let ev = AskUserEvent {
        question_id: question_id.to_string(),
        questions,
    };
    registry.broadcast(session_id, AiSseEvent::new("ask_user", &ev));
}

/// 处理 `permission.asked` / `permission.v2.asked`：翻译为 `require_approval` + 登记 pending permission。
/// 无超时 —— 对齐 OpenCode 原生行为：permission 无限等待直到用户审批/会话结束。
fn handle_permission_asked(
    props: &serde_json::Value,
    session_id: &str,
    _client: &OpenCodeClient,
    registry: &SessionRegistry,
) {
    let permission_id = props.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if permission_id.is_empty() || session_id.is_empty() {
        return;
    }

    // 登记待处理 permission（仅记录 id 供 approval 接口转发；无超时）。
    registry.register_pending_permission(session_id, permission_id);

    // opencode V1 用 `permission`(action 名)+ `patterns` + `metadata`；
    // V2 用 `action` + `resources` + `metadata`。实际运行只发布 V1,但两者都兼容。
    let action = props
        .get("action")
        .and_then(|v| v.as_str())
        .or_else(|| props.get("permission").and_then(|v| v.as_str()))
        .unwrap_or("");

    // 描述:优先从 metadata 提取工具特定信息(bash 的 command、edit 的 path/diff 等),
    // 回退到 patterns/resources 的第一个。
    let metadata = props.get("metadata").unwrap_or(&serde_json::Value::Null);
    let description = metadata
        .get("command")
        .and_then(|v| v.as_str())
        .or_else(|| metadata.get("path").and_then(|v| v.as_str()))
        .or_else(|| metadata.get("url").and_then(|v| v.as_str()))
        .or_else(|| metadata.get("pattern").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .or_else(|| {
            // 回退:patterns(V1) 或 resources(V2) 的第一个元素
            props
                .get("patterns")
                .or_else(|| props.get("resources"))
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let ev = RequireApprovalEvent {
        approval_id: permission_id.to_string(),
        action: action.to_string(),
        title: humanize_action(action),
        description,
        diff: None, // 一期（纯生成）审批较少，diff 留二期。
    };
    registry.broadcast(session_id, AiSseEvent::new("require_approval", &ev));
}

/// 处理 `session.status`：idle 时提取产物并下发 result+done。
fn handle_session_status(
    props: &serde_json::Value,
    session_id: &str,
    text_buffers: &mut HashMap<String, String>,
    json_streams: &mut HashMap<String, JsonStreamState>,
    last_full_text: &mut HashMap<String, String>,
    registry: &SessionRegistry,
) {
    if session_id.is_empty() {
        return;
    }
    let status_type = props
        .get("status")
        .and_then(|s| s.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match status_type {
        "idle" => {
            // 生成完成：从累积文本提取产物。
            // 优先用 last_full_text（message.part.updated 的完整文本），回退 text_buffers。
            let full_text = last_full_text
                .get(session_id)
                .cloned()
                .or_else(|| text_buffers.get(session_id).cloned())
                .unwrap_or_default();
            let result = extract_result(&full_text);
            registry.broadcast(session_id, AiSseEvent::result(result));
            registry.broadcast(session_id, AiSseEvent::done());
            // 清理本轮缓冲（含 JSON 流式状态）+ 释放活跃锁。
            text_buffers.remove(session_id);
            json_streams.remove(session_id);
            last_full_text.remove(session_id);
            registry.release_session(session_id);
        }
        "retry" => {
            // 重试中（非终态）：仅记日志，不下发前端。
            // retry 是瞬时状态，OpenCode 会继续尝试；若当 error 下发会让前端误以为本轮失败结束。
            // 真正的失败终态由独立的 session.error 事件处理（见 process_sse_frame）。
            let msg = props
                .get("status")
                .and_then(|s| s.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let attempt = props
                .get("status")
                .and_then(|s| s.get("attempt"))
                .and_then(|v| v.as_u64());
            tracing::warn!(session_id, attempt, msg, "OpenCode 上游重试中（非终态，不下发前端）");
        }
        "busy" => {
            tracing::trace!(session_id, "session 进入 busy");
        }
        // 注：session.status 只有 idle/busy/retry（源码核实），无 error 子类型。
        // 错误走独立的 session.error 事件（见 process_sse_frame 的 session.error 分支）。
        _ => {
            tracing::trace!(status_type, session_id, "未处理的 session.status 子类型");
        }
    }
}

// 注：原 ask_user/require_approval 的 120 秒硬超时已移除 —— 对齐 OpenCode 原生行为：
// question/permission 无限等待直到用户回答/会话结束，不自动 reject。
// （opencode V1/V2 question 服务、permission 服务、HTTP server 均无任何超时计时器。）

/// JSON 流式切分状态机（per-session），用于把 `message.part.delta` 中的 JSON 片段
/// 切分为 `json_chunk` 事件。
///
/// # 状态
/// - [`JsonStreamState::Inactive`]：尚未进入 JSON 块。检测到 ```` ```json ```` 围栏起始，
///   或裸 JSON（当前累积以 `{`/`[` 开头）时切换。
/// - [`JsonStreamState::InFencedJson`]：在 ```` ```json ```` 围栏内，每个 delta 直接作为一个 chunk。
/// - [`JsonStreamState::InBareJson`]：裸 JSON 块，同上。
///
/// # 切分策略（一期）
/// 进入 JSON 块后，每次 [`JsonStreamState::feed`] 把当前 delta 作为独立 chunk 广播，
/// `chunk_index` 自增。前端按顺序累积即可实时预览（文档 4.3 节）。
/// 检测到围栏结束（```` ``` ````）时切回 Inactive。复杂括号深度切分留二期。
#[derive(Debug, Clone, Default)]
struct JsonStreamState {
    /// 当前是否处于 JSON 块内，及进入方式。
    mode: JsonMode,
    /// 已下发的 chunk 序号（从 0 开始）。
    chunk_index: u32,
}

/// JSON 块的进入方式。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum JsonMode {
    /// 未进入 JSON 块。
    #[default]
    Inactive,
    /// 在 ```json 围栏内。
    Fenced,
    /// 裸 JSON（以 { 或 [ 起始）。
    Bare,
}

/// ```` ```json ```` 围栏起始标记。
const FENCE_OPEN: &str = "```json";
/// ```` ``` ```` 围栏结束标记。
const FENCE_CLOSE: &str = "```";

impl JsonStreamState {
    /// 喂入一段文本 delta，若产生新的 JSON 片段则返回 [`JsonChunkEvent`]。
    ///
    /// 一次 feed 最多返回一个 chunk（当前 delta 全量作为一个片段）。
    fn feed(&mut self, delta: &str) -> Option<JsonChunkEvent> {
        match self.mode {
            JsonMode::Inactive => {
                // 检测 ```json 围栏起始。
                if let Some(pos) = delta.find(FENCE_OPEN) {
                    // 围栏后的内容（跳过围栏标记 + 可能的换行）作为首个 chunk。
                    let after = &delta[pos + FENCE_OPEN.len()..];
                    let content = after.strip_prefix('\n').unwrap_or(after);
                    if !content.is_empty() {
                        self.mode = JsonMode::Fenced;
                        let ev = self.make_chunk(content);
                        return Some(ev);
                    }
                    // 围栏标记在 delta 末尾，下次 feed 再切。
                    self.mode = JsonMode::Fenced;
                    return None;
                }
                // 检测裸 JSON：当前累积 trim 后以 { 或 [ 开头。
                // 仅当 delta 本身以 { / [ 起始（避免把普通文本中的括号误判）。
                let trimmed = delta.trim_start();
                if trimmed.starts_with('{') || trimmed.starts_with('[') {
                    self.mode = JsonMode::Bare;
                    let content = trimmed;
                    if content.is_empty() {
                        return None;
                    }
                    return Some(self.make_chunk(content));
                }
                None
            }
            JsonMode::Fenced => {
                // 检测围栏结束 ```：移除围栏标记，切回 Inactive，不广播围栏本身。
                if let Some(pos) = delta.find(FENCE_CLOSE) {
                    let content = &delta[..pos];
                    self.mode = JsonMode::Inactive;
                    if content.is_empty() {
                        return None;
                    }
                    return Some(self.make_chunk(content));
                }
                // 块内：整段 delta 作为一个 chunk。
                if delta.is_empty() {
                    return None;
                }
                Some(self.make_chunk(delta))
            }
            JsonMode::Bare => {
                // 裸 JSON：当遇到顶层闭合后的非 JSON 文本时切回 Inactive。
                // 一期简化：整段 delta 作为一个 chunk；遇到 ``` 或明显非 JSON 文本（如纯中文描述）时结束。
                // 用「delta 含 ``` 或全是非 JSON 字符」作为粗略结束判断。
                if delta.contains(FENCE_CLOSE) {
                    self.mode = JsonMode::Inactive;
                    let content = delta.split(FENCE_CLOSE).next().unwrap_or("");
                    if content.is_empty() {
                        return None;
                    }
                    return Some(self.make_chunk(content));
                }
                if delta.is_empty() {
                    return None;
                }
                Some(self.make_chunk(delta))
            }
        }
    }

    /// 构造一个 chunk 事件并自增序号。
    fn make_chunk(&mut self, content: &str) -> JsonChunkEvent {
        let ev = JsonChunkEvent {
            chunk_index: self.chunk_index,
            path: None, // 一期不推断路径，二期可基于括号深度推断。
            content: content.to_string(),
            total_hint: None,
        };
        self.chunk_index += 1;
        ev
    }
}

/// 从累积文本中提取最终产物，构造 [`ResultEvent`]。
///
/// 一期识别规则：若文本包含 HTML 标签（如 `<div`、`<ui5-`）或 `<script`，
/// 视为 HTML 页面产物；否则视为纯文本说明。二期可增加 JSON 产物（DCT/DOC）识别。
fn extract_result(full_text: &str) -> ResultEvent {
    let is_html = full_text.contains("<div")
        || full_text.contains("<ui5-")
        || full_text.contains("<cmx-")
        || full_text.contains("<script");

    let (result_type, product_type) = if is_html {
        ("html_page_result", "html")
    } else {
        ("text_result", "text")
    };

    ResultEvent {
        result_type: result_type.to_string(),
        data: full_text.to_string(),
        validation: Some(ResultValidation {
            passed: !full_text.is_empty(),
            message: None,
        }),
        summary: Some(summarize(full_text)),
        // 一期固定不可保存（二期置 true 时前端显示保存按钮）。
        saveable: false,
        product_type: product_type.to_string(),
    }
}

/// 生成结果的简短摘要（取前 60 字符）。
fn summarize(text: &str) -> String {
    let trimmed = text.trim();
    let summary: String = trimmed.chars().take(60).collect();
    if trimmed.chars().count() > 60 {
        format!("{summary}…")
    } else {
        summary
    }
}

/// 把 OpenCode action 转为人话标题。
fn humanize_action(action: &str) -> String {
    match action {
        "write" => "确认写入文件".to_string(),
        "edit" => "确认修改文件".to_string(),
        "bash" => "确认执行命令".to_string(),
        "read" => "确认读取文件".to_string(),
        "grep" => "确认搜索文件".to_string(),
        "glob" => "确认查找文件".to_string(),
        "webfetch" => "确认访问网页".to_string(),
        "websearch" => "确认执行搜索".to_string(),
        "external_directory" => "确认访问工作区外目录".to_string(),
        "" => "确认执行操作".to_string(),
        _ => format!("确认执行：{action}"),
    }
}

/// 把 OpenCode AssistantError 的 name + data.message 归类为对用户友好的中文文案。
/// 透传英文 name（如 `MessageAbortedError`、`APIError`）或英文 message 体验差，这里统一映射。
/// detail（原 message）仅对部分错误作为辅助信息附在括号里；为空则忽略。
fn friendly_error_message(name: &str, detail: &str) -> String {
    let detail = detail.trim();
    let has_detail = !detail.is_empty();
    let base = match name {
        // 鉴权类
        "ProviderAuthError" => "AI 服务鉴权失败，请检查 API Key 配置".to_string(),
        "AuthError" => "AI 服务鉴权失败，请检查 API Key 配置".to_string(),
        // 中断（兜底，正常路径在 session.error 分支已提前处理）
        "MessageAbortedError" => "已中断".to_string(),
        // 输出长度超限
        "MessageOutputLengthError" => "回复长度超出上限，请缩小问题范围后重试".to_string(),
        // 结构化输出失败
        "StructuredOutputError" => "结构化输出解析失败，请重试".to_string(),
        // 上下文超长
        "ContextOverflowError" => "对话上下文过长，请精简后重试".to_string(),
        // 内容安全过滤
        "ContentFilterError" => "内容被安全策略拦截".to_string(),
        // API 调用错误（provider/网关/限流），保留细节
        "APIError" if has_detail => format!("AI 服务请求失败（{detail}）"),
        "APIError" => "AI 服务请求失败，请稍后重试".to_string(),
        // 未知错误，尽量带上原始 message
        "UnknownError" if has_detail => format!("生成失败：{detail}"),
        _ if has_detail => format!("生成失败：{detail}"),
        _ => "生成失败，请重试".to_string(),
    };
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_html_result() {
        let text = r#"<div data-node-id="n-1"><ui5-input></ui5-input></div>"#;
        let r = extract_result(text);
        assert_eq!(r.result_type, "html_page_result");
        assert_eq!(r.product_type, "html");
        assert!(r.validation.as_ref().unwrap().passed);
    }

    #[test]
    fn extract_text_result() {
        let r = extract_result("只是普通文本说明");
        assert_eq!(r.result_type, "text_result");
        assert_eq!(r.product_type, "text");
    }

    #[test]
    fn summarize_truncates_long_text() {
        let long = "a".repeat(100);
        let s = summarize(&long);
        assert!(s.ends_with('…'));
        assert_eq!(s.chars().count(), 61); // 60 + 省略号
    }

    #[test]
    fn humanize_known_actions() {
        assert_eq!(humanize_action("write"), "确认写入文件");
        assert_eq!(humanize_action("bash"), "确认执行命令");
        assert_eq!(humanize_action("custom"), "确认执行：custom");
    }

    // ── JsonStreamState 切分逻辑测试 ──

    #[test]
    fn json_stream_ignores_plain_text() {
        let mut s = JsonStreamState::default();
        // 纯文本不应产生 json_chunk。
        assert!(s.feed("这是普通说明文本，没有 JSON").is_none());
        assert_eq!(s.mode, JsonMode::Inactive);
    }

    #[test]
    fn json_stream_detects_fenced_json_open() {
        let mut s = JsonStreamState::default();
        // 围栏起始：```json 后跟内容。
        let chunk = s.feed("```json\n{\"name\":\"员工表\"").expect("应产生首个 chunk");
        assert_eq!(s.mode, JsonMode::Fenced);
        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.content, "{\"name\":\"员工表\"");
        assert!(chunk.path.is_none());
        assert!(chunk.total_hint.is_none());
    }

    #[test]
    fn json_stream_fenced_continues_and_closes() {
        let mut s = JsonStreamState::default();
        let _ = s.feed("```json\n{\"a\":1,");
        // 块内继续：chunk_index 自增。
        let chunk2 = s.feed("\"b\":2}").expect("块内应产生 chunk");
        assert_eq!(chunk2.chunk_index, 1);
        assert_eq!(chunk2.content, "\"b\":2}");
        // 围栏结束：切回 Inactive。
        let chunk3 = s.feed("```");
        // 围栏结束标记本身无内容，不产生 chunk。
        assert!(chunk3.is_none());
        assert_eq!(s.mode, JsonMode::Inactive);
    }

    #[test]
    fn json_stream_detects_bare_json() {
        let mut s = JsonStreamState::default();
        // 裸 JSON（以 { 起始）。
        let chunk = s.feed("{\"dictId\":\"customer\"").expect("裸 JSON 应产生 chunk");
        assert_eq!(s.mode, JsonMode::Bare);
        assert_eq!(chunk.chunk_index, 0);
        assert!(chunk.content.starts_with('{'));
    }

    #[test]
    fn json_stream_bare_json_terminated_by_fence_close() {
        let mut s = JsonStreamState::default();
        let _ = s.feed("[1,2,3");
        // 遇到 ``` 切回 Inactive（粗略结束判断）。
        let chunk = s.feed("]```");
        assert!(chunk.is_some());
        assert_eq!(s.mode, JsonMode::Inactive);
    }

    #[test]
    fn json_stream_chunk_index_increments_across_chunks() {
        let mut s = JsonStreamState::default();
        let _ = s.feed("```json\n[");
        let c1 = s.feed("1,").expect("chunk");
        let c2 = s.feed("2,").expect("chunk");
        let c3 = s.feed("3]").expect("chunk");
        assert_eq!(c1.chunk_index, 1);
        assert_eq!(c2.chunk_index, 2);
        assert_eq!(c3.chunk_index, 3);
    }

    #[test]
    fn json_stream_empty_delta_returns_none() {
        let mut s = JsonStreamState::default();
        let _ = s.feed("```json\n{}");
        assert!(s.feed("").is_none());
    }

    // ── Bug1 回归测试：partID 查表区分 text/reasoning delta（field 恒为 text）──

    /// 辅助：构造 message.part.updated 的 properties 载荷。
    fn part_updated_props(part_id: &str, part_type: &str, text: &str) -> serde_json::Value {
        serde_json::json!({
            "sessionID": "ses_test",
            "part": {
                "id": part_id,
                "type": part_type,
                "text": text,
                "sessionID": "ses_test",
                "messageID": "msg_test"
            },
            "time": 0
        })
    }

    /// 辅助：构造 message.part.delta 的 properties 载荷（field 恒为 "text"，模拟 OpenCode）。
    fn part_delta_props(part_id: &str, delta: &str) -> serde_json::Value {
        serde_json::json!({
            "sessionID": "ses_test",
            "messageID": "msg_test",
            "partID": part_id,
            "field": "text",
            "delta": delta
        })
    }

    #[tokio::test]
    async fn delta_text_part_emits_text_delta() {
        // 前置 message.part.updated 登记 prt_text 为 text part。
        let reg = SessionRegistry::new();
        let mut last_full = HashMap::new();
        let mut part_types = HashMap::new();
        handle_part_updated(
            &part_updated_props("prt_text1", "text", ""),
            "ses_test",
            &mut last_full,
            &mut part_types,
            &reg,
        );
        assert_eq!(part_types.get("prt_text1").map(|s| s.as_str()), Some("text"));

        // 订阅 + 发 delta，应收到 text_delta（而非 reasoning_delta）。
        let mut rx = reg.subscribe("ses_test");
        let mut bufs = HashMap::new();
        let mut jsons = HashMap::new();
        handle_part_delta(
            &part_delta_props("prt_text1", "你好"),
            "ses_test",
            &mut bufs,
            &mut jsons,
            &mut part_types,
            &reg,
        );
        let ev = rx.recv().await.expect("应收到 text_delta");
        assert_eq!(ev.event_name, "text_delta");
        assert!(ev.payload.contains("你好"));
    }

    #[tokio::test]
    async fn delta_reasoning_part_emits_reasoning_delta() {
        // 关键回归：OpenCode 的 reasoning delta 字段也是 field="text"，但 partID 对应 reasoning part。
        // 必须靠 partID 查表发 reasoning_delta，而非 text_delta。
        let reg = SessionRegistry::new();
        let mut last_full = HashMap::new();
        let mut part_types = HashMap::new();
        handle_part_updated(
            &part_updated_props("prt_reason1", "reasoning", ""),
            "ses_test",
            &mut last_full,
            &mut part_types,
            &reg,
        );
        assert_eq!(part_types.get("prt_reason1").map(|s| s.as_str()), Some("reasoning"));

        let mut rx = reg.subscribe("ses_test");
        let mut bufs = HashMap::new();
        let mut jsons = HashMap::new();
        handle_part_delta(
            &part_delta_props("prt_reason1", "思考中..."),
            "ses_test",
            &mut bufs,
            &mut jsons,
            &mut part_types,
            &reg,
        );
        let ev = rx.recv().await.expect("应收到 reasoning_delta");
        assert_eq!(ev.event_name, "reasoning_delta");
        assert!(ev.payload.contains("思考中..."));
    }

    #[tokio::test]
    async fn delta_unknown_partid_defaults_to_text() {
        // 未登记的 partID（无前置 part.updated）兜底按 text 处理。
        let reg = SessionRegistry::new();
        let mut rx = reg.subscribe("ses_test");
        let mut bufs = HashMap::new();
        let mut jsons = HashMap::new();
        let mut part_types = HashMap::new();
        handle_part_delta(
            &part_delta_props("prt_unknown", "hi"),
            "ses_test",
            &mut bufs,
            &mut jsons,
            &mut part_types,
            &reg,
        );
        let ev = rx.recv().await.expect("应兜底收到 text_delta");
        assert_eq!(ev.event_name, "text_delta");
    }
}
