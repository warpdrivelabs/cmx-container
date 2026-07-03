//! 服务工具模块
//!
//! 提供插件服务共用的工具函数。
//!
//! 注:表 DDL(create_or_upgrade_table)与表元数据保存已迁移到模块安装流程
//! (ModuleInstallService::install_metadata / save_table_metadata),
//! 本模块仅保留插件单独安装时仍需随插件包执行的种子数据初始化(execute_seed_data)。

use std::path::Path;

use cmx_core::model::meta::plugin::PluginDefinition;
use cmx_metadata::config::{TableDefinesConfigManager, load_table_defines_config_from_path};
use cmx_metadata::seed::PgSeedDataExecutor;

use crate::error::{PluginError, PluginResult};

/// 执行插件种子数据初始化(不含建表/元数据)。
///
/// 在插件单独安装时调用:建表 DDL 已迁移到模块安装流程,
/// 但 seeddata 保留在插件包内,种子初始化仍需随插件安装执行。
///
/// 从 plugin_define.table_config_files 加载配置,收集 seed_data 配置,
/// 执行种子数据导入(种子文件相对 install_path 定位)。
/// 若插件无 table_config_files 或无种子数据,静默返回。
///
/// # Arguments
/// * `db_id` - 数据库ID
/// * `plugin_id` - 插件ID(用于日志)
/// * `install_path` - 插件安装路径(种子文件相对路径的基准)
/// * `plugin_define` - 插件配置(含 table_config_files)
pub async fn execute_seed_data(
    db_id: &str,
    plugin_id: &str,
    install_path: &Path,
    plugin_define: &PluginDefinition,
) -> PluginResult<()> {
    if plugin_define.table_config_files.is_empty() {
        return Ok(());
    }

    // 加载表配置并收集表定义(种子执行需要 table_defines 做字段映射)
    let mut table_config_manager = TableDefinesConfigManager::new();
    for table_config_file in &plugin_define.table_config_files {
        let config_path = install_path.join(table_config_file);
        let table_df = load_table_defines_config_from_path(&config_path)
            .map_err(|e| PluginError::Metadata(format!(
                "加载表配置文件失败:路径{:?}，错误： {}", config_path, e
            )))?;
        table_config_manager.add_config(table_df);
    }
    let table_defs = table_config_manager
        .load_all_tables(install_path)
        .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;

    // 收集种子配置并执行
    let all_seed_configs = table_config_manager.collect_seed_configs();
    if all_seed_configs.is_empty() {
        tracing::info!("插件 {} 没有种子数据", plugin_id);
        return Ok(());
    }

    tracing::info!(
        "插件 {} 开始执行种子数据初始化，数据文件数{}",
        plugin_id,
        all_seed_configs.len()
    );
    let seed_executor = PgSeedDataExecutor::new(db_id, None);
    let summary = seed_executor
        .execute_all_seed_data(&table_defs, &all_seed_configs, install_path)
        .await;

    tracing::info!(
        "插件 {} 种子数据执行完成: {} 表处理, {} 成功, {} 失败, 耗时 {}ms",
        plugin_id,
        summary.table_results.len(),
        summary.total_success(),
        summary.total_failed(),
        summary.total_duration_ms,
    );

    // 输出错误详情(不阻断安装)
    for result in &summary.table_results {
        for failure in &result.failures {
            tracing::error!(
                "种子数据执行失败: 表={}, 行={}, 错误={}",
                result.table_name,
                failure.row_index,
                failure.error_message,
            );
        }
        if let Some(db_count) = result.db_row_count
            && db_count < result.file_row_count
        {
            tracing::warn!(
                "种子数据条数不一致: 表={}, 文件={}条, 数据库={}条",
                result.table_name,
                result.file_row_count,
                db_count,
            );
        }
    }

    Ok(())
}
