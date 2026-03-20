//! 安装服务模块
//! 
//! 处理插件安装流程

use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus, PluginFilter};
use crate::domain::dependency::{DependencyCheckResult, MissingDependency};
use crate::infrastructure::database::repository::{PluginRepository, PluginDbRecord};
use crate::infrastructure::cache::layered::{LayeredCacheManager, CacheValue};
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::security::validator::SecurityValidator;
use crate::audit::logger::AuditLogger;
use crate::audit::record::{AuditRecord, OperationType};
use crate::fetcher::local::LocalFetcher;
use crate::fetcher::remote::RemoteFetcher;

/// 安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件来源
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    /// 是否强制安装（覆盖已存在的插件）
    pub force: bool,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 版本约束（仅对注册表来源有效，如 "^1.0.0", ">=2.0.0"）
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 安装响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 安装服务
pub struct InstallService {
    /// 数据仓库
    repository: Arc<PluginRepository>,
    /// 缓存管理器
    cache: Arc<LayeredCacheManager>,
    /// 文件存储
    storage: Arc<FileStorage>,
    /// 备份管理器
    backup_manager: Arc<BackupManager>,
    /// 安全验证器
    validator: Arc<SecurityValidator>,
    /// 事件总线
    event_bus: Arc<EventBus>,
    /// 审计日志
    audit_logger: Arc<AuditLogger>,
    /// 安装根目录
    install_root: PathBuf,
    /// 临时目录
    temp_dir: PathBuf,
    /// 已安装插件缓存
    installed_plugins: Arc<RwLock<std::collections::HashMap<String, PluginInfo>>>,
}

impl InstallService {
    /// 创建新的安装服务
    pub fn new(
        repository: Arc<PluginRepository>,
        cache: Arc<LayeredCacheManager>,
        storage: Arc<FileStorage>,
        backup_manager: Arc<BackupManager>,
        validator: Arc<SecurityValidator>,
        event_bus: Arc<EventBus>,
        audit_logger: Arc<AuditLogger>,
        install_root: PathBuf,
        temp_dir: PathBuf,
    ) -> Self {
        Self {
            repository,
            cache,
            storage,
            backup_manager,
            validator,
            event_bus,
            audit_logger,
            install_root,
            temp_dir,
            installed_plugins: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// 安装插件
    /// 
    /// 完整的安装流程：
    /// 1. 获取插件包
    /// 2. 验证插件安全性
    /// 3. 解析插件定义
    /// 4. 检查已安装状态
    /// 5. 检查依赖
    /// 6. 创建安装目录
    /// 7. 复制文件
    /// 8. 创建数据库表
    /// 9. 注册插件
    /// 10. 保存数据库记录
    /// 11. 更新缓存
    /// 12. 记录审计日志
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：获取插件包
        let package_path = self.fetch_package(&request.source, request.version_constraint.as_deref()).await?;
        
        // 步骤2：验证插件安全性
        let validation_result = self.validator.validate_plugin_package(&package_path).await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }
        
        // 步骤3：解析插件定义
        let plugin_def = self.parse_plugin_definition(&package_path).await?;
        let plugin_id = plugin_def.id.clone();
        let version = plugin_def.version.clone().unwrap_or_else(|| "1.0.0".to_string());
        
        // 步骤4：检查已安装状态
        if !request.force {
            if self.is_plugin_installed(&plugin_id).await? {
                return Err(PluginError::plugin_already_exists(&plugin_id));
            }
        }
        
        // 步骤5：检查依赖
        let dep_result = self.check_dependencies(&plugin_def).await?;
        if !dep_result.satisfied {
            let missing: Vec<String> = dep_result.missing.iter()
                .map(|m| format!("{} ({})", m.plugin_id, m.required_by))
                .collect();
            return Err(PluginError::Dependency(format!(
                "缺少依赖插件: {}",
                missing.join(", ")
            )));
        }
        
        // 步骤6：创建安装目录
        let install_path = self.install_root.join(&plugin_id);
        if install_path.exists() && request.force {
            self.storage.remove_dir(&install_path)?;
        }
        self.storage.create_dir(&install_path)?;
        
        // 步骤7：复制文件
        self.copy_plugin_files(&package_path, &install_path).await?;
        
        // 步骤8：创建数据库表（如果需要）
        let db_id = request.db_id.unwrap_or_else(|| self.repository.default_db_id().to_string());
        if !plugin_def.table_config_files.is_empty() {
            self.create_plugin_tables(&plugin_def, &db_id, &install_path).await?;
        }
        
        // 步骤9：注册插件
        let plugin_info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: version.clone(),
            description: plugin_def.description.clone(),
            author: plugin_def.vendor_name.clone(),
            source: request.source.clone(),
            status: PluginStatus::Installed,
            installed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        };
        
        // 步骤10：保存数据库记录
        let db_record = PluginDbRecord {
            id: Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: version.clone(),
            wasm_path: install_path.join(&plugin_def.main_file).to_string_lossy().to_string(),
            install_path: install_path.to_string_lossy().to_string(),
            config_path: None,
            db_id: db_id.clone(),
            status: "installed".to_string(),
            is_system: false,
            is_locked: false,
            domain_code: plugin_def.domain_code.clone(),
            application_code: plugin_def.application_code.clone(),
            module_code: plugin_def.module_code.clone(),
            vendor_name: plugin_def.vendor_name.clone(),
            vendor_url: plugin_def.vendor_url.clone(),
            vendor_contact: plugin_def.vendor_contact.clone(),
            metadata: None,
            signature_algorithm: None,
            signer_key_id: None,
            activated_at: None,
            create_time: Utc::now(),
            update_time: Utc::now(),
        };
        
        self.repository.insert_plugin(&db_record).await?;
        
        // 步骤11：更新缓存
        self.cache.set(
            &format!("plugin:{}", plugin_id),
            CacheValue::Json(serde_json::to_value(&plugin_info).unwrap()),
            None,
        ).await;
        
        // 更新内存缓存
        {
            let mut installed = self.installed_plugins.write().await;
            installed.insert(plugin_id.clone(), plugin_info.clone());
        }
        
        // 步骤12：记录审计日志
        let audit_record = AuditRecord::success(
            plugin_id.clone(),
            OperationType::Install,
        ).with_details(serde_json::json!({
            "version": version,
            "install_path": install_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginInstalled,
            plugin_id.clone(),
            serde_json::json!({
                "version": version,
                "install_path": install_path.to_string_lossy().to_string(),
            }),
        )).await;
        
        // 如果需要自动激活
        if request.auto_activate {
            // 调用激活服务激活插件
            let activate_service = crate::service::activate::ActivateService::new(
                self.repository.clone(),
                self.cache.clone(),
                self.event_bus.clone(),
                self.audit_logger.clone(),
                std::sync::Arc::new(crate::runtime::activation::ActivationManager::new()),
                std::sync::Arc::new(crate::runtime::service_registry::ServiceRegistry::new()),
            );
            
            let activate_req = crate::service::activate::ActivateRequest {
                plugin_id: plugin_id.clone(),
                force: false,
            };
            
            if let Err(e) = activate_service.activate(activate_req).await {
                tracing::warn!("自动激活插件失败: {}", e);
            }
        }
        
        Ok(InstallResponse {
            plugin_id,
            install_path,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }
    
    /// 获取插件包
    async fn fetch_package(&self, source: &PluginSource, version_constraint: Option<&str>) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let fetcher = LocalFetcher::new(&self.install_root);
                fetcher.fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取本地插件包失败: {}", e)))
            }
            PluginSource::Remote { url, checksum } => {
                let fetcher = RemoteFetcher::new(self.temp_dir.clone());
                fetcher.fetch(&crate::fetcher::source::PluginSource::remote(url.clone(), checksum.clone()))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取远程插件包失败: {}", e)))
            }
            PluginSource::Registry { registry_url, package_name } => {
                let registry_info = crate::fetcher::registry::RegistryInfo::new(registry_url.clone().unwrap_or_default());
                let fetcher = crate::fetcher::registry::RegistryFetcher::new(registry_info, self.temp_dir.clone());
                
                fetcher.fetch_by_name(package_name, version_constraint.map(|s| s.to_string())).await
                    .map_err(|e| PluginError::Install(format!("从注册表获取插件包失败: {}", e)))
            }
        }
    }
    
    /// 解析插件定义
    async fn parse_plugin_definition(&self, package_path: &Path) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let manifest_path = package_path.join("manifest.json");
        
        if !manifest_path.exists() {
            return Err(PluginError::Metadata("插件定义文件 plugin.json 不存在".to_string()));
        }
        
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Metadata(format!("读取插件定义文件失败: {}", e)))?;
        
        let definition: cmx_core::model::meta::plugin::PluginDefinition = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析插件定义文件失败: {}", e)))?;
        
        Ok(definition)
    }
    
    /// 检查插件是否已安装
    pub async fn is_plugin_installed(&self, plugin_id: &str) -> PluginResult<bool> {
        // 先查内存缓存
        {
            let installed = self.installed_plugins.read().await;
            if installed.contains_key(plugin_id) {
                return Ok(true);
            }
        }
        
        // 再查数据库
        self.repository.plugin_exists(plugin_id).await
    }
    
    /// 复制插件文件
    async fn copy_plugin_files(&self, source: &Path, target: &Path) -> PluginResult<()> {
        if source.is_dir() {
            self.storage.copy_dir(source, target)
                .map_err(|e| PluginError::Install(format!("复制插件文件失败: {}", e)))?;
        } else if source.extension().map(|e| e == "zip").unwrap_or(false) {
            // 解压 ZIP 文件
            self.extract_zip(source, target).await?;
        }
        Ok(())
    }
    
    /// 解压 ZIP 文件
    async fn extract_zip(&self, zip_path: &Path, target: &Path) -> PluginResult<()> {
        cmx_utils::zip::ZipExtractor::extract(zip_path, target)
            .map_err(|e| PluginError::Install(format!("解压插件包失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 创建插件数据库表
    /// 
    /// 使用 cmx-metadata 解析表定义并创建数据库表。
    async fn create_plugin_tables(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        db_id: &str,
        install_path: &Path,
    ) -> PluginResult<()> {
        // 如果没有表配置文件，直接返回
        if plugin_def.table_config_files.is_empty() {
            return Ok(());
        }
        
        // 遍历所有表配置文件
        for table_config_file in &plugin_def.table_config_files {
            let config_path = install_path.join(table_config_file);
            
            if !config_path.exists() {
                tracing::warn!(
                    "表配置文件不存在: {}",
                    config_path.display()
                );
                continue;
            }
            
            // 读取配置文件内容
            let config_content = std::fs::read_to_string(&config_path)
                .map_err(|e| PluginError::Metadata(format!(
                    "读取表配置文件失败: {} - {}",
                    config_path.display(),
                    e
                )))?;
            
            // 解析表定义（尝试 JSON 和 TOML 格式）
            let table_def: cmx_core::model::cell::TableDefine = 
                serde_json::from_str(&config_content)
                    .or_else(|_| {
                        toml::from_str(&config_content)
                            .map_err(|e| PluginError::Metadata(format!(
                                "解析表配置文件失败: {} - {}",
                                config_path.display(),
                                e
                            )))
                    })?;
            
            // 创建表执行器
            let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, None);
            
            // 执行建表
            use cmx_core::model::meta::base::TableDefineDbExecutor;
            executor.create_table(&table_def)
                .map_err(|e| PluginError::Metadata(format!(
                    "创建表失败: {} - {}",
                    table_def.table_name,
                    e
                )))?;
            
            tracing::info!(
                "成功创建插件表: {} ({})",
                table_def.table_name,
                plugin_def.id
            );
        }
        
        Ok(())
    }
    
    /// 获取已安装插件列表
    pub async fn list_installed(&self) -> PluginResult<Vec<PluginInfo>> {
        let installed = self.installed_plugins.read().await;
        Ok(installed.values().cloned().collect())
    }
    
    /// 获取已安装插件
    pub async fn get_installed(&self, plugin_id: &str) -> PluginResult<Option<PluginInfo>> {
        let installed = self.installed_plugins.read().await;
        Ok(installed.get(plugin_id).cloned())
    }
    
    /// 刷新已安装插件缓存
    pub async fn refresh_installed_cache(&self) -> PluginResult<()> {
        let records = self.repository.list_plugins(&PluginFilter::default()).await?;
        
        let mut installed = self.installed_plugins.write().await;
        installed.clear();
        
        for record in records {
            let info = PluginInfo {
                id: record.plugin_id,
                name: record.name,
                version: record.version,
                description: None,
                author: record.vendor_name,
                source: PluginSource::Local { path: PathBuf::from(&record.install_path) },
                status: PluginStatus::Installed,
                installed_at: Some(record.create_time),
                updated_at: Some(record.update_time),
            };
            installed.insert(info.id.clone(), info);
        }
        
        Ok(())
    }
    
    /// 检查插件依赖
    /// 
    /// 验证插件的所有依赖是否已安装且版本满足约束。
    pub async fn check_dependencies(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<DependencyCheckResult> {
        let mut result = DependencyCheckResult::new();
        
        for dep in &plugin_def.dependencies {
            if dep.optional {
                continue;
            }
            
            let installed = self.is_plugin_installed(&dep.plugin_id).await?;
            
            if !installed {
                let version_constraint = dep.version_constraint.as_ref()
                    .and_then(|v| crate::domain::version::VersionConstraint::parse(v).ok());
                
                result.add_missing(MissingDependency {
                    plugin_id: dep.plugin_id.clone(),
                    version_constraint,
                    required_by: plugin_def.id.clone(),
                });
                continue;
            }
            
            if let Some(ref constraint_str) = dep.version_constraint {
                if let Ok(constraint) = crate::domain::version::VersionConstraint::parse(constraint_str) {
                    if let Some(plugin_info) = self.get_installed(&dep.plugin_id).await? {
                        if let Ok(installed_version) = crate::domain::version::SemanticVersion::parse(&plugin_info.version) {
                            if !constraint.satisfies(&installed_version) {
                                result.add_conflict(crate::domain::dependency::DependencyConflict {
                                    plugin_id: dep.plugin_id.clone(),
                                    constraints: vec![(plugin_def.id.clone(), constraint)],
                                });
                            }
                        }
                    }
                }
            }
        }
        
        Ok(result)
    }
}

impl Default for InstallService {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginRepository::default()),
            Arc::new(LayeredCacheManager::default()),
            Arc::new(FileStorage::new(Path::new(""))),
            Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            Arc::new(SecurityValidator::new()),
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::default()),
            PathBuf::from("./plugins"),
            PathBuf::from("./temp"),
        )
    }
}
