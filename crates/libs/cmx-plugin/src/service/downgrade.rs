//! 降级服务模块
//!
//! 处理插件降级流程，提供将插件回退到指定旧版本的功能。
//!
//! 降级只是切换版本目录，不涉及文件拷贝。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use cmx_database::get_default_db_manager;
use cmx_traits::{GlobalEventBus, plugin_events, PluginLifecyclePayload, ServiceQuery, ServiceStorage};
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginSource;
use crate::service::data_parser::ServiceParseParams;
use crate::service::service_parser::parse_services_from_plugin_dir;

/// 降级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 插件来源（可选，用于下载旧版本）
    pub source: Option<PluginSource>,
    /// 操作者
    pub operator: Option<String>,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
    /// 是否发送事件通知（管控接口调用时设为 false）
    #[serde(default = "default_true")]
    pub send_event: bool,
}

fn default_true() -> bool {
    true
}

/// 降级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 旧版本
    pub old_version: String,
    /// 新版本
    pub new_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 降级服务依赖
#[derive(Clone)]
pub struct DowngradeServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
    /// 服务查询（用于查询插件的服务定义）
    pub service_query: Arc<dyn ServiceQuery>,
    /// 服务存储（用于更新服务定义版本）
    pub service_storage: Arc<dyn ServiceStorage>,
    /// 跨实例插件变更通知器
    pub plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,

}

/// 降级服务
#[derive(Clone)]
pub struct DowngradeService {
    deps: DowngradeServiceDeps,
}

impl DowngradeService {
    /// 创建新的降级服务
    pub fn new(deps: DowngradeServiceDeps) -> Self {
        Self { deps }
    }

    /// 降级插件（简化版）
    ///
    /// 降级流程（只切换版本目录，不涉及文件拷贝）:
    /// 1. 检查插件存在
    /// 2. 查找目标版本信息
    /// 3. 更新 cmx_plugin_versions
    /// 4. 更新 cmx_plugin 主表
    /// 5. 更新注册表
    /// 6. 更新缓存
    /// 9. 记录审计日志
    /// 10. 发布降级事件
    pub async fn downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> {
        let start_time = std::time::Instant::now();

        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());
        let send_event = request.send_event;

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id, &app_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        // 步骤2: 查找目标版本信息
        let target_version_record = self.deps.version_history_repository
            .find_version(&request.plugin_id, &app_id, &request.target_version, None)
            .await?
            .ok_or_else(|| {
                PluginError::Downgrade(format!("未找到版本 {} 的记录", request.target_version))
            })?;

        let plugin_id = request.plugin_id.clone();


        let default_db_id = self.deps.default_database_id.clone();
        //开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 步骤4: 更新 cmx_plugin_versions（使用原子操作）
        // 使用 set_current_version 会自动：
        // 1. 标记所有版本为非当前
        // 2. 将目标版本标记为当前
        // 注意：降级场景下 install_path 和 wasm_path 使用已有的记录值
        self.deps.version_history_repository
            .set_current_version(
                &plugin_id,
                &app_id,
                &request.target_version,
                &target_version_record.install_path,
                &target_version_record.wasm_path,
                Some(txn_guard.txn_id()),
            )
            .await?;

        // 步骤5: 更新 cmx_plugin 主表（含回写 marketplace_source_id）
        let fields = crate::infrastructure::database::repository::PluginUpdateParams {
            version: Some(request.target_version.clone()),
            wasm_path: Some(target_version_record.wasm_path.clone()),
            install_path: Some(target_version_record.install_path.clone()),
            marketplace_source_id: target_version_record.marketplace_source_id.clone(),
            ..Default::default()
        };
        self.deps.repository.update_plugin(&plugin_id, &app_id, &fields, Some(txn_guard.txn_id())).await?;

        // 步骤6.1: 更新 cmx_meta_table_define  version 字段
        {
            let dbm = get_default_db_manager();
            crate::infrastructure::database::table_metadata::TableMetadataService::update_version_by_plugin_id(
                dbm,
                default_db_id.as_str(),
                Some(txn_guard.txn_id()),
                &plugin_id,
                &app_id,
                &request.target_version,
            )
            .await
            .map_err(|e| PluginError::Database(format!("更新表元数据 version 失败: {}", e)))?;
        }

        // 步骤6.2: 处理降级时的服务定义
        // 降级时需要从旧版本插件目录解析实际包含的服务定义，
        // 而不是从数据库查询（因为数据库中可能有新版新增的服务）
        {
            // 1. 从旧版本插件目录解析实际的服务定义列表
            let install_path = PathBuf::from(&target_version_record.install_path);
            let parse_params = ServiceParseParams {
                plugin_id: plugin_id.clone(),
                plugin_version: request.target_version.clone(),
                app_id: app_id.clone(),
                domain_code: plugin.domain_code.clone().unwrap_or_default(),
                application_code: plugin.application_code.clone().unwrap_or_default(),
                module_code: plugin.module_code.clone().unwrap_or_default(),
            };
            let old_version_services = parse_services_from_plugin_dir(
                &install_path,
                &parse_params
            )?;
            let old_service_keys: HashSet<String> = old_version_services.iter()
                .map(|s| s.service_key.clone())
                .collect();

            // 2. 查询数据库中该插件的所有服务
            let db_services = self.deps.service_query
                .get_services_by_plugin(&plugin_id)
                .await
                .map_err(|e| PluginError::Database(format!("查询服务定义失败: {}", e)))?;

            // 3. 删除在新版本中存在但旧版本中不存在的服务，更新保留服务的版本号
            let mut deleted_count = 0;
            let mut updated_count = 0;
            for service in db_services {
                if !old_service_keys.contains(&service.service_key) {
                    // 服务在旧版本中不存在，应该删除
                    self.deps.service_storage
                        .delete_service(&service.service_key, &app_id, Some(txn_guard.txn_id()), None)
                        .await
                        .map_err(|e| PluginError::Database(format!("删除服务定义 {} 失败: {}", service.service_key, e)))?;
                    deleted_count += 1;
                } else {
                    // 更新保留服务的版本号
                    let mut updated_service: cmx_core::model::service::ServiceDefinition = service.into();
                    updated_service.version = request.target_version.clone();
                    self.deps.service_storage
                        .save_service(&updated_service, Some(txn_guard.txn_id()))
                        .await
                        .map_err(|e| PluginError::Database(format!("更新服务定义版本失败: {}", e)))?;
                    updated_count += 1;
                }
            }
            tracing::info!(
                "插件 {} 降级时服务处理完成: 删除 {} 个服务，更新 {} 个服务",
                plugin_id, deleted_count, updated_count
            );
        }

        // 步骤7: 更新注册表
        {
            let mut registry = self.deps.registry.write().await;
            if let Some(info) = registry.get(&plugin_id) {
                let mut info = info.clone();
                info.version = request.target_version.clone();
                info.install_path =  PathBuf::from(target_version_record.install_path.clone());
                registry.register(info);
            }
        }

        // 步骤8: 更新缓存
        let plugin_status = match plugin.status.as_str() {
            "activated" => crate::domain::plugin::PluginStatus::Activated,
            "deactivated" => crate::domain::plugin::PluginStatus::Deactivated,
            "error" => crate::domain::plugin::PluginStatus::Error,
            _ => crate::domain::plugin::PluginStatus::Installed,
        };
        let plugin_info = crate::domain::plugin::PluginInfo {
            id: plugin_id.clone(),
            name: plugin.name.clone(),
            version: request.target_version.clone(),
            description: plugin.description.clone(),
            author: plugin.vendor_name.clone(),
            source: PluginSource::Local { path: target_version_record.install_path.clone().into() },
            status: plugin_status,
            installed_at: Some(plugin.create_time),
            updated_at: Some(Utc::now()),
            install_path:PathBuf::from(&target_version_record.install_path),
            domain_code: plugin.domain_code.unwrap_or_default(),
            application_code: plugin.application_code.unwrap_or_default(),
            module_code: plugin.module_code.unwrap_or_default(),
            plugin_type: target_version_record.plugin_type.clone().unwrap_or_default(),
            source_path: target_version_record.source_path.clone(),
            app_id: plugin.app_id.clone(),
        };
        self.deps
            .cache
            .set(
                &plugin_id,
                crate::infrastructure::cache::layered::CacheValue::Json(
                    serde_json::to_value(&plugin_info)?,
                ),
                None,
            )
            .await;

        // 步骤9: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Downgrade,
        )
        .with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": request.target_version,
        }))
        .with_old_value(old_version.clone())
        .with_new_value(request.target_version.clone())
        .with_completed(duration_ms);
       let _ = self.deps.audit_logger.log(audit_record).await;

        //提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 条件发布跨实例变更通知
        if send_event {
            if let Some(notifier) = &self.deps.plugin_notifier {
                notifier.notify_changed(&plugin_id).await;
            }
        }

        // 条件发布降级完成事件
        if send_event {
            let payload = PluginLifecyclePayload::new(&app_id, &plugin_id, &request.target_version)
                .with_old_version(&old_version)
                .with_install_path(PathBuf::from(&target_version_record.install_path))
                .with_wasm_path(PathBuf::from(&target_version_record.wasm_path));

            GlobalEventBus::get()
                .publish(plugin_events::DOWNGRADED, serde_json::to_value(&payload).unwrap())
                .await;
        }

        Ok(DowngradeResponse {
            plugin_id,
            old_version,
            new_version: request.target_version,
            success: true,
            message: "插件降级成功".to_string(),
        })
    }
}


