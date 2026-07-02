//! 插件数据导入器实现 — `PluginDataImporterImpl`。
//!
//! 实现 `cmx_traits::plugin::PluginDataImporter` trait，按 `PluginDataCategory`
//! 路由到对应的 Service 固有方法。当前仅支持 `Perm`（权限）类别。
//!
//! HTTP 端点和 gRPC 服务端均通过此 trait 调用，统一路径与缓存失效逻辑。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;
use cmx_traits::plugin::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult,
    PluginDataImporter,
};

use super::service::PermissionServiceImpl;

/// 插件数据导入器实现。
///
/// 持有 `PermissionServiceImpl` 的具体类型引用（非 trait 对象），
/// 以便调用其固有方法 `import_permissions` / `cleanup_permissions`。
pub struct PluginDataImporterImpl {
    /// 权限服务实现（具体类型）。
    permission_service: Arc<PermissionServiceImpl>,
}

impl PluginDataImporterImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `permission_service` - 权限服务实现的具体类型实例。
    ///
    /// # Returns
    ///
    /// 返回新的 `PluginDataImporterImpl` 实例。
    pub fn new(permission_service: Arc<PermissionServiceImpl>) -> Self {
        Self { permission_service }
    }

    /// 构造 `SVRContext`（系统调用上下文，无 HTTP 请求头信息）。
    ///
    /// 插件导入流程通常由 gRPC/内部调用触发，无完整 HTTP 上下文，
    /// 此处构造最小化的 `SVRContext` 供审计日志使用。
    fn build_svr_ctx() -> SVRContext {
        SVRContext::new(
            serde_json::Value::Null,
            HashMap::new(),
            Utc::now(),
            cmx_utils::id::snowflake_id_str(),
        )
    }
}

#[async_trait]
impl PluginDataImporter for PluginDataImporterImpl {
    /// 导入插件数据。
    ///
    /// 按 `PluginDataCategory` 路由：
    /// - `Perm` → 调用 `PermissionServiceImpl::import_permissions`
    /// - 其他类别 → 返回不支持错误
    async fn import_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, TraitError> {
        match request.category {
            PluginDataCategory::Perm => {
                let svr_ctx = Self::build_svr_ctx();
                self.permission_service
                    .import_permissions(
                        &svr_ctx,
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &request.zip_data,
                    )
                    .await
            }
            _ => Err(TraitError::Business(format!(
                "不支持的数据类别: {:?}",
                request.category
            ))),
        }
    }

    /// 清理插件数据。
    ///
    /// 按 `PluginDataCategory` 路由：
    /// - `Perm` → 调用 `PermissionServiceImpl::cleanup_permissions`
    /// - 其他类别 → 返回不支持错误
    async fn cleanup_data(
        &self,
        request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, TraitError> {
        match request.category {
            PluginDataCategory::Perm => {
                let svr_ctx = Self::build_svr_ctx();
                self.permission_service
                    .cleanup_permissions(
                        &svr_ctx,
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                    )
                    .await
            }
            _ => Err(TraitError::Business(format!(
                "不支持的数据类别: {:?}",
                request.category
            ))),
        }
    }
}
