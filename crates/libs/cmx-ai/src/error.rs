//! AI 中继层错误类型。
//!
//! 遵循 [AGENTS.md](../../../../AGENTS.md) 规范：使用 `thiserror` 派生，禁止手写 `impl Display/Error`。
//! 通过 `impl From<AiError> for cmx_api_types::Error` 让 handler 层可直接用 `?` 传播。

use thiserror::Error;

/// AI 中继层统一错误。
#[derive(Debug, Error)]
pub enum AiError {
    /// 配置错误（如 base_url 非法、必填项缺失）。
    #[error("AI 配置错误: {0}")]
    Config(String),

    /// OpenCode 上游返回业务错误（响应体解析后的错误消息）。
    #[error("OpenCode 上游错误: {0}")]
    Upstream(String),

    /// OpenCode 上游返回非 2xx 状态码。
    #[error("OpenCode 上游返回状态码 {0}: {1}")]
    UpstreamStatus(u16, String),

    /// 请求 OpenCode 超时。
    #[error("请求 OpenCode 超时")]
    Timeout,

    /// OpenCode 服务未配置（base_url 为空）。
    #[error("AI 服务未配置：请在 [opencode] 段或 OPENCODE_BASE_URL 环境变量中设置 base_url")]
    NotConfigured,

    /// 会话不存在或已失效（如 sid 不符合 ses_* 格式、已被删除）。
    #[error("AI 会话无效: {0}")]
    InvalidSession(String),

    /// 无待处理的询问/审批（前端重复回答或超时已被自动拒绝）。
    #[error("无待处理的询问/审批请求: {0}")]
    NoPendingRequest(String),

    /// mpsc 通道已关闭（订阅端断开）。
    #[error("SSE 订阅通道已关闭")]
    ChannelClosed,

    /// JSON 序列化/反序列化失败。
    #[error("JSON 解析失败: {0}")]
    Serde(#[from] serde_json::Error),

    /// reqwest HTTP 请求失败（连接拒绝、DNS 解析失败等网络层错误）。
    #[error("HTTP 请求失败: {0}")]
    Http(#[from] reqwest::Error),

    /// URL 解析/拼接失败。
    #[error("URL 解析失败: {0}")]
    Url(#[from] url::ParseError),

    /// 其它内部错误。
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// AI 中继层 Result 别名。
pub type AiResult<T> = core::result::Result<T, AiError>;

impl AiError {
    /// 构造配置错误。
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// 构造上游错误。
    pub fn upstream(msg: impl Into<String>) -> Self {
        Self::Upstream(msg.into())
    }

    /// 构造无效会话错误。
    pub fn invalid_session(msg: impl Into<String>) -> Self {
        Self::InvalidSession(msg.into())
    }

    /// 构造无待处理请求错误。
    pub fn no_pending(msg: impl Into<String>) -> Self {
        Self::NoPendingRequest(msg.into())
    }
}

/// 把 AI 中继层错误映射为 API 层统一错误，使 handler 可直接 `?` 传播。
///
/// 映射策略（参照 cmx-portal-base::PortalError，配合 [`cmx_api_types::Error`] 现有变体）：
/// - [`AiError::InvalidSession`] / [`AiError::NoPendingRequest`] → `NotFound`（HTTP 404）
/// - [`AiError::Timeout`] → `Timeout`（HTTP 504）
/// - [`AiError::NotConfigured`] → `BusinessError`（HTTP 200 + 业务码 1，前端按 `code` 判断未配置；
///   如需 501 语义，handler 入口可改返回 `ApiResp::fail(501, ...)`）
/// - 其它 → `InternalError`（HTTP 500）
impl From<AiError> for cmx_api_types::Error {
    fn from(err: AiError) -> Self {
        match err {
            AiError::InvalidSession(msg) | AiError::NoPendingRequest(msg) => {
                cmx_api_types::Error::not_found(msg)
            }
            AiError::Timeout => cmx_api_types::Error::Timeout,
            AiError::NotConfigured => cmx_api_types::Error::business_error(err.to_string()),
            other => cmx_api_types::Error::internal_error(other.to_string()),
        }
    }
}
