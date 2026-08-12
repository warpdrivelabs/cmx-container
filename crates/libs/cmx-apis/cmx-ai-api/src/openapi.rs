//! cmx-ai-api 的 OpenApi 切片。
//!
//! 从 cmx-api/openapi.rs 迁入的 AI 相关 paths + schemas，由 platform-app 用
//! `OpenApi::merge()` 聚合到总文档。

use utoipa::OpenApi;

/// AI 中继模块 OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handler::create_session,
        crate::handler::send_message,
        crate::handler::answer_question,
        crate::handler::approve,
        crate::handler::abort_session,
        crate::handler::delete_session,
        crate::handler::subscribe_events,
    ),
    components(
        schemas(
            cmx_ai::types::CreateSessionReq,
            cmx_ai::types::TextPartInput,
            cmx_ai::types::SendMessageReq,
            cmx_ai::types::AnswerReq,
            cmx_ai::types::ApprovalReq,
            cmx_ai::types::ApprovalDecision,
            cmx_ai::types::SessionInfo,
            cmx_ai::types::TextDeltaEvent,
            cmx_ai::types::ReasoningDeltaEvent,
            cmx_ai::types::ToolCallEvent,
            cmx_ai::types::AskUserEvent,
            cmx_ai::types::AskUserQuestion,
            cmx_ai::types::AskUserOption,
            cmx_ai::types::RequireApprovalEvent,
            cmx_ai::types::ApprovalDiff,
            cmx_ai::types::ResultEvent,
            cmx_ai::types::ResultValidation,
            cmx_ai::types::JsonChunkEvent,
            cmx_ai::types::ErrorEvent,
            cmx_ai::types::DoneEvent,
            cmx_api_types::ApiResp<cmx_ai::types::SessionInfo>,
        )
    )
)]
pub struct AiApiDoc;
