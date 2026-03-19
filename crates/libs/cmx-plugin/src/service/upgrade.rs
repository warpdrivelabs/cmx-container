//! 升级服务模块
//! 
//! 处理插件升级流程

use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::{PluginRepository, PluginUpdateFields};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::audit::record::{AuditRecord, OperationType};
use crate::service::install::InstallService;
use crate::service::activate::ActivateService;

/// 升级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 新版本来源
    pub source: crate::domain::plugin::PluginSource,
    /// 是否强制升级（忽略版本检查）
    pub force: bool,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 是否保留旧版本备份
    pub keep_backup: bool,
    /// 版本约束（仅对注册表来源有效）
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 升级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
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

/// 升级服务
pub struct UpgradeService {
    /// 数据仓库
    repository: Arc<PluginRepository>,
    /// 缓存管理器
    cache: Arc<LayeredCacheManager>,
    /// 文件存储
    storage: Arc<FileStorage>,
    /// 备份管理器
    backup_manager: Arc<BackupManager>,
    /// 事件总线
    event_bus: Arc<EventBus>,
    /// 审计日志
    audit_logger: Arc<AuditLogger>,
    /// 安装服务
    install_service: Arc<InstallService>,
    /// 激活服务
    activate_service: Arc<ActivateService>,
    /// 安装根目录
    install_root: PathBuf,
}

impl UpgradeService {
    /// 创建新的升级服务
    pub fn new(
        repository: Arc<PluginRepository>,
        cache: Arc<LayeredCacheManager>,
        storage: Arc<FileStorage>,
        backup_manager: Arc<BackupManager>,
        event_bus: Arc<EventBus>,
        audit_logger: Arc<AuditLogger>,
        install_service: Arc<InstallService>,
        activate_service: Arc<ActivateService>,
        install_root: PathBuf,
    ) -> Self {
        Self {
            repository,
            cache,
            storage,
            backup_manager,
            event_bus,
            audit_logger,
            install_service,
            activate_service,
            install_root,
        }
    }
    
    /// 升级插件
    /// 
    /// 完整的升级流程：
    /// 1. 检查插件存在
    /// 2. 检查新版本是否比当前版本高
    /// 3. 停用当前版本
    /// 4. 创建备份
    /// 5. 安装新版本
    /// 6. 更新数据库记录
    /// 7. 激活新版本（可选）
    /// 8. 记录审计日志
    pub async fn upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：检查插件存在
        let old_plugin = self.repository.find_plugin(&request.plugin_id).await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;
        
        let old_version = old_plugin.version.clone();
        let was_activated = old_plugin.status == "activated";
        
        // 步骤2：停用当前版本（如果已激活）
        if was_activated {
            self.activate_service.deactivate(crate::service::activate::DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: false,
            }).await?;
        }
        
        // 步骤3：创建备份
        if request.keep_backup {
            let install_path = PathBuf::from(&old_plugin.install_path);
            if install_path.exists() {
                self.backup_manager.create_backup(
                    &request.plugin_id,
                    &old_version,
                    &install_path,
                ).await.map_err(|e| PluginError::Upgrade(format!("创建备份失败: {}", e)))?;
            }
        }
        
        // 步骤4：获取新版本信息
        let package_path = self.fetch_package(&request.source).await?;
        let new_def = self.parse_plugin_definition(&package_path).await?;
        let new_version = new_def.version.clone().unwrap_or_else(|| "1.0.0".to_string());
        
        // 步骤5：删除旧文件
        let install_path = PathBuf::from(&old_plugin.install_path);
        if install_path.exists() {
            self.storage.remove_dir(&install_path)
                .map_err(|e| PluginError::Upgrade(format!("删除旧版本文件失败: {}", e)))?;
        }
        
        // 步骤6：安装新版本
        self.install_service.install(crate::service::install::InstallRequest {
            source: request.source.clone(),
            db_id: Some(old_plugin.db_id.clone()),
            force: true,
            auto_activate: false,
            version_constraint: request.version_constraint.clone(),
        }).await?;
        
        // 步骤7：激活新版本（可选）
        if request.auto_activate || was_activated {
            self.activate_service.activate(crate::service::activate::ActivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: false,
            }).await?;
        }
        
        // 步骤8：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Upgrade,
        ).with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": new_version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginUpgraded,
            request.plugin_id.clone(),
            serde_json::json!({
                "old_version": old_version,
                "new_version": new_version,
            }),
        )).await;
        
        Ok(UpgradeResponse {
            plugin_id: request.plugin_id,
            old_version,
            new_version,
            success: true,
            message: "插件升级成功".to_string(),
        })
    }
    
    /// 获取插件包
    async fn fetch_package(&self, source: &crate::domain::plugin::PluginSource) -> PluginResult<PathBuf> {
        match source {
            crate::domain::plugin::PluginSource::Local { path } => {
                let fetcher = crate::fetcher::local::LocalFetcher::new(&self.install_root);
                fetcher.fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Upgrade(format!("获取本地插件包失败: {}", e)))
            }
            crate::domain::plugin::PluginSource::Remote { url, checksum } => {
                let fetcher = crate::fetcher::remote::RemoteFetcher::new(self.install_root.join("temp"));
                fetcher.fetch(&crate::fetcher::source::PluginSource::remote(url.clone(), checksum.clone()))
                    .await
                    .map_err(|e| PluginError::Upgrade(format!("获取远程插件包失败: {}", e)))
            }
            crate::domain::plugin::PluginSource::Registry { registry_url, package_name } => {
                Err(PluginError::Upgrade("注册表获取尚未实现".to_string()))
            }
        }
    }
    
    /// 解析插件定义
    async fn parse_plugin_definition(&self, package_path: &std::path::Path) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let manifest_path = package_path.join("plugin.json");
        
        if !manifest_path.exists() {
            return Err(PluginError::Metadata("插件定义文件 plugin.json 不存在".to_string()));
        }
        
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Metadata(format!("读取插件定义文件失败: {}", e)))?;
        
        let definition: cmx_core::model::meta::plugin::PluginDefinition = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析插件定义文件失败: {}", e)))?;
        
        Ok(definition)
    }
}

impl Default for UpgradeService {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginRepository::default()),
            Arc::new(LayeredCacheManager::default()),
            Arc::new(FileStorage::new(std::path::Path::new(""))),
            Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::default()),
            Arc::new(InstallService::default()),
            Arc::new(ActivateService::default()),
            PathBuf::from("./plugins"),
        )
    }
}
