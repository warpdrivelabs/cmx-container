//! 权限定义导入器 trait + ZIP 格式权限导入/清理 trait。
//!
//! - [`PermissionDefinitionImporter`]:结构化定义列表的 apply/list(模块导入/导出用)
//! - [`PermissionZipImporter`]:ZIP 格式权限数据的导入/清理(插件数据中心远程接收用)

use async_trait::async_trait;
use cmx_core::model::iam::PermissionDefinition;

use crate::error::TraitError;
use crate::plugin::{PluginDataImportResult};

/// 权限定义导入器(本地/远程统一接口)。
///
/// 实现方负责两阶段 upsert:
/// 1. 第一阶段:按 code upsert(parent_id 暂置 NULL,full_code_path = '/' + code)
/// 2. 第二阶段:回填 parent_id / parent_code / full_code_path / level,父节点 is_leaf = 0
#[async_trait]
pub trait PermissionDefinitionImporter: Send + Sync {
    /// 将权限定义列表 upsert 到指定作用域。
    async fn apply_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[PermissionDefinition],
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有权限定义(重建 parent_code)。
    async fn list_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<PermissionDefinition>, TraitError>;
}

/// ZIP 格式权限导入/清理(插件数据中心远程接收端用)。
///
/// 封装 `PermissionServiceImpl` 的固有方法 `import_permissions` / `cleanup_permissions`,
/// 使 `PluginDataImporterImpl`(cmx-biz)可通过 trait 对象持有,无需依赖 cmx-iam。
///
/// 与 [`PermissionDefinitionImporter`] 的区别:
/// - 本 trait 面向 **ZIP permdata 格式**(插件包 `permdata/*.zip`),含 diff/审计/缓存失效
/// - `PermissionDefinitionImporter` 面向 **结构化 `&[PermissionDefinition]`**(模块包 JSON)
#[async_trait]
pub trait PermissionZipImporter: Send + Sync {
    /// 从 ZIP 数据导入权限(解压→解析→diff→事务 upsert→审计→缓存失效)。
    async fn import_permissions_zip(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        zip_data: &[u8],
    ) -> Result<PluginDataImportResult, TraitError>;

    /// 清理指定作用域下的所有权限及其角色关联(物理删除)。
    async fn cleanup_permissions_zip(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<PluginDataImportResult, TraitError>;
}
