//! 服务工具模块
//!
//! 提供插件服务共用的工具函数

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use cmx_buffer::LockManager;
use cmx_metadata::TableDefineDbExecutor;
use cmx_core::model::cell::TableDefine;
use cmx_metadata::config::{TableDefinesConfigManager, load_table_defines_config_from_path};
use cmx_metadata::PgTableDefineExecutor;
use cmx_metadata::seed::PgSeedDataExecutor;
use cmx_core::model::meta::plugin::PluginDefinition;
use cmx_database::get_default_db_manager;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::table_metadata::{TableMetadataForCreate, TableMetadataForUpdate, TableMetadataService};

/// 插件表元数据保存上下文。
///
/// 在插件安装或升级时调用，存储表元数据信息。
pub struct TableMetadataSaveContext {
    /// 数据库ID
    pub db_id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 应用隔离标识
    pub app_id: String,
    /// 插件版本
    pub version: String,
    /// 表定义列表
    pub table_defs: Vec<TableDefine>,
    /// 领域编码
    pub domain_code: Option<String>,
    /// 应用编码
    pub application_code: Option<String>,
    /// 模块编码
    pub module_code: Option<String>,
    /// 可选的事务ID
    pub txn_id: Option<String>,
    /// 操作者
    pub operator: Option<String>,
}

impl TableMetadataSaveContext {
    /// 创建新的保存上下文。
    ///
    /// # Arguments
    ///
    /// * `db_id` - 数据库ID
    /// * `plugin_id` - 插件ID
    /// * `app_id` - 应用隔离标识
    /// * `version` - 插件版本
    ///
    /// # Returns
    ///
    /// 返回新的 `TableMetadataSaveContext` 实例。
    pub fn new(
        db_id: String,
        plugin_id: String,
        app_id: String,
        version: String,
    ) -> Self {
        Self {
            db_id,
            plugin_id,
            app_id,
            version,
            table_defs: Vec::new(),
            domain_code: None,
            application_code: None,
            module_code: None,
            txn_id: None,
            operator: None,
        }
    }
}

/// 创建插件数据库表
///
/// 使用 cmx-metadata 解析表定义并创建数据库表。
///
/// # 参数
/// - `db_id`: 数据库ID
/// - `plugin_id`: 插件ID
/// - `app_id`: 应用隔离标识
/// - `version`: 插件版本
/// - `install_path`: 插件安装路径
/// - `plugin_define`: 插件配置信息表配置文件列表
/// - `txn_id`: 可选的事务ID
pub async fn create_plugin_tables(
    db_id: &str,
    plugin_id: &str,
    app_id: &str,
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

    // 执行种子数据初始化
    let all_seed_configs = table_config_manager.collect_seed_configs();
    if !all_seed_configs.is_empty() {
        tracing::info!("插件 {} 开始执行种子数据初始化，数据文件数{}", plugin_id, &all_seed_configs.len());
        let seed_executor = PgSeedDataExecutor::new(db_id, None);
        let summary = seed_executor
            .execute_all_seed_data(&table_defs, &all_seed_configs, install_path)
            .await;

        // 输出汇总日志（不阻断安装）
        tracing::info!(
            "插件 {} 种子数据执行完成: {} 表处理, {} 成功, {} 失败, 耗时 {}ms",
            plugin_id,
            summary.table_results.len(),
            summary.total_success(),
            summary.total_failed(),
            summary.total_duration_ms,
        );

        // 输出错误详情
        for result in &summary.table_results {
            for failure in &result.failures {
                tracing::error!(
                    "种子数据执行失败: 表={}, 行={}, 错误={}",
                    result.table_name,
                    failure.row_index,
                    failure.error_message,
                );
            }
            // 数据条数校验警告
            if let Some(db_count) = result.db_row_count
                && db_count < result.file_row_count {
                    tracing::warn!(
                        "种子数据条数不一致: 表={}, 文件={}条, 数据库={}条",
                        result.table_name,
                        result.file_row_count,
                        db_count,
                    );
                }
        }
    }else{
        tracing::info!("插件 {} 没有种子数据", plugin_id);
    }

    // 构建元数据保存上下文
    let mut ctx = TableMetadataSaveContext::new(
        db_id.to_string(),
        plugin_id.to_string(),
        app_id.to_string(),
        version.to_string(),
    );
    ctx.table_defs = table_defs.clone();
    ctx.domain_code = plugin_define.domain_code.clone();
    ctx.application_code = plugin_define.application_code.clone();
    ctx.module_code = plugin_define.module_code.clone();
    ctx.txn_id = txn_id.map(|s| s.to_string());

    if let Err(e) = save_plugin_table_metadata(ctx).await {
        tracing::error!("保存表元数据失败: {}", e);
        return Err(e);
    }


    Ok(table_defs)
}

/// 保存插件表元数据。
///
/// 在插件安装或升级时调用，存储表元数据信息。
///
/// # Arguments
///
/// * `ctx` - 插件表元数据保存上下文
///
/// # Returns
///
/// 保存操作的结果
pub async fn save_plugin_table_metadata(
    ctx: TableMetadataSaveContext,
) -> PluginResult<()> {
    for table_def in &ctx.table_defs {
        let dbm = get_default_db_manager();
        let default_db_id = dbm.get_default_db_id().await;

        let table_metadata_result = TableMetadataService::get_by_table_name(dbm,default_db_id.as_str(),
                                                                            table_def.table_name.as_str(), Some(&ctx.db_id)).await;

        if let Ok(metadata) = table_metadata_result {
            if !metadata.is_empty() {
                //存在  更新下
                //先查询
                let table_meta_defines_result = TableMetadataService::parse_metadata_record(&metadata);
                let record   = table_meta_defines_result.as_ref().unwrap().iter().next().unwrap();

                let table_define_primary_id = record.id.clone();

                let update_info = TableMetadataForUpdate{
                    display_name: Some(table_def.display_name.clone()),
                    version: Some(ctx.version.clone()),
                    domain_code: ctx.domain_code.clone(),
                    application_code: ctx.application_code.clone(),
                    module_code: ctx.module_code.clone(),
                    metadata: Some(serde_json::to_value(table_def).unwrap_or(serde_json::Value::Null)),
                };
                TableMetadataService::update(dbm, &ctx.plugin_id, default_db_id.as_str(), ctx.txn_id.as_deref(), Value::String(table_define_primary_id), update_info).await?;
            } else {
                //不存在  新增
                let create_info = TableMetadataForCreate{
                    table_name: table_def.table_name.clone(),
                    display_name: table_def.display_name.clone(),
                    db_id: ctx.db_id.clone(),
                    plugin_id: ctx.plugin_id.clone(),
                    version: ctx.version.clone(),
                    domain_code: ctx.domain_code.clone().unwrap_or_default(),
                    application_code: ctx.application_code.clone().unwrap_or_default(),
                    module_code: ctx.module_code.clone().unwrap_or_default(),
                    metadata: serde_json::to_value(table_def).unwrap_or(serde_json::Value::Null),
                    app_id: Some(ctx.app_id.clone()),
                };
                TableMetadataService::create(dbm, default_db_id.as_str(), ctx.txn_id.as_deref(), create_info).await?;
            }
        }
    }

    Ok(())
}

/// 执行 DDL 操作（带分布式锁保护）。
///
/// 使用 `try_lock` 非阻塞分布式锁保护 DDL 操作，确保多实例下
/// 只有一个实例执行 DDL。DML 使用 upsert 天然幂等，无需锁保护。
///
/// # Arguments
///
/// * `lock_manager` - 分布式锁管理器，为 `None` 时直接执行 DDL
/// * `target_db_id` - 目标数据库 ID
/// * `plugin_id` - 插件 ID
/// * `app_id` - 应用 ID
/// * `version` - 插件版本
/// * `install_path` - 安装路径
/// * `plugin_def` - 插件定义
/// * `txn_id` - 事务 ID，为 `None` 时 DDL 不在事务内执行
pub async fn execute_ddl_with_lock(
    lock_manager: &Option<Arc<LockManager>>,
    target_db_id: &str,
    plugin_id: &str,
    app_id: &str,
    version: &str,
    install_path: &Path,
    plugin_def: &PluginDefinition,
    txn_id: Option<&str>,
) -> PluginResult<()> {
    // 无表配置时直接返回，避免不必要的锁操作
    if plugin_def.table_config_files.is_empty() {
        return Ok(());
    }

    let lock_key = format!("plugin:ddl:{}", plugin_id);

    if let Some(lm) = lock_manager {
        match lm.try_lock_with_value(&lock_key).await {
            Ok((true, Some(lock_value))) => {
                tracing::info!("获取DDL锁成功，本实例负责创建/升级表: {}", plugin_id);
                create_plugin_tables(
                    target_db_id, plugin_id, app_id, version,
                    install_path, plugin_def, txn_id,
                ).await?;
                if let Err(e) = lm.unlock_with_value(&lock_key, &lock_value).await {
                    tracing::debug!("释放DDL锁失败（将等待TTL过期）: {}", e);
                }
            }
            Ok(_) => {
                tracing::info!("其他实例正在创建/升级表，跳过DDL: {}", plugin_id);
            }
            Err(e) => {
                tracing::warn!("锁服务异常: {}，继续创建/升级表", e);
                create_plugin_tables(
                    target_db_id, plugin_id, app_id, version,
                    install_path, plugin_def, None,
                ).await?;
            }
        }
    } else {
        create_plugin_tables(
            target_db_id, plugin_id, app_id, version,
            install_path, plugin_def, txn_id,
        ).await?;
    }

    Ok(())
}
