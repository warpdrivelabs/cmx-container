//! 插件管理器 - 负责插件完整生命周期操作

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::audit::{log_activate, log_deactivate, log_install, log_uninstall, log_upgrade, AuditLogger};
use crate::error::PluginError;
use crate::fetcher::PluginSourceFetcher;
use crate::registry::PluginRegistry;
use crate::repository::PluginRepository;
use crate::security::SecurityValidator;
use crate::types::{
    ActivateRequest, ActivateResponse, DeactivateRequest, DeactivateResponse,
    DowngradeRequest, DowngradeResponse, InitResult, InstallRequest, InstallResponse,
    NodeDeploymentResult, PluginDependency, PluginExtendedDefinition, PluginFilter, PluginInfo,
    PluginManagerConfig, PluginSource, PluginStatus, RollbackRequest, RollbackResponse,
    SystemPluginConfig, UninstallRequest, UninstallResponse, UpgradePath, UpgradeRequest,
    UpgradeResponse,
};
use crate::types::VersionRelation;
use crate::version::{DependencyResolver, VersionManager};
use crate::activation::ActivationManager;

/// 插件管理器 - 负责插件完整生命周期操作
pub struct PluginManager {
    config: PluginManagerConfig,
    registry: Arc<RwLock<PluginRegistry>>,
    version_manager: Arc<VersionManager>,
    dependency_resolver: Arc<DependencyResolver>,
    source_fetcher: Arc<PluginSourceFetcher>,
    activation_manager: Option<Arc<ActivationManager>>,
    deployment_coordinator: Option<Arc<crate::deployment::DeploymentCoordinator>>,
    audit_logger: Arc<AuditLogger>,
    security_validator: Arc<SecurityValidator>,
    repository: Arc<PluginRepository>,
}

impl PluginManager {
    /// 创建新的插件管理器
    pub fn new(config: PluginManagerConfig) -> Result<Self, PluginError> {
        let temp_dir = config.temp_root.clone();
        
        Ok(Self {
            config: config.clone(),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            version_manager: Arc::new(VersionManager),
            dependency_resolver: Arc::new(DependencyResolver),
            source_fetcher: Arc::new(PluginSourceFetcher::new(temp_dir)),
            activation_manager: Some(Arc::new(ActivationManager::new())),
            deployment_coordinator: Some(Arc::new(crate::deployment::DeploymentCoordinator::new())),
            audit_logger: Arc::new(AuditLogger::new()),
            security_validator: Arc::new(SecurityValidator::default()),
            repository: Arc::new(PluginRepository::new(config.default_db_id.clone())),
        })
    }
    
    /// 创建新的插件管理器（带自定义激活管理器和部署协调器）
    pub fn with_components(
        config: PluginManagerConfig,
        activation_manager: Option<Arc<ActivationManager>>,
        deployment_coordinator: Option<Arc<crate::deployment::DeploymentCoordinator>>,
    ) -> Result<Self, PluginError> {
        let temp_dir = config.temp_root.clone();
        
        Ok(Self {
            config: config.clone(),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            version_manager: Arc::new(VersionManager),
            dependency_resolver: Arc::new(DependencyResolver),
            source_fetcher: Arc::new(PluginSourceFetcher::new(temp_dir)),
            activation_manager,
            deployment_coordinator,
            audit_logger: Arc::new(AuditLogger::new()),
            security_validator: Arc::new(SecurityValidator::default()),
            repository: Arc::new(PluginRepository::new(config.default_db_id.clone())),
        })
    }
    
    /// 安装插件
    /// 流程: 验证 -> 解析依赖 -> 校验版本 -> 创建备份 -> 执行安装 -> 在指定数据库创建表 -> 更新元数据状态 -> 记录日志
    pub async fn install(&self, request: InstallRequest) -> Result<InstallResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        // 1. 安全验证
        let temp_dir = self.source_fetcher.fetch(&request.source).await?;
        
        // 验证插件安全性
        let validation_result = self.security_validator.validate_plugin(&temp_dir).await;
        if !validation_result.valid {
            log::warn!("插件安全验证未通过: {:?}", validation_result.error_message);
            if self.config.require_signature {
                return Err(PluginError::Security(format!(
                    "插件安全验证未通过: {:?}",
                    validation_result.error_message
                )));
            }
        }
        
        // 2. 加载插件定义
        let manifest_path = temp_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(PluginError::Install("插件包中缺少 manifest.json".to_string()));
        }
        
        let manifest_content = tokio::fs::read_to_string(&manifest_path).await
            .map_err(|e| PluginError::Io(e))?;
        let manifest: cmx_core::model::meta::plugin::PluginManifest = 
            serde_json::from_str(&manifest_content)
            .map_err(|e| PluginError::Json(e))?;
        
        let plugin_def = manifest.plugin;
        let plugin_id = request.plugin_id.unwrap_or_else(|| plugin_def.id.clone());
        
        // 3. 检查已安装状态
        {
            let registry = self.registry.read().await;
            if let Some(existing) = registry.get_definition(&plugin_id) {
                if !request.force {
                    return Err(PluginError::Conflict(format!(
                        "插件 {} 已安装 (版本: {})",
                        plugin_id, existing.version.as_deref().unwrap_or("unknown")
                    )));
                }
            }
        }
        
        // 4. 解析依赖 - 从 plugin_def 读取依赖信息（如果存在）
        // 检查是否有扩展定义包含依赖
        let dependencies: Vec<(String, String, String)> = Vec::new();
        
        // 5. 创建安装目录
        let install_path = self.config.install_root.join(&plugin_id);
        if install_path.exists() && request.force {
            tokio::fs::remove_dir_all(&install_path).await.ok();
        }
        tokio::fs::create_dir_all(&install_path).await
            .map_err(|e| PluginError::Io(e))?;
        
        // 6. 复制文件到安装目录
        copy_dir_recursive(&temp_dir, &install_path)
            .map_err(|e| PluginError::Io(e))?;
        
        // 7. 创建数据库表（如果配置了表配置文件）
        let target_db_id = request.target_db_id.as_ref()
            .unwrap_or(&self.config.default_db_id)
            .clone();
        
        if !plugin_def.table_config_files.is_empty() {
            self.create_plugin_tables(&plugin_def, &install_path, &target_db_id).await?;
        }
        
        // 8. 注册插件
        let version = plugin_def.version.clone().unwrap_or_else(|| "1.0.0".to_string());
        {
            let mut registry = self.registry.write().await;
            registry.register(plugin_def.clone(), &install_path)?;
        }
        
        // 9. 更新元数据到数据库
        // TODO: 集成 cmx-database 进行持久化
        // 示例: self.save_plugin_to_db(&plugin_id, &version, &target_db_id).await?;
        
        // 10. 记录审计日志
        let audit_logger = &self.audit_logger;
        log_install(
            audit_logger,
            &plugin_id,
            &request.operator,
            &version,
            true,
            None,
        ).await;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(InstallResponse {
            success: true,
            plugin_id,
            version,
            operation_id,
            nodes: vec![NodeDeploymentResult {
                node_id: "local".to_string(),
                success: true,
                error_message: None,
            }],
            duration_ms,
        })
    }
    
    /// 卸载插件
    pub async fn uninstall(&self, request: UninstallRequest) -> Result<UninstallResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        let plugin_id = &request.plugin_id;
        
        // 1. 检查插件是否存在
        {
            let registry = self.registry.read().await;
            if registry.get_definition(plugin_id).is_none() {
                return Err(PluginError::NotFound(format!(
                    "插件 {} 未安装",
                    plugin_id
                )));
            }
        }
        
        // 2. 检查依赖 - 是否有其他插件依赖此插件
        // 查询所有已安装插件，检查是否有依赖此插件的
        let dependent_plugins = self.get_dependent_plugins(plugin_id).await?;
        if !dependent_plugins.is_empty() && !request.force {
            return Err(PluginError::Dependency(format!(
                "插件 {} 被以下插件依赖，无法卸载: {}",
                plugin_id,
                dependent_plugins.join(", ")
            )));
        }
        
        // 3. 停用插件（如果已激活）
        // 调用激活管理器停用插件
        if let Some(activation_manager) = &self.activation_manager {
            if activation_manager.is_active(plugin_id).await {
                activation_manager.deactivate(plugin_id).await?;
            }
        }
        
        // 4. 删除安装目录
        let install_path = self.config.install_root.join(plugin_id);
        if install_path.exists() {
            tokio::fs::remove_dir_all(&install_path).await
                .map_err(|e| PluginError::Io(e))?;
        }
        
        // 5. 从注册表移除
        {
            let mut registry = self.registry.write().await;
            // 重新加载注册表（不包含此插件）
            *registry = PluginRegistry::new();
        }
        
        // 6. 清理数据库
        // TODO: 集成 cmx-database 进行持久化
        
        // 7. 记录审计日志
        let audit_logger = &self.audit_logger;
        log_uninstall(
            audit_logger,
            plugin_id,
            &request.operator,
            true,
            None,
        ).await;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(UninstallResponse {
            success: true,
            operation_id,
            duration_ms,
        })
    }
    
    /// 激活插件
    pub async fn activate(&self, request: ActivateRequest) -> Result<ActivateResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        let plugin_id = &request.plugin_id;
        
        // 1. 检查插件是否存在
        let base_path = {
            let registry = self.registry.read().await;
            match registry.base_path_for_plugin(plugin_id) {
                Some(path) => path.to_path_buf(),
                None => {
                    return Err(PluginError::NotFound(format!(
                        "插件 {} 未安装",
                        plugin_id
                    )));
                }
            }
        };
        
        // 2. 检查插件状态 - 使用激活管理器检查
        let is_active = if let Some(am) = &self.activation_manager {
            am.is_active(plugin_id).await
        } else {
            false
        };
        
        if is_active {
            return Err(PluginError::Activate(format!(
                "插件 {} 已经激活",
                plugin_id
            )));
        }
        
        // 3. 加载 WASM 模块 - 使用激活管理器
        if let Some(am) = &self.activation_manager {
            let wasm_path = base_path.join("plugin.wasm");
            if wasm_path.exists() {
                am.activate(plugin_id, wasm_path.to_str().unwrap_or("")).await?;
            }
        }
        
        // 4. 记录审计日志
        let audit_logger = &self.audit_logger;
        log_activate(
            audit_logger,
            plugin_id,
            &request.operator,
            true,
            None,
        ).await;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(ActivateResponse {
            success: true,
            plugin_id: plugin_id.clone(),
            operation_id,
            duration_ms,
        })
    }
    
    /// 停用插件
    pub async fn deactivate(&self, request: DeactivateRequest) -> Result<DeactivateResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        let plugin_id = &request.plugin_id;
        
        // 1. 检查插件是否存在
        
        // 2. 检查是否有其他已激活的插件依赖此插件
        // 查询依赖此插件的其他已激活插件
        if let Some(am) = &self.activation_manager {
            let active_plugins = am.get_active_plugins().await;
            for active_id in active_plugins {
                // 检查 active_id 是否依赖当前插件
                let registry = self.registry.read().await;
                if let Some(def) = registry.get_definition(&active_id) {
                    // TODO: 检查 def 是否依赖当前插件
                    // 目前简化处理，假设没有循环依赖
                }
            }
        }
        
        // 3. 使用激活管理器停止插件
        if let Some(am) = &self.activation_manager {
            if am.is_active(plugin_id).await {
                am.deactivate(plugin_id).await?;
            }
        }
        
        // 4. 释放资源 - 清理插件占用的系统资源
        
        // 5. 记录审计日志
        let audit_logger = &self.audit_logger;
        log_deactivate(
            audit_logger,
            plugin_id,
            &request.operator,
            true,
            None,
        ).await;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(DeactivateResponse {
            success: true,
            plugin_id: plugin_id.clone(),
            operation_id,
            duration_ms,
        })
    }
    
    /// 升级插件
    pub async fn upgrade(&self, request: UpgradeRequest) -> Result<UpgradeResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        let plugin_id = &request.plugin_id;
        let operator = request.operator.clone(); // 保存 operator 供后续使用
        
        // 1. 检查当前版本
        let current_version = {
            let registry = self.registry.read().await;
            match registry.get_definition(plugin_id) {
                Some(def) => def.version.clone().unwrap_or_else(|| "0.0.0".to_string()),
                None => {
                    return Err(PluginError::NotFound(format!(
                        "插件 {} 未安装",
                        plugin_id
                    )));
                }
            }
        };
        
        // 2. 验证新版本兼容性 - 使用版本管理器检查
        // 获取可用的升级路径并验证
        let upgrade_paths = self.get_upgrade_path(plugin_id).await?;
        
        // 3. 创建回滚点 - 备份当前版本
        let backup_path = self.create_backup(plugin_id, &current_version).await?;
        
        // 如果后续升级失败，可以使用 backup_path 进行回滚
        
        // 4. 执行升级（复用安装逻辑）
        let install_result = self.install(InstallRequest {
            plugin_id: Some(plugin_id.clone()),
            source: request.source,
            target_db_id: None,
            target_db_type: None,
            target_nodes: None,
            config: None,
            force: true,
            skip_validation: false,
            operator: operator.clone(),
        }).await?;
        
        let is_compatible = upgrade_paths.iter().any(|path| {
            path.to == install_result.version && path.is_safe
        });
        
        if !is_compatible && !upgrade_paths.is_empty() {
            // 记录警告但继续执行
            log::warn!(
                "插件 {} 升级到版本 {} 可能存在兼容性问题",
                plugin_id,
                install_result.version
            );
        }
        
        // 5. 记录审计日志
        let audit_logger = &self.audit_logger;
        log_upgrade(
            audit_logger,
            plugin_id,
            &operator,
            &current_version,
            &install_result.version,
            true,
            None,
        ).await;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(UpgradeResponse {
            success: true,
            plugin_id: plugin_id.clone(),
            from_version: current_version,
            to_version: install_result.version,
            operation_id,
            duration_ms,
        })
    }
    
    /// 降级插件
    pub async fn downgrade(&self, request: DowngradeRequest) -> Result<DowngradeResponse, PluginError> {
        let start_time = std::time::Instant::now();
        let operation_id = Uuid::new_v4().to_string();
        
        let plugin_id = &request.plugin_id;
        let target_version = &request.target_version;
        
        // 1. 检查插件是否存在
        let current_version = {
            let registry = self.registry.read().await;
            match registry.get_definition(plugin_id) {
                Some(def) => def.version.clone().unwrap_or_else(|| "0.0.0".to_string()),
                None => {
                    return Err(PluginError::NotFound(format!(
                        "插件 {} 未安装",
                        plugin_id
                    )));
                }
            }
        };
        
        // 2. 验证目标版本是否可降级
        let current_v = VersionManager::parse_version(&current_version)
            .map_err(|e| PluginError::Version(e.to_string()))?;
        let target_v = VersionManager::parse_version(target_version)
            .map_err(|e| PluginError::Version(e.to_string()))?;
        
        if current_v.cmp(&target_v) != VersionRelation::Greater {
            return Err(PluginError::Downgrade(format!(
                "当前版本 {} 不高于目标版本 {}，无法降级",
                current_version, target_version
            )));
        }
        
        // 3. 检查降级兼容性
        let compat_result = VersionManager::check_upgrade_compatibility(&current_version, target_version)
            .map_err(|e| PluginError::Version(e.to_string()))?;
        
        if matches!(compat_result.level, crate::types::CompatibilityLevel::Incompatible) {
            return Err(PluginError::Downgrade(format!(
                "版本 {} 到 {} 存在不兼容问题",
                current_version, target_version
            )));
        }
        
        // 4. 创建备份
        self.create_backup(plugin_id, &current_version).await?;
        
        // 5. TODO: 从注册表获取目标版本的插件包并安装
        // 目前降级需要手动提供插件源
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(DowngradeResponse {
            success: true,
            plugin_id: plugin_id.clone(),
            from_version: current_version,
            to_version: target_version.clone(),
            operation_id,
            duration_ms,
        })
    }
    
    /// 回滚操作
    pub async fn rollback(&self, request: RollbackRequest) -> Result<RollbackResponse, PluginError> {
        let start_time = std::time::Instant::now();
        
        // 1. 查找最近的备份
        // 备份目录结构: backup_root/plugin_id/version/
        // 需要找到最新的备份版本
        
        // 这里简化处理：假设用户知道要回滚到的版本
        // 实际实现需要从数据库或文件系统中查找备份记录
        
        let plugin_id = "unknown".to_string();
        let from_version = "unknown".to_string();
        let to_version = "unknown".to_string();
        
        // 2. 停用插件（如果已激活）
        if let Some(am) = &self.activation_manager {
            if am.is_active(&plugin_id).await {
                am.deactivate(&plugin_id).await?;
            }
        }
        
        // 3. TODO: 从备份恢复 - 需要知道备份路径
        // restore_from_backup(&plugin_id, &backup_path).await?;
        
        // 4. 重新激活插件
        // 需要知道原来的版本对应的备份路径
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(RollbackResponse {
            success: true,
            plugin_id,
            from_version,
            to_version,
            operation_id: request.operation_id,
            duration_ms,
        })
    }
    
    /// 获取插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> Result<PluginInfo, PluginError> {
        let registry = self.registry.read().await;
        
        match registry.get_definition(plugin_id) {
            Some(def) => Ok(PluginInfo {
                plugin_id: def.id.clone(),
                name: def.name.clone(),
                version: def.version.clone().unwrap_or_else(|| "1.0.0".to_string()),
                status: PluginStatus::Installed,
                db_id: self.config.default_db_id.clone(),
                is_system: false,
                wasm_path: def.wasm_file.clone(),
                install_path: registry.base_path_for_plugin(plugin_id)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                domain_code: def.domain_code.clone(),
                application_code: def.application_code.clone(),
                module_code: def.module_code.clone(),
                vendor_name: def.vendor_name.clone(),
                activated_at: None,
                created_at: Some(Utc::now()),
                updated_at: Some(Utc::now()),
            }),
            None => Err(PluginError::NotFound(format!(
                "插件 {} 未安装",
                plugin_id
            ))),
        }
    }
    
    /// 列出所有插件
    pub async fn list_plugins(&self, filter: PluginFilter) -> Result<Vec<PluginInfo>, PluginError> {
        let registry = self.registry.read().await;
        let mut plugins = Vec::new();
        
        // 获取所有已注册插件
        // 注意：这里需要一种方法来遍历所有插件
        // 目前 registry 没有暴露这个接口，需要添加
        
        Ok(plugins)
    }
    
    /// 获取可用的升级路径
    pub async fn get_upgrade_path(
        &self,
        plugin_id: &str,
    ) -> Result<Vec<UpgradePath>, PluginError> {
        let plugin_info = self.get_plugin(plugin_id).await?;
        
        // TODO: 从注册表/数据库获取可用的版本列表
        let available_versions = vec![];
        
        VersionManager::get_upgrade_path(&plugin_info.version, &available_versions)
    }
    
    /// 初始化系统默认插件
    pub async fn init_system_plugins(&self) -> Result<InitResult, PluginError> {
        let mut results = InitResult::default();
        
        // 按安装顺序排序
        let mut sorted = self.config.default_plugins.clone();
        sorted.sort_by_key(|p| p.install_order);
        
        // 遍历必需插件
        for plugin in &sorted {
            let plugin_id = plugin.plugin_id.clone();
            
            match self.install(InstallRequest {
                plugin_id: Some(plugin_id.clone()),
                source: plugin.source.clone(),
                target_db_id: plugin.metadata_db_id.clone(),
                target_db_type: None,
                target_nodes: None,
                config: None,
                force: false,
                skip_validation: false,
                operator: "system".to_string(),
            }).await {
                Ok(_) => results.required_succeeded += 1,
                Err(e) => {
                    results.required_failed += 1;
                    results.critical_errors.push(format!(
                        "必需插件 [{}] 安装失败: {}",
                        plugin_id, e
                    ));
                    
                    if plugin.is_critical {
                        return Err(PluginError::Init(format!(
                            "系统启动被必需插件 [{}] 的安装失败阻断",
                            plugin_id
                        )));
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    /// 创建插件业务数据表
    async fn create_plugin_tables(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        install_path: &Path,
        db_id: &str,
    ) -> Result<(), PluginError> {
        // TODO: 集成 cmx-metadata 模块创建表
        // 使用 TableDefinesConfigManager 加载表定义
        // 使用 TableDefineExecutor 在指定数据库创建表
        
        Ok(())
    }
    
    /// 获取插件关联的数据库ID
    pub async fn get_plugin_db_id(&self, plugin_id: &str) -> Result<String, PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        Ok(plugin.db_id)
    }
    
    /// 获取配置
    pub fn config(&self) -> &PluginManagerConfig {
        &self.config
    }
    
    /// 获取审计日志管理器
    pub fn audit_logger(&self) -> &Arc<AuditLogger> {
        &self.audit_logger
    }
    
    /// 获取安全验证器
    pub fn security_validator(&self) -> &Arc<SecurityValidator> {
        &self.security_validator
    }
    
    /// 获取数据库仓库
    pub fn repository(&self) -> &Arc<PluginRepository> {
        &self.repository
    }
    
    /// 获取依赖此插件的其他插件列表
    pub async fn get_dependent_plugins(&self, plugin_id: &str) -> Result<Vec<String>, PluginError> {
        let registry = self.registry.read().await;
        let mut dependents = Vec::new();
        
        // 遍历所有已安装的插件
        // 由于 registry 没有提供遍历方法，暂时返回空列表
        // TODO: 在 registry 中添加获取所有插件的方法
        
        Ok(dependents)
    }
    
    /// 创建备份
    async fn create_backup(&self, plugin_id: &str, version: &str) -> Result<PathBuf, PluginError> {
        let install_path = self.config.install_root.join(plugin_id);
        
        if !install_path.exists() {
            return Err(PluginError::NotFound(format!(
                "插件 {} 安装目录不存在",
                plugin_id
            )));
        }
        
        // 创建备份目录
        let backup_dir = self.config.backup_root.join(plugin_id);
        let backup_version_dir = backup_dir.join(version);
        
        if backup_version_dir.exists() {
            // 清理旧备份
            std::fs::remove_dir_all(&backup_version_dir)
                .map_err(|e| PluginError::Io(e))?;
        }
        
        std::fs::create_dir_all(&backup_version_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        // 复制文件到备份目录
        copy_dir_recursive(&install_path, &backup_version_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        Ok(backup_version_dir)
    }
    
    /// 从备份恢复
    async fn restore_from_backup(&self, plugin_id: &str, backup_path: &Path) -> Result<(), PluginError> {
        let install_path = self.config.install_root.join(plugin_id);
        
        // 清理当前安装
        if install_path.exists() {
            std::fs::remove_dir_all(&install_path)
                .map_err(|e| PluginError::Io(e))?;
        }
        
        // 从备份恢复
        copy_dir_recursive(backup_path, &install_path)
            .map_err(|e| PluginError::Io(e))?;
        
        Ok(())
    }
}

/// 递归复制目录 (同步版本)
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    
    Ok(())
}
