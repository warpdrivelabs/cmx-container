//! 表结构定义导入器 trait。

use async_trait::async_trait;
use cmx_core::model::meta::table::TableDefine;

use crate::error::TraitError;

/// 表结构定义导入器(本地/远程统一接口)。
///
/// 与表单/菜单的差异:多一个 `biz_db_id` 参数(建表目标库),
/// 因元数据有「建表到业务库 + 登记到 default 库」的双库语义。
///
/// 实现方负责:
/// - [`apply_table_definitions`]:建表到业务库 + 登记元数据到 default 库
/// - [`list_table_definitions`]:导出指定模块的所有表结构定义
///
/// [`apply_table_definitions`]: TableDefinitionImporter::apply_table_definitions
/// [`list_table_definitions`]: TableDefinitionImporter::list_table_definitions
#[async_trait]
pub trait TableDefinitionImporter: Send + Sync {
    /// 将表结构定义建表到业务库 + 登记元数据。
    ///
    /// # Arguments
    /// * `domain_code` - 域编码
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    /// * `app_id` - 应用隔离标识(当前 ≡ module_code,见 AGENTS.md 六章)
    /// * `definitions` - 表结构定义列表
    /// * `biz_db_id` - 建表目标业务库 ID(元数据登记库由实现内部决定)
    /// * `txn_id` - 外部事务 ID(仅本地实现有效,作用于元数据登记;
    ///   建表 DDL 在 PG 自动提交不进事务;远程实现跨服务无共享事务,会忽略此参数)
    ///
    /// # Returns
    /// 成功处理的表数量。
    async fn apply_table_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        app_id: &str,
        definitions: &[TableDefine],
        biz_db_id: &str,
        txn_id: Option<&str>,
    ) -> Result<usize, TraitError>;

    /// 导出指定模块的所有表结构定义。
    ///
    /// # Arguments
    /// * `app_code` - 应用编码
    /// * `module_code` - 模块编码
    async fn list_table_definitions(
        &self,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<TableDefine>, TraitError>;
}
