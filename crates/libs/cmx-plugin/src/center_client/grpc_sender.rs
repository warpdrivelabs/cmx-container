//! gRPC 服务中心 Sender 实现。
//!
//! 通过 `GlobalRpcClient` 调用远程 gRPC 服务，复用 cmx-rpc 的服务发现和负载均衡能力。
//! 适用于 `mode = "grpc"` 模式。
//!
//! 服务名路由：根据 `DataCategory` 从 `CenterClientConfig.discovery` 中获取对应的服务名
//! （如 `perm_service = "cmx-perm-center"`），传给 `RpcClient` 用于服务发现。

use async_trait::async_trait;
use tracing::info;

use cmx_rpc::GlobalRpcClient;
use cmx_traits::plugin::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest,
};

use super::config::CenterClientConfig;
use super::sender::{CenterError, ServiceCenterSender};
use super::types::{CenterCleanupRequest, CenterSendRequest, CenterResponse, DataCategory};

/// gRPC 服务中心 Sender。
///
/// 持有 `CenterClientConfig` 以便根据 `DataCategory` 解析目标服务名。
pub struct GrpcServiceCenterSender {
    /// 服务中心客户端配置（用于解析 discovery 中的服务名）。
    config: CenterClientConfig,
}

impl GrpcServiceCenterSender {
    /// 创建新的 gRPC Sender。
    pub fn new(config: CenterClientConfig) -> Self {
        Self { config }
    }

    /// 根据 `DataCategory` 解析目标服务名。
    ///
    /// 从 `CenterClientConfig.discovery` 中获取对应的服务名配置，
    /// 如 `perm_service`、`menu_service` 等。
    /// 未配置时返回 `CenterError::Config`。
    fn resolve_service_name(&self, category: DataCategory) -> Result<&str, CenterError> {
        self.config.discovery.get_service_name(category).ok_or_else(|| {
            CenterError::Config(format!(
                "{} 服务名未配置（需在 [center_client.discovery] 中配置 {}_service）",
                category.center_name(),
                category.dir_name().trim_end_matches("data")
            ))
        })
    }
}

/// 将 `DataCategory` 转换为 `PluginDataCategory`。
fn to_plugin_category(category: DataCategory) -> PluginDataCategory {
    match category {
        DataCategory::Menu => PluginDataCategory::Menu,
        DataCategory::Perm => PluginDataCategory::Perm,
        DataCategory::Form => PluginDataCategory::Form,
        DataCategory::Flow => PluginDataCategory::Flow,
    }
}

#[async_trait]
impl ServiceCenterSender for GrpcServiceCenterSender {
    async fn send_data(
        &self,
        request: CenterSendRequest,
    ) -> Result<CenterResponse, CenterError> {
        if !GlobalRpcClient::is_initialized() {
            return Err(CenterError::Unavailable {
                center: request.category.center_name().to_string(),
                url: "grpc://GlobalRpcClient".to_string(),
            });
        }

        let category = request.category;
        let service_name = self.resolve_service_name(category)?;

        info!(
            target: "cmx_plugin_center",
            category = %category.center_name(),
            service_name = %service_name,
            plugin_id = %request.plugin_id,
            zip_size = request.zip_data.len(),
            "gRPC 发送数据到服务中心"
        );

        let import_request = PluginDataImportRequest {
            category: to_plugin_category(category),
            domain_code: request.domain_code,
            application_code: request.application_code,
            module_code: request.module_code,
            plugin_id: request.plugin_id,
            app_id: request.app_id,
            version: request.version,
            zip_data: request.zip_data,
        };

        let result = cmx_rpc::plugin_data_client()
            .import_plugin_data(service_name, import_request)
            .await
            .map_err(|e| CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: e.to_string(),
            })?;

        if !result.success {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: result.message,
            });
        }

        Ok(CenterResponse {
            success: true,
            message: result.message,
            center_id: None,
        })
    }

    async fn cleanup_data(
        &self,
        request: CenterCleanupRequest,
    ) -> Result<CenterResponse, CenterError> {
        if !GlobalRpcClient::is_initialized() {
            return Err(CenterError::Unavailable {
                center: request.category.center_name().to_string(),
                url: "grpc://GlobalRpcClient".to_string(),
            });
        }

        let category = request.category;
        let service_name = self.resolve_service_name(category)?;

        info!(
            target: "cmx_plugin_center",
            category = %category.center_name(),
            service_name = %service_name,
            plugin_id = %request.plugin_id,
            "gRPC 清理服务中心数据"
        );

        let cleanup_request = PluginDataCleanupRequest {
            category: to_plugin_category(category),
            domain_code: request.domain_code,
            application_code: request.application_code,
            module_code: request.module_code,
            plugin_id: request.plugin_id,
            app_id: request.app_id,
        };

        let result = cmx_rpc::plugin_data_client()
            .cleanup_plugin_data(service_name, cleanup_request)
            .await
            .map_err(|e| CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: e.to_string(),
            })?;

        if !result.success {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: result.message,
            });
        }

        Ok(CenterResponse {
            success: true,
            message: result.message,
            center_id: None,
        })
    }
}
