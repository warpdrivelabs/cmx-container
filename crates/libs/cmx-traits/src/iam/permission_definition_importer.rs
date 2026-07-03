//! 权限定义导入器 trait。
//!
//! 定义将 `PermissionDefinition` 列表 upsert 到 `cmx_permission` 的统一接口,
//! 供 cmx-iam 实现、cmx-plugin(模块导入)消费,消除两阶段 upsert 逻辑的三处重复。
//!
//! 与 `PluginDataImporter` 的区别:本 trait 接收**已解析的结构体列表**,
//! 不含 ZIP 解压/校验/审计/缓存失效;适用于模块导入这种「磁盘 JSON → 直接 upsert」场景。
//! `PluginDataImporter` 接收 ZIP 字节流,面向插件数据中心的完整导入流程。

use async_trait::async_trait;
use cmx_core::model::iam::PermissionDefinition;

use crate::error::TraitError;

/// 权限定义导入器 trait。
///
/// 实现方负责两阶段 upsert:
/// 1. 第一阶段:按 code upsert(parent_id 暂置 NULL,full_code_path = '/' + code)
/// 2. 第二阶段:回填 parent_id / parent_code / full_code_path / level,父节点 is_leaf = 0
#[async_trait]
pub trait PermissionDefinitionImporter: Send + Sync {
    /// 将权限定义列表 upsert 到指定作用域。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码(cmx_permission.app_code 列)
    /// * `module_code` - 模块编码
    /// * `definitions` - 权限定义列表(已解析,无需再次解压/校验)
    ///
    /// # Returns
    /// 成功处理的权限数量;空列表时返回 0(不视为错误)。
    async fn apply_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[PermissionDefinition],
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有权限定义(对称契约)。
    ///
    /// 实现方负责查询 `cmx_permission` 并重建 `parent_code`(DB 存 parent_id),
    /// 返回结构化的 `PermissionDefinition` 列表,供模块导出复用。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    async fn list_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<PermissionDefinition>, TraitError>;
}
