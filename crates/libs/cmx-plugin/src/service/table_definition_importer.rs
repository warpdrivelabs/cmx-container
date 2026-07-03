//! `TableDefinitionImporter` trait 的本地实现。
//!
//! 建表到业务库(PgTableDefineExecutor) + 登记元数据到 default 库(TableMetadataService)。
//! 收敛原 `module_install::install_metadata` + `save_table_metadata` 的逻辑。

use async_trait::async_trait;
use cmx_core::model::meta::table::TableDefine;
use cmx_database::DatabaseManager;
use cmx_metadata::TableDefineDbExecutor;
use cmx_traits::error::TraitError;
use cmx_traits::module::TableDefinitionImporter;
use std::sync::Arc;
use tracing::{info, warn};

use crate::infrastructure::database::table_metadata::TableMetadataService;

/// 本地表结构定义导入器。
///
/// 双库语义:建表到 `biz_db_id`(业务库),元数据登记到 `default_db_id`(default 库),
/// 登记记录的 db_id 列标记 biz_db_id(业务表所在库)。
pub struct LocalTableDefinitionImporter {
    mm: Arc<DatabaseManager>,
    default_db_id: String,
}

impl LocalTableDefinitionImporter {
    /// 创建本地表结构定义导入器。
    ///
    /// # Arguments
    /// * `mm` - 数据库管理器
    /// * `default_db_id` - 元数据登记库(default 库)
    pub fn new(mm: Arc<DatabaseManager>, default_db_id: String) -> Self {
        Self { mm, default_db_id }
    }
}

#[async_trait]
impl TableDefinitionImporter for LocalTableDefinitionImporter {
    async fn apply_table_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        app_id: &str,
        definitions: &[TableDefine],
        biz_db_id: &str,
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 用 PgTableDefineExecutor 建表到业务库(无需分布式锁,模块安装是低频操作)
        let executor = cmx_metadata::executor::PgTableDefineExecutor::new(biz_db_id, None);
        let mut count = 0usize;
        for table_def in definitions {
            match executor.create_or_upgrade_table(table_def).await {
                Ok(_) => info!(table = %table_def.table_name, "建表/升级成功(业务库)"),
                Err(e) => {
                    warn!(table = %table_def.table_name, error = %e, "建表失败");
                    continue;
                }
            }
            // 登记元数据到 default 库(记录 db_id 列标记 biz 库)
            if let Err(e) = TableMetadataService::upsert_by_table_name(
                &self.mm,
                &self.default_db_id,
                biz_db_id,
                table_def,
                domain_code,
                app_code,
                module_code,
                app_id,
            )
            .await
            {
                warn!(table = %table_def.table_name, error = %e, "元数据登记失败");
            }
            count += 1;
        }
        info!(count, "表结构定义导入完成");
        Ok(count)
    }

    async fn list_table_definitions(
        &self,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<TableDefine>, TraitError> {
        TableMetadataService::list_by_module(&self.mm, &self.default_db_id, app_code, module_code)
            .await
            .map_err(|e| TraitError::Business(format!("查询模块表结构定义失败: {e}")))
    }
}
