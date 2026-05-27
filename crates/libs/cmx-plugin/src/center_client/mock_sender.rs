//! Mock 服务中心发送器。
//!
//! 当前阶段的 Mock 实现，模拟向各中心发送数据和清理数据的接口调用，
//! 始终返回成功结果。后续替换为 `HttpServiceCenterSender` 即可对接真实服务。

use async_trait::async_trait;
use super::sender::{CenterError, ServiceCenterSender};
use super::types::*;

/// Mock 服务中心发送器。
///
/// 所有接口调用均返回成功结果，并输出日志记录调用信息。
pub struct MockServiceCenterSender;

#[async_trait]
impl ServiceCenterSender for MockServiceCenterSender {
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError> {
        tracing::info!(
            "[Mock] 向{}推送数据: plugin={}, zip={}, size={}bytes",
            request.category.center_name(),
            request.plugin_id,
            request.zip_file_name,
            request.zip_data.len(),
        );
        Ok(CenterResponse {
            success: true,
            message: format!("Mock: {}数据接收成功", request.category.center_name()),
            center_id: Some(format!("mock-{}", request.category.dir_name())),
        })
    }

    async fn cleanup_data(
        &self,
        request: CenterCleanupRequest,
    ) -> Result<CenterResponse, CenterError> {
        tracing::info!(
            "[Mock] 通知{}清理数据: plugin={}, app={}",
            request.category.center_name(),
            request.plugin_id,
            request.app_id,
        );
        Ok(CenterResponse {
            success: true,
            message: format!("Mock: {}数据清理成功", request.category.center_name()),
            center_id: None,
        })
    }
}
