//! 表单定义导入器 trait。

use async_trait::async_trait;
use cmx_core::model::module::FormDefinition;

use crate::error::TraitError;

/// 表单定义导入器(本地/远程统一接口)。
///
/// 实现方负责:
/// - [`apply_form_definitions`]:将结构化表单定义 upsert 到指定作用域(幂等:先删同 code 再建)
/// - [`list_form_definitions`]:导出指定模块的所有表单定义(对称契约)
///
/// [`apply_form_definitions`]: FormDefinitionImporter::apply_form_definitions
/// [`list_form_definitions`]: FormDefinitionImporter::list_form_definitions
#[async_trait]
pub trait FormDefinitionImporter: Send + Sync {
    /// 将表单定义列表 upsert 到指定作用域。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    /// * `definitions` - 表单定义列表(已解析)
    /// * `txn_id` - 外部事务 ID(仅本地实现有效;远程实现跨服务无共享事务,会忽略此参数)
    ///
    /// # Returns
    /// 成功处理的表单数量;空列表返回 0。
    async fn apply_form_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[FormDefinition],
        txn_id: Option<&str>,
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有表单定义。
    ///
    /// # Arguments
    /// * `module_code` - 模块编码
    async fn list_form_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<FormDefinition>, TraitError>;
}
