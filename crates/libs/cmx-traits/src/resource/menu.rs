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
    /// 将菜单节点定义列表安装到指定作用域。
    ///
    /// 本地实现内部自行开启并提交事务(一次 apply 一个事务);远程实现跨服务无共享事务。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    /// * `definitions` - 菜单节点定义列表
    ///
    /// # Returns
    /// 成功处理的菜单数量。
    async fn apply_menu_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[MenuDefinition],
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
