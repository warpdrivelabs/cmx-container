//! AI 中继层对前端暴露的请求/响应与 SSE 事件类型（前端契约）。
//!
//! 命名遵循 `#[serde(rename_all = "camelCase")]`，与项目 `ApiResp` 风格一致。
//! 所有类型派生 [`utoipa::ToSchema`]，供 Swagger UI 展示前端可直接消费的契约。
//!
//! # SSE 事件协议
//! cmx-ai 把 OpenCode 原生事件（`message.part.delta` / `session.status` /
//! `question.v2.asked` 等）翻译为下表简化事件，经 `GET /api/ai/events` 推送：
//!
//! | cmx-ai 事件 | 来源 OpenCode 事件 |
//! |------|------|
//! | `text_delta` | `message.part.delta`（`field:"text"` 的 `delta`）|
//! | `reasoning_delta` | `message.part.delta`（`field:"reasoning"` 的 `delta`）|
//! | `tool_call` | `message.part.updated`（`part.type=="tool"` 的状态变更）|
//! | `json_chunk` | cmx-ai 从 `message.part.delta` 识别 ```json 围栏或裸 JSON 边界后切分 |
//! | `ask_user` | `question.v2.asked` |
//! | `require_approval` | `permission.v2.asked` |
//! | `result` | `session.status`（`status.type=="idle"`）后，从累积文本提取产物 |
//! | `error` | `session.status`（`status.type=="error"`）/ 连接错误 |
//! | `done` | result/abort 后的收尾标志 |

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ───────────────────────── 请求 DTO ─────────────────────────

/// 创建会话请求。
///
/// 一期为空对象（透传给 OpenCode `POST /session`）；二期可扩展 title 等字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct CreateSessionReq {
    /// 会话标题（可选，二期用；一期 OpenCode 自动生成）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// 发送消息请求体中的一个文本片段（对应 OpenCode `TextPartInput`）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TextPartInput {
    /// 固定 `"text"`。
    #[serde(rename = "type")]
    pub part_type: String,
    /// 文本内容（用户的自然语言需求）。
    pub text: String,
}

/// 发送消息请求。
///
/// `parts` 为必填，每项是 [`TextPartInput`]；多轮对话无需手动拼接历史，OpenCode 自动带上下文。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendMessageReq {
    /// 消息片段列表。
    pub parts: Vec<TextPartInput>,
}

/// 回答 AI 询问请求。
///
/// `answers` 为「按问题顺序、每问题一个被选 label 数组」的二维结构（OpenCode `QuestionV2Reply` 要求）。
/// 单选时内层数组也只有一个元素。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AnswerReq {
    /// 待回答的询问 id（OpenCode `que_*`，来自 `ask_user` 事件）。
    pub question_id: String,
    /// 答案二维数组。
    pub answers: Vec<Vec<String>>,
}

/// 审批决策请求。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApprovalReq {
    /// 待审批的请求 id（OpenCode `per_*`，来自 `require_approval` 事件）。
    pub approval_id: String,
    /// 决策：`approve`（同意）/ `reject`（拒绝）。
    pub decision: ApprovalDecision,
    /// 备注（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// 审批决策枚举。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    /// 同意（映射 OpenCode `reply:"once"`）。
    Approve,
    /// 拒绝（映射 OpenCode `reply:"reject"`）。
    Reject,
}

// ───────────────────────── 响应 DTO ─────────────────────────

/// 会话信息（`POST /api/ai/sessions` 响应）。
///
/// 一期 `session_id` 直接透传 OpenCode 的 `ses_*`；`title`/`created_at` 预留（二期填充）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// 会话 id（OpenCode `ses_*`）。
    pub session_id: String,
    /// 会话标题（一期可能为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 创建时间（Unix 毫秒，一期可能为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

// ───────────────────────── SSE 前端事件载荷 ─────────────────────────
//
// 每个 XxxEvent 对应一类 SSE 事件，event 字段即类型名（snake_case），
// data 字段是该结构序列化后的 JSON。前端用 EventSource.addEventListener("text_delta", ...)。

/// `text_delta` 事件：AI 回复/解释的流式文本片段。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TextDeltaEvent {
    /// 本次增量文本。
    pub content: String,
}

/// `reasoning_delta` 事件：推理过程片段（若模型输出）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReasoningDeltaEvent {
    /// 本次增量推理文本。
    pub content: String,
}

/// `tool_call` 事件：工具调用进度。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ToolCallEvent {
    /// 工具名称（如 `generate_html_page`）。
    pub tool: String,
    /// Part ID（opencode message part 的唯一标识，用于前端区分多个同名工具调用）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part_id: String,
    /// 调用状态：`running` / `completed` / `failed`。
    pub state: String,
    /// 工具输入参数（completed 时携带；question 工具的 questions 列表在此）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// 工具输出（completed 时的结果文本，如有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// 工具元数据（question 工具的 answers 在此：`{ answers: string[][] }`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// serde 默认值辅助：custom 字段缺省为 true（与 OpenCode V1 一致：默认允许自定义答案）。
fn default_true() -> bool {
    true
}

/// `ask_user` 事件：弹出询问卡片，用户回答后继续生成。
///
/// 一次询问可携带多个问题（对齐 OpenCode：question 工具一次 ask 携带 questions 数组，
/// 用户一次性回答所有问题，回复 answers 为二维数组 `[[ans1...],[ans2...],...]`）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskUserEvent {
    /// 询问 id（OpenCode `que_*`），回答时带上。
    pub question_id: String,
    /// 问题列表（至少一个；多个时前端按分区/标签逐一呈现，统一提交）。
    pub questions: Vec<AskUserQuestion>,
}

/// 单个询问问题（ask_user 事件的一项）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestion {
    /// 询问类型：`single_choice`（单选）/ `multi_choice`（多选）/ `text`（自由输入）。
    #[serde(rename = "type")]
    pub question_type: String,
    /// 标题（简短，≤30 字）。
    pub title: String,
    /// 完整问题描述。
    pub message: String,
    /// 是否允许多选。
    pub multiple: bool,
    /// 是否允许自定义文本答案（与选项并存）。默认 true（OpenCode V1 语义）。
    #[serde(default = "default_true")]
    pub custom: bool,
    /// 可选项列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AskUserOption>,
}

/// 询问可选项。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AskUserOption {
    /// 选项标签（用户选择后作为 answers 回传）。
    pub label: String,
    /// 选项描述（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `require_approval` 事件：审批窗口，展示变更待确认/拒绝。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequireApprovalEvent {
    /// 审批请求 id（OpenCode `per_*`），回复时带上。
    pub approval_id: String,
    /// 操作类型（来自 OpenCode `action`）。
    pub action: String,
    /// 标题。
    pub title: String,
    /// 人话描述。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 变更前后对比（一期可能为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<ApprovalDiff>,
}

/// 审批变更对比。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApprovalDiff {
    /// 变更前（JSON 对象）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    /// 变更后（JSON 对象）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
}

/// `result` 事件：最终完整结果（HTML 或 JSON）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResultEvent {
    /// 结果类型：`html_page_result` / `dct_result` / `doc_result`。
    #[serde(rename = "type")]
    pub result_type: String,
    /// 结果内容（HTML 页面源码 / JSON 字符串）。
    pub data: String,
    /// 校验结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ResultValidation>,
    /// 结果摘要（人话）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// 是否可保存（一期固定 `false`，二期置 `true` 时前端显示「保存」按钮）。
    #[serde(default)]
    pub saveable: bool,
    /// 产物类型：`html` / `dct` / `doc`（一期固定 `html`，二期预留）。
    #[serde(default)]
    pub product_type: String,
}

/// 结果校验信息。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResultValidation {
    /// 是否通过。
    pub passed: bool,
    /// 校验消息（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// `error` 事件：异常信息。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorEvent {
    /// 错误消息。
    pub message: String,
    /// 错误码（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<u16>,
}

/// `json_chunk` 事件：渐进 JSON 片段（DCT/DOC 等结构化产物）。
///
/// 当 AI 正在输出 JSON 产物时，cmx-ai 识别到 JSON 边界（如 ```` ```json ```` 围栏或连续
/// `{`/`[`），把累积的 JSON 片段切分为本事件，供前端实时拼装预览（字段逐步出现）。
/// 最终完整结果仍由 `result` 事件下发。
///
/// 详见任务文档 4.3 节。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JsonChunkEvent {
    /// 片段序号（从 0 开始，每次切分自增）。
    pub chunk_index: u32,
    /// 当前片段在最终 JSON 中的位置提示（可选，cmx-ai 推断；如 `fields[2]`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 本次输出的 JSON 片段内容。
    pub content: String,
    /// 预估总片段数（可选，用于前端进度指示）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_hint: Option<u32>,
}

/// `done` 事件：本轮流结束标志（无载荷，空对象）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct DoneEvent {}

// ───────────────────────── 隐式上下文回传 ─────────────────────────
//
// 插件工具（如 GetCurrentPage）需要前端当前页面信息时，经 cmx 后端桥接：
// 工具 → POST context-request（挂起）→ 后端 broadcast SSE context_request
//      → 前端自动收集 → POST context-response → 后端 resolve → 工具解除挂起。
// 全程无询问框，对用户透明。

/// `POST /api/ai/sessions/{sid}/context-request` 请求体（插件工具发起）。
///
/// 工具发起一次隐式上下文请求：后端广播 `context_request` SSE 给前端，
/// 前端自动收集当前页面信息后回传，工具解除挂起。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextRequestReq {
    /// 请求 id（插件生成，如 `ctx_*`），用于匹配响应。
    pub request_id: String,
    /// 期望获取的信息类型（如 `["menuId","htmlPage"]`）；为空则前端返回全部可用信息。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub want: Vec<String>,
}

/// `context_request` SSE 事件载荷（前端据此自动收集并回传）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextRequestEvent {
    /// 请求 id（回传时带上以匹配）。
    pub request_id: String,
    /// 期望的信息类型。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub want: Vec<String>,
}

/// `POST /api/ai/sessions/{sid}/context-response` 请求体（前端回传）。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContextResponseReq {
    /// 对应的请求 id。
    pub request_id: String,
    /// 前端收集到的当前页面信息（自由结构：menuId/menuLabel/htmlPage 等）。
    pub data: serde_json::Value,
}
