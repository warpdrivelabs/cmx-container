//! 服务工具模块
//!
//! 提供插件服务共用的工具函数

use std::path::Path;

use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_metadata::config::{TableDefinesConfigManager, load_table_defines_config_from_path};
use cmx_metadata::PgTableDefineExecutor;

use crate::error::{PluginError, PluginResult};

/// 创建插件数据库表
///
/// 使用 cmx-metadata 解析表定义并创建数据库表。
///
/// # 参数
/// - `db_id`: 数据库ID
/// - `install_path`: 插件安装路径
/// - `table_config_files`: 表配置文件列表
/// - `txn_id`: 可选的事务ID
pub async fn create_plugin_tables(
    db_id: &str,
    install_path: &Path,
    table_config_files: &[String],
    txn_id: Option<String>,
) -> PluginResult<()> {
    if table_config_files.is_empty() {
        return Ok(());
    }

    let mut table_config_manager = TableDefinesConfigManager::new();
    let executor = PgTableDefineExecutor::new(db_id, txn_id);

    for table_config_file in table_config_files {
        let config_path = install_path.join(table_config_file);
        let table_df = load_table_defines_config_from_path(&config_path)
            .map_err(|e| PluginError::Metadata(format!("加载表配置文件失败: {}", e)))?;
        table_config_manager.add_config(table_df);
    }

    let table_defs = table_config_manager.load_all_tables(install_path)
        .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;

    for table_def in table_defs {
        executor
            .create_or_upgrade_table(&table_def).await
            .map_err(|e|
                PluginError::Metadata(format!("创建或升级表{}失败: {}", &table_def.table_name, e)))?;
    }
    //记录tabledefine中

    Ok(())
}
