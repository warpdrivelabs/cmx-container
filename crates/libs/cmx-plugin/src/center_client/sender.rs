//! 服务中心数据发送器 trait 定义。

use super::types::{CenterCleanupRequest, CenterResponse, CenterSendRequest};
use async_trait::async_trait;

/// 服务中心调用错误类型。
#[derive(Debug, thiserror::Error)]
pub enum CenterError {
    /// 中心接口调用失败。
    #[error("{center}调用失败: {message}")]
    CallFailed { center: String, message: String },
    /// 中心服务不可用。
    #[error("{center}不可用: {url}")]
    Unavailable { center: String, url: String },
    /// 数据打包失败。
    #[error("数据打包失败: {0}")]
    PackError(String),
    /// 配置错误。
    #[error("配置错误: {0}")]
    Config(String),
    /// 网络错误。
    #[error("网络错误: {0}")]
    Network(String),
    /// 响应超时。
    #[error("超时: {center} 响应超时 ({timeout_ms}ms)")]
    Timeout { center: String, timeout_ms: u64 },
}

/// 服务中心数据发送器 trait。
///
/// 定义向外部基础服务中心推送/清理数据的统一接口。
/// 当前使用 `MockServiceCenterSender` 实现，后续替换为 HTTP form-data 实现即可对接真实服务。
#[async_trait]
pub trait ServiceCenterSender: Send + Sync {
    /// 发送数据到服务中心。
    ///
    /// 以 form-data 方式发送 ZIP 文件（包含整个数据目录的内容）。
    ///
    /// # Errors
    ///
    /// 当网络错误、中心不可用或响应超时时返回 `CenterError`。
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError>;

    /// 清理服务中心中与指定插件相关的数据。
    ///
    /// # Errors
    ///
    /// 当网络错误、中心不可用或响应超时时返回 `CenterError`。
    async fn cleanup_data(
        &self,
        request: CenterCleanupRequest,
    ) -> Result<CenterResponse, CenterError>;
}
