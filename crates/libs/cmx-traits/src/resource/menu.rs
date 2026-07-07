//! 菜单定义导入器 trait。

use async_trait::async_trait;
use cmx_core::model::module::MenuDefinition;

use crate::error::TraitError;

/// 菜单定义导入器(本地/远程统一接口)。
///
/// 实现方负责:
/// - [`apply_menu_definitions`]:将根菜单定义列表安装到指定作用域(每个 definition 含完整菜单树)
/// - [`list_menu_definitions`]:导出指定模块的所有根菜单定义
///
/// [`apply_menu_definitions`]: MenuDefinitionImporter::apply_menu_definitions
/// [`list_menu_definitions`]: MenuDefinitionImporter::list_menu_definitions
#[async_trait]
pub trait MenuDefinitionImporter: Send + Sync {
    /// 将根菜单定义列表安装到指定作用域。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    /// * `definitions` - 根菜单定义列表(每个含完整菜单树)
    /// * `txn_id` - 外部事务 ID(仅本地实现有效;远程实现跨服务无共享事务,会忽略此参数)
    ///
    /// # Returns
    /// 成功处理的菜单数量。
    async fn apply_menu_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[MenuDefinition],
        txn_id: Option<&str>,
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有根菜单定义。
    ///
    /// # Arguments
    /// * `module_code` - 模块编码
    async fn list_menu_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<MenuDefinition>, TraitError>;
}
