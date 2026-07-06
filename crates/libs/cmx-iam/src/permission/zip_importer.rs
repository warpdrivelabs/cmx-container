//! `PermissionZipImporter` trait 的实现。
//!
//! 桥接 `PermissionServiceImpl` 的固有方法 `import_permissions` / `cleanup_permissions`
//! 到 trait 接口,使 `ResourceDataImporterImpl`(cmx-biz)可通过 trait 对象持有权限服务,
//! 无需直接依赖 cmx-iam。

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;
use cmx_traits::resource::PermissionZipImporter;
use cmx_traits::resource::ResourceDataImportResult;

use crate::permission::service::PermissionServiceImpl;

#[async_trait]
impl PermissionZipImporter for PermissionServiceImpl {
    async fn import_permissions_zip(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        zip_data: &[u8],
    ) -> Result<ResourceDataImportResult, TraitError> {
        let svr_ctx = SVRContext::new(
            serde_json::Value::Null,
            std::collections::HashMap::new(),
            chrono::Utc::now(),
            cmx_utils::id::snowflake_id_str(),
        );
        self.import_permissions(&svr_ctx, domain_code, app_code, module_code, zip_data)
            .await
    }

    async fn cleanup_permissions_zip(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<ResourceDataImportResult, TraitError> {
        let svr_ctx = SVRContext::new(
            serde_json::Value::Null,
            std::collections::HashMap::new(),
            chrono::Utc::now(),
            cmx_utils::id::snowflake_id_str(),
        );
        self.cleanup_permissions(&svr_ctx, domain_code, app_code, module_code)
            .await
    }
}
