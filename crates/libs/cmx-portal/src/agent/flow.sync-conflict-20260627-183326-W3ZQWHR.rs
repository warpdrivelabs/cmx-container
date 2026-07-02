//! agent 流程编排（复刻 `agentRoutes.js` 的 runAgentFlow / buildApprovalEvents / approvals）。
//!
//! 事件序列：assistant → plan → tool_call/tool_result（按 decision 选只读工具）→ workflow →
//! assistant_start/done。approval 分支生成 approval_required + 暂存 pendingApprovals。

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::agent::{planner, root_dir, schemas, tools};
use crate::error::{PortalError, PortalResult};

/// 待审批项。
#[derive(Clone)]
struct PendingApproval {
    action: String,
    args: Value,
    created_at: u128,
    context: Value,
}

fn pending() -> &'static Mutex<HashMap<String, PendingApproval>> {
    static M: OnceLock<Mutex<HashMap<String, PendingApproval>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// 生成带时间戳的伪唯一 id（用 ms + 计数器，避免依赖 rand）。
fn now_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CNT: AtomicU64 = AtomicU64::new(0);
    let c = CNT.fetch_add(1, Ordering::Relaxed);
    let ms = now_ms();
    format!("{prefix}_{:x}_{:x}", ms, c)
}

/// ISO8601 时间戳。
fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 构造一个 agent 事件。
fn event(event_type: &str, mut payload: Value) -> Value {
    let obj = payload.as_object_mut().unwrap();
    obj.insert("type".to_string(), json!(event_type));
    obj.insert("at".to_string(), json!(iso_now()));
    payload
}

/// 归一消息（role 合法 + content 截断 + 取最近 20）。
pub fn normalize_messages(raw: &Value) -> Vec<Value> {
    let arr = raw.as_array().cloned().unwrap_or_default();
    let mut out: Vec<Value> = arr
        .iter()
        .map(|m| {
            json!({
                "role": m.get("role").and_then(|v| v.as_str()).unwrap_or("").trim(),
                "content": m.get("content").and_then(|v| v.as_str()).map(|s| s.chars().take(8000).collect::<String>()).unwrap_or_default(),
            })
        })
        .filter(|m| {
            matches!(m.get("role").and_then(|v| v.as_str()), Some("user" | "assistant" | "system"))
                && !m.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().is_empty()
        })
        .collect();
    if out.len() > 20 {
        out = out[out.len() - 20..].to_vec();
    }
    out
}

/// 运行 agent 流程，收集事件（非流式）。`emit` 回调用于流式增量推送。
pub async fn run_agent_flow<F>(messages: &[Value], context: &Value, mut emit: F) -> PortalResult<Vec<Value>>
where
    F: FnMut(Value),
{
    let root = root_dir();
    let text = planner::latest_user_text(messages);
    if text.is_empty() {
        return Err(PortalError::bad_request("缺少用户消息"));
    }
    let mut emitted: Vec<Value> = Vec::new();
    macro_rules! send {
        ($ev:expr) => {{
            let ev = $ev;
            emitted.push(ev.clone());
            emit(ev);
        }};
    }

    let decision = planner::plan(messages, &root).await;

    if decision.get("kind").and_then(|v| v.as_str()) == Some("approval") {
        let approval_events = build_approval_events(&root, &decision, context).await?;
        for ev in approval_events {
            send!(ev);
        }
        return Ok(emitted);
    }

    send!(event("assistant", json!({ "text": decision.get("intro").and_then(|v| v.as_str()).unwrap_or("我先查看项目上下文。") })));
    send!(event("plan", json!({ "items": decision.get("plan").cloned().unwrap_or(json!([])) })));

    // 只读工具按 decision 触发
    if decision.get("wantsDefinitions").and_then(|v| v.as_bool()).unwrap_or(false) {
        let id = now_id("tool");
        let args = json!({ "limit": 60 });
        send!(event("tool_call", json!({ "id": id, "name": "list_definitions", "args": args })));
        match tools::run_tool(&root, "list_definitions", &args).await {
            Ok(data) => {
                let n = data.as_array().map(|a| a.len()).unwrap_or(0);
                send!(event("tool_result", json!({ "id": id, "status": "ok", "summary": format!("找到 {n} 个定义文件摘要。"), "data": data })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    }
    if decision.get("wantsHtmlPages").and_then(|v| v.as_bool()).unwrap_or(false) {
        let id = now_id("tool");
        let args = decision.get("htmlPagesFilter").filter(|v| v.is_object()).cloned().unwrap_or(json!({ "page": 1, "pageSize": 20 }));
        send!(event("tool_call", json!({ "id": id, "name": "list_html_pages", "args": args })));
        match tools::run_tool(&root, "list_html_pages", &args).await {
            Ok(data) => {
                let total = data.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
                let n = data.get("items").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                send!(event("tool_result", json!({ "id": id, "status": "ok", "summary": format!("找到 {total} 个自定义 HTML 页面，本次返回 {n} 个。"), "data": data })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    }
    if let Some(args) = decision.get("readHtmlPage").filter(|v| v.is_object()) {
        let id = now_id("tool");
        send!(event("tool_call", json!({ "id": id, "name": "read_html_page", "args": args })));
        match tools::run_tool(&root, "read_html_page", args).await {
            Ok(data) => {
                let pid = data.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let bytes = data.get("bytes").and_then(|v| v.as_i64()).unwrap_or(0);
                send!(event("tool_result", json!({ "id": id, "status": "ok", "summary": format!("读取 HTML 页面 {pid}，{bytes} bytes。"), "data": data })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    }
    if let Some(args) = decision.get("readFile").filter(|v| v.is_object()) {
        let id = now_id("tool");
        send!(event("tool_call", json!({ "id": id, "name": "read_file", "args": args })));
        match tools::run_tool(&root, "read_file", args).await {
            Ok(data) => {
                let path = data.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let bytes = data.get("bytes").and_then(|v| v.as_i64()).unwrap_or(0);
                // content 截断到 12000
                let mut d = data.clone();
                if let Some(c) = d.get("content").and_then(|v| v.as_str()) {
                    let t: String = c.chars().take(12000).collect();
                    d.as_object_mut().unwrap().insert("content".to_string(), json!(t));
                }
                send!(event("tool_result", json!({ "id": id, "status": "ok", "summary": format!("读取 {path}，{bytes} bytes。"), "data": d })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    } else if let Some(args) = decision.get("search").filter(|v| v.is_object()) {
        let id = now_id("tool");
        send!(event("tool_call", json!({ "id": id, "name": "search_files", "args": args })));
        match tools::run_tool(&root, "search_files", args).await {
            Ok(data) => {
                let n = data.as_array().map(|a| a.len()).unwrap_or(0);
                let summary = if n > 0 { format!("搜索到 {n} 处匹配。") } else { "没有搜索到匹配项。".to_string() };
                send!(event("tool_result", json!({ "id": id, "status": "ok", "summary": summary, "data": data })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    }
    if decision.get("wantsValidate").and_then(|v| v.as_bool()).unwrap_or(false) {
        let id = now_id("tool");
        let args = json!({});
        send!(event("tool_call", json!({ "id": id, "name": "validate_metadata", "args": args })));
        match tools::run_tool(&root, "validate_metadata", &args).await {
            Ok(data) => {
                let checked = data.get("checked").and_then(|v| v.as_i64()).unwrap_or(0);
                let errs = data.get("errors").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                let status = if errs > 0 { "warning" } else { "ok" };
                send!(event("tool_result", json!({ "id": id, "status": status, "summary": format!("检查 {checked} 个 JSON，错误 {errs} 个。"), "data": data })));
            }
            Err(e) => send!(event("tool_result", json!({ "id": id, "status": "error", "summary": e.to_string(), "data": null }))),
        }
    }

    // workflow + 总结
    let workflow_id = now_id("workflow");
    send!(event("workflow", json!({
        "id": workflow_id, "status": "running", "title": "生成回复",
        "steps": [{ "label": "收集上下文", "status": "done" }, { "label": "汇总结果", "status": "running" }]
    })));
    let stream_id = now_id("assistant");
    send!(event("assistant_start", json!({ "id": stream_id })));
    let summary = planner::build_summary(&emitted, context, messages).await;
    send!(event("assistant_done", json!({ "id": stream_id, "text": summary })));
    send!(event("workflow", json!({
        "id": workflow_id, "status": "done", "title": "生成回复",
        "steps": [{ "label": "收集上下文", "status": "done" }, { "label": "汇总结果", "status": "done" }]
    })));

    Ok(emitted)
}

/// 审批分支：生成补丁预览 + approval_required，并暂存 pending。
async fn build_approval_events(root: &std::path::Path, decision: &Value, context: &Value) -> PortalResult<Vec<Value>> {
    let action = decision.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let args = decision.get("args").cloned().unwrap_or(json!({}));
    let approval_id = now_id("approval");
    let mut preview: Option<Value> = None;
    let mut title = decision.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut risk = decision.get("risk").and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut approval_args = args.clone();

    if action == "apply_json_patch" {
        let p = tools::prepare_json_patch(root, &args).await?;
        title = title.or_else(|| Some(format!("修改 {}", p.get("path").and_then(|v| v.as_str()).unwrap_or(""))));
        approval_args = json!({ "path": p.get("path"), "pointer": p.get("pointer"), "value": p.get("value") });
        preview = Some(p);
    } else if action == "apply_text_replace" {
        let p = tools::prepare_text_replace(root, &args).await?;
        title = title.or_else(|| Some(format!("替换 {} 中的文本", p.get("path").and_then(|v| v.as_str()).unwrap_or(""))));
        let reps = p.get("replacements").and_then(|v| v.as_i64()).unwrap_or(0);
        risk = risk.or_else(|| Some(format!("审批通过后会写入项目文件；本次预计替换 {reps} 处。")));
        approval_args = json!({ "path": p.get("path"), "oldText": p.get("oldText"), "newText": p.get("newText"), "occurrence": p.get("occurrence") });
        preview = Some(p);
    }

    // 暂存
    pending().lock().await.insert(approval_id.clone(), PendingApproval {
        action: action.to_string(),
        args: args.clone(),
        created_at: now_ms(),
        context: context.clone(),
    });

    let mut events = Vec::new();
    events.push(event("assistant", json!({ "text": decision.get("intro").and_then(|v| v.as_str()).unwrap_or("这一步需要审批。") })));
    events.push(event("plan", json!({ "items": decision.get("plan").cloned().unwrap_or(json!([])) })));
    let preview_out = preview.as_ref().map(|p| {
        json!({
            "diff": p.get("diff"),
            "before": p.get("before").and_then(|v| v.as_str()).map(|s| s.chars().take(8000).collect::<String>()).unwrap_or_default(),
            "after": p.get("after").and_then(|v| v.as_str()).map(|s| s.chars().take(8000).collect::<String>()).unwrap_or_default(),
        })
    });
    let mut req = json!({
        "id": approval_id, "action": action, "title": title, "risk": risk,
        "args": approval_args, "expiresInMs": 10 * 60 * 1000,
    });
    if let Some(pv) = preview_out {
        req.as_object_mut().unwrap().insert("preview".to_string(), pv);
    }
    events.push(event("approval_required", req));
    events.push(event("assistant", json!({ "text": decision.get("outro").and_then(|v| v.as_str()).unwrap_or("请在审批卡片中选择同意或拒绝。") })));
    Ok(events)
}

fn tool_result_summary(action: &str, data: &Value) -> String {
    match action {
        "run_command" => format!("{} 退出码 {}", data.get("command").and_then(|v| v.as_str()).unwrap_or(""), data.get("exitCode").and_then(|v| v.as_i64()).unwrap_or(0)),
        "apply_json_patch" => format!("已修改 {} 的 {}", data.get("path").and_then(|v| v.as_str()).unwrap_or(""), data.get("pointer").and_then(|v| v.as_str()).unwrap_or("")),
        "apply_text_replace" => format!("已修改 {}，替换 {} 处", data.get("path").and_then(|v| v.as_str()).unwrap_or(""), data.get("replacements").and_then(|v| v.as_i64()).unwrap_or(0)),
        _ => "工具执行完成".to_string(),
    }
}

fn tool_result_status(action: &str, data: &Value) -> &'static str {
    match action {
        "run_command" => if data.get("exitCode").and_then(|v| v.as_i64()) == Some(0) { "ok" } else { "error" },
        "apply_json_patch" | "apply_text_replace" => if data.get("path").is_some() { "ok" } else { "error" },
        _ => "ok",
    }
}

/// 生成 lint 验证审批（补丁应用后）。
fn create_lint_approval(approval_id: &str) -> Value {
    event("approval_required", json!({
        "id": approval_id, "action": "run_command",
        "title": "运行 lint 验证本次修改",
        "risk": "补丁已应用。建议运行只读 lint 检查，确认没有引入新的代码问题。",
        "args": { "command": "npm", "args": ["run", "lint", "-w", "cmx-portal-manager"] },
        "workflowStep": "verify", "expiresInMs": 10 * 60 * 1000,
    }))
}

/// 处理审批决定。
pub async fn handle_approval(id: &str, decision: &str) -> PortalResult<Value> {
    let root = root_dir();
    let approval = pending().lock().await.remove(id);
    let Some(approval) = approval else {
        return Err(PortalError::bad_request("审批请求不存在或已处理"));
    };
    if now_ms() - approval.created_at > 10 * 60 * 1000 {
        return Err(PortalError::bad_request("审批请求已过期"));
    }
    let decision = decision.to_lowercase();
    if decision != "approve" {
        return Ok(json!({ "events": [
            event("approval_decision", json!({ "id": id, "decision": "reject" })),
            event("assistant", json!({ "text": "已拒绝执行，本轮不会运行该命令。" })),
        ] }));
    }

    let tool_id = now_id("tool");
    let mut events = vec![
        event("approval_decision", json!({ "id": id, "decision": "approve" })),
        event("tool_call", json!({ "id": tool_id, "name": approval.action, "args": approval.args })),
    ];
    let data = tools::run_tool(&root, &approval.action, &approval.args).await?;
    let status = tool_result_status(&approval.action, &data);
    let is_patch = approval.action == "apply_json_patch" || approval.action == "apply_text_replace";
    events.push(event("tool_result", json!({ "id": tool_id, "status": status, "summary": tool_result_summary(&approval.action, &data), "data": data })));

    if is_patch && status == "ok" {
        events.push(event("workflow", json!({
            "status": "waiting", "title": "修改已应用，等待验证",
            "steps": [
                { "label": "生成补丁", "status": "done" },
                { "label": "应用补丁", "status": "done" },
                { "label": "运行 lint", "status": "waiting" }
            ]
        })));
        // 暂存 lint 审批
        let lint_id = now_id("approval");
        pending().lock().await.insert(lint_id.clone(), PendingApproval {
            action: "run_command".to_string(),
            args: json!({ "command": "npm", "args": ["run", "lint", "-w", "cmx-portal-manager"] }),
            created_at: now_ms(),
            context: approval.context.clone(),
        });
        events.push(create_lint_approval(&lint_id));
    }
    events.push(event("assistant", json!({
        "text": if is_patch {
            if status == "ok" { "补丁已应用。我已生成 lint 验证审批，建议继续确认运行。" } else { "补丁应用失败，详情里有错误信息。" }
        } else if status == "ok" { "命令执行完成，结果已回填到详情面板。" } else { "命令执行失败，详情里有 stdout/stderr，可继续让我分析。" }
    })));
    Ok(json!({ "events": events }))
}

/// capabilities 响应。
pub fn capabilities() -> Value {
    let llm = std::env::var("CMX_AGENT_PLANNER").ok().as_deref() == Some("llm");
    json!({
        "mode": "local-edit-approval",
        "planner": if llm { "LlmPlanner" } else { "LocalRulePlanner" },
        "plannerEnabled": true,
        "root": root_dir().to_string_lossy(),
        "tools": schemas::public_tool_schemas(),
    })
}

/// /agent/message：一次性返回所有事件。
pub async fn message(body: &Value) -> PortalResult<Value> {
    let messages = normalize_messages(body.get("messages").unwrap_or(&Value::Null));
    let context = body.get("context").filter(|v| v.is_object()).cloned().unwrap_or(json!({}));
    let events = run_agent_flow(&messages, &context, |_| {}).await?;
    let conv_id = body.get("conversationId").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| now_id("conv"));
    Ok(json!({ "conversationId": conv_id, "events": events }))
}
