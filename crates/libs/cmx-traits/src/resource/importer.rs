//! 资源数据导入器 trait 定义。
//!
//! 定义将平台资源数据（权限、菜单、表单、流程）导入到基础服务中心的统一接口，
//! 供 cmx-rpc（gRPC 服务端）和 cmx-api（HTTP 端点）统一调用。

use async_trait::async_trait;

use crate::error::TraitError;
use crate::resource::{
    ResourceDataCleanupRequest, ResourceDataImportRequest, ResourceDataImportResult,
    ResourceDataListResult,
};

/// 资源数据导入器 trait。
///
/// 定义将资源数据导入到基础服务中心的统一接口。
/// cmx-biz 的 `ResourceDataImporterImpl` 实现此 trait，
/// HTTP 端点和 gRPC 服务端均通过此 trait 调用。
#[async_trait]
pub trait ResourceDataImporter: Send + Sync {
    /// 导入资源数据（解压 ZIP → 解析 → 比对 DB → 事务写入）。
    async fn import_data(
        &self,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataImportResult, TraitError>;

    /// 清理资源数据（按三元组物理删除所有匹配记录）。
    async fn cleanup_data(
        &self,
        request: ResourceDataCleanupRequest,
    ) -> Result<ResourceDataImportResult, TraitError>;

    /// 查询（导出）资源数据，返回 JSON 序列化的定义列表。
    ///
    /// 按 `request.category` 路由到对应资源的 `list_*` 方法，
    /// 返回序列化后的 JSON 字节（供远程导出场景复用）。
    async fn list_data(
        &self,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataListResult, TraitError>;
}
