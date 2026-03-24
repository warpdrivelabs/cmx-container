//! 服务工具模块
//!
//! 提供插件服务共用的工具函数

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_core::model::cell::TableDefine;
use cmx_metadata::config::{TableDefinesConfigManager, load_table_defines_config_from_path};
use cmx_metadata::PgTableDefineExecutor;
use uuid::Uuid;

use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::table_metadata::{
    TableMetadataRecord, TableMetadataRepository, TableMetadataVersionRecord,
};

/// 创建插件数据库表
///
/// 使用 cmx-metadata 解析表定义并创建数据库表。
///
/// # 参数
/// - `db_id`: 数据库ID
/// - `plugin_id`: 插件ID
/// - `version`: 插件版本
/// - `install_path`: 插件安装路径
/// - `table_config_files`: 表配置文件列表
/// - `txn_id`: 可选的事务ID
/// - `table_metadata_repo`: 表元数据仓库（用于存储元数据）
pub async fn create_plugin_tables(
    db_id: &str,
    plugin_id: &str,
    version: &str,
    install_path: &Path,
    table_config_files: &[String],
    txn_id: Option<String>,
    table_metadata_repo: Option<&TableMetadataRepository>,
) -> PluginResult<Vec<TableDefine>> {
    if table_config_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut table_config_manager = TableDefinesConfigManager::new();
    let executor = PgTableDefineExecutor::new(db_id, txn_id.clone());

    for table_config_file in table_config_files {
        let config_path = install_path.join(table_config_file);
        let table_df = load_table_defines_config_from_path(&config_path)
            .map_err(|e| PluginError::Metadata(format!("加载表配置文件失败: {}", e)))?;
        table_config_manager.add_config(table_df);
    }

    let table_defs = table_config_manager
        .load_all_tables(install_path)
        .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;

    for table_def in &table_defs {
        executor
            .create_or_upgrade_table(table_def)
            .await
            .map_err(|e| {
                PluginError::Metadata(format!(
                    "创建或升级表{}失败: {}",
                    &table_def.table_name, e
                ))
            })?;
    }

    if let Some(repo) = table_metadata_repo {
        if let Err(e) = save_plugin_table_metadata(
            repo,
            db_id,
            plugin_id,
            version,
            &table_defs,
            None,
        )
        .await
        {
            log::warn!("保存表元数据失败: {}", e);
        }
    }

    Ok(table_defs)
}

/// 保存插件表元数据
///
/// 在插件安装或升级时调用，存储表元数据信息。
///
/// # 参数
/// - `repository`: 表元数据仓库
/// - `db_id`: 数据库ID
/// - `plugin_id`: 插件ID
/// - `version`: 插件版本
/// - `table_defs`: 表定义列表
/// - `operator`: 操作者
pub async fn save_plugin_table_metadata(
    repository: &TableMetadataRepository,
    db_id: &str,
    plugin_id: &str,
    version: &str,
    table_defs: &[TableDefine],
    operator: Option<&str>,
) -> PluginResult<()> {
    for table_def in table_defs {
        let now = Utc::now();
        let operator_str = operator.map(String::from);

        let version_record = TableMetadataVersionRecord {
            id: Uuid::new_v4().to_string(),
            table_name: table_def.table_name.clone(),
            db_id: db_id.to_string(),
            plugin_id: plugin_id.to_string(),
            version: version.to_string(),
            metadata: serde_json::to_value(table_def).unwrap_or(serde_json::Value::Null),
            archived: 0,
            create_time: now,
            update_time: now,
            create_by: operator_str.clone(),
            create_name: None,
            update_by: None,
            update_name: None,
        };

        repository
            .insert_or_update_version(&version_record, None)
            .await?;

        let metadata_record = TableMetadataRecord {
            id: Uuid::new_v4().to_string(),
            table_name: table_def.table_name.clone(),
            db_id: db_id.to_string(),
            plugin_id: plugin_id.to_string(),
            version: version.to_string(),
            metadata: serde_json::Value::Null,
            archived: 0,
            create_time: now,
            update_time: now,
            create_by: operator_str,
            create_name: None,
            update_by: None,
            update_name: None,
        };
        repository.upsert_metadata(&metadata_record, None).await?;
    }

    Ok(())
}
