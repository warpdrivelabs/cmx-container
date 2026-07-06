//! 资源数据管理 RPC 客户端 trait。
//!
//! 对应 gRPC 服务 `CmxResourceDataService`，负责跨实例的资源数据导入和清理。

use async_trait::async_trait;

use crate::resource::{
    ResourceDataCleanupRequest, ResourceDataImportRequest, ResourceDataImportResult,
    ResourceDataListResult,
};
use crate::rpc::error::RpcError;

/// 资源数据管理 RPC 客户端接口（策略模式 — 策略接口）。
///
/// 对应 gRPC 服务 `CmxResourceDataService`，负责跨实例的资源数据导入和清理。
#[async_trait]
pub trait ResourceDataClient: Send + Sync {
    /// 导入资源数据到远程服务（gRPC 专用，HTTP 端点不使用此方法）。
    ///
    /// 将 ZIP 数据通过 gRPC 发送到远程实例的 `ImportResourceData` 方法。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 目标服务在注册中心的服务名（如 `cmx-perm-center`），
    ///   用于服务发现。与 `request.app_id`（插件应用 ID）不同。
    /// * `request` - 资源数据导入请求。
    ///
    /// # Errors
    ///
    /// * [`RpcError::NoAvailableInstance`] - 无可用实例。
    /// * [`RpcError::RpcCallFailed`] - RPC 调用失败。
    async fn import_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataImportResult, RpcError>;

    /// 清理远程服务中的资源数据（gRPC 专用）。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 目标服务在注册中心的服务名。
    /// * `request` - 资源数据清理请求。
    ///
    /// # Errors
    ///
    /// * [`RpcError::NoAvailableInstance`] - 无可用实例。
    /// * [`RpcError::RpcCallFailed`] - RPC 调用失败。
    async fn cleanup_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataCleanupRequest,
    ) -> Result<ResourceDataImportResult, RpcError>;

    /// 查询（导出）远程服务中的资源数据。
    ///
    /// 通过 gRPC 调用远程实例的 `ListResourceData` 方法，返回 JSON 序列化的定义列表。
    ///
    /// # Arguments
    ///
    /// * `service_name` - 目标服务在注册中心的服务名。
    /// * `request` - 查询请求（category + domain/app/module 作用域）。
    async fn list_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataListResult, RpcError>;
}
