//! 服务工具模块
//!
//! 提供插件服务共用的工具函数

use std::path::Path;

use serde_json::Value;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_core::model::cell::TableDefine;
use cmx_metadata::config::{TableDefinesConfigManager, load_table_defines_config_from_path};
use cmx_metadata::PgTableDefineExecutor;
use cmx_core::model::meta::plugin::PluginDefinition;
use cmx_database::get_default_db_manager;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::table_metadata::{TableMetadataForCreate, TableMetadataForUpdate, TableMetadataService};

/// 创建插件数据库表
///
/// 使用 cmx-metadata 解析表定义并创建数据库表。
///
/// # 参数
/// - `db_id`: 数据库ID
/// - `plugin_id`: 插件ID
/// - `version`: 插件版本
/// - `install_path`: 插件安装路径
/// - `plugin_define`: 插件配置信息表配置文件列表
/// - `txn_id`: 可选的事务ID
pub async fn create_plugin_tables(
    db_id: &str,
    plugin_id: &str,
    version: &str,
    install_path: &Path,
    plugin_define:&PluginDefinition,
    txn_id: Option<&str>,
) -> PluginResult<Vec<TableDefine>> {
    if plugin_define.table_config_files.clone().is_empty() {
        return Ok(Vec::new());
    }

    let mut table_config_manager = TableDefinesConfigManager::new();
    // let executor = PgTableDefineExecutor::new(db_id, txn_id.map(|s| s.to_string()));
    let executor = PgTableDefineExecutor::new(db_id, None);

    for table_config_file in plugin_define.table_config_files.clone() {
        let config_path = install_path.join(table_config_file);
        let table_df = load_table_defines_config_from_path(&config_path)
            .map_err(|e| PluginError::Metadata(format!("加载表配置文件失败:路径{:?}，错误： {}",config_path, e)))?;
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

        if let Err(e) = save_plugin_table_metadata(
            db_id,
            plugin_id,
            version,
            &table_defs,
            plugin_define.domain_code.clone(),
            plugin_define.application_code.clone(),
            plugin_define.module_code.clone(),
            txn_id,
            None
        )
        .await
        {
            log::error!("保存表元数据失败: {}", e);
            return Err(e);
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
    db_id: &str,
    plugin_id: &str,
    version: &str,
    table_defs: &[TableDefine],
    domain_code: Option<String>,
    application_code: Option<String>,
    moudule_code: Option<String>,
    txn_id: Option<&str>,
    _operator: Option<&str>,
) -> PluginResult<()> {
    for table_def in table_defs {
        let dbm = get_default_db_manager();
        let default_db_id = dbm.get_default_db_id().await;

        let table_metadata_result = TableMetadataService::get_by_table_name(dbm,default_db_id.as_str(),
                                                                     table_def.table_name.as_str(),db_id).await;

        if table_metadata_result.is_ok() && !table_metadata_result.as_ref().unwrap().is_empty(){
            //存在  更新下
            //先查询
            let table_meta_defines_result = TableMetadataService::parse_metadata_record(&table_metadata_result.unwrap());
             let record   = table_meta_defines_result.as_ref().unwrap().iter().next().unwrap();

            let table_define_primary_id = record.id.clone();

            let update_info = TableMetadataForUpdate{
                version: Some(version.to_string()),
                domain_code: domain_code.clone(),
                application_code: application_code.clone(),
                module_code: moudule_code.clone(),
                metadata: Some(serde_json::to_value(table_def).unwrap_or(serde_json::Value::Null)),
            };
            TableMetadataService::update(dbm,default_db_id.as_str(),txn_id,Value::String(table_define_primary_id),update_info).await?;

        }else{
            //不存在  新增
            let create_info = TableMetadataForCreate{
                table_name: table_def.table_name.clone(),
                db_id: db_id.to_string(),
                plugin_id: plugin_id.to_string(),
                version: version.to_string(),
                domain_code: domain_code.clone().unwrap_or_default(),
                application_code: application_code.clone().unwrap_or_default(),
                module_code: moudule_code.clone().unwrap_or_default(),
                metadata: serde_json::to_value(table_def).unwrap_or(serde_json::Value::Null),

            };
            TableMetadataService::create(dbm,default_db_id.as_str(),txn_id,create_info).await?;
        }
    }

    Ok(())
}
