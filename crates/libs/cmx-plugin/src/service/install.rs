//! 安装服务模块
//!
//! 处理插件安装流程

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_metadata::config::{load_table_defines_config_from_path, TableDefinesConfigManager};
use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::domain::dependency::{DependencyCheckResult, MissingDependency};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::security::validator::SecurityValidator;
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::core::context::PluginContext;
use crate::fetcher::local::LocalFetcher;
use crate::fetcher::remote::RemoteFetcher;
use crate::GlobalPluginManager;

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

/// 安装服务依赖
pub struct InstallServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 备份管理器
    pub backup_manager: Arc<BackupManager>,
    /// 安全验证器
    pub security_validator: Arc<SecurityValidator>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
}

/// 安装服务
pub struct InstallService {
    deps: InstallServiceDeps,
}

impl InstallService {
    /// 创建新的安装服务
    pub fn new(deps: InstallServiceDeps) -> Self {
        Self { deps }
    }

    /// 执行安装操作
    ///
    /// 完整流程：
    /// 1. 获取插件包（zip 或文件夹）
    /// 2. 如果是 zip，解压到临时目录
    /// 3. 在临时目录进行安全验证和元数据解析
    /// 4. 检查已安装状态
    /// 5. 检查依赖
    /// 6. 创建安装目录
    /// 7. 复制文件到安装目录
    /// 8. 创建数据库表
    /// 9. 注册插件
    /// 10. 保存数据库记录
    /// 11. 更新缓存
    /// 12. 记录审计日志
    /// 13. 清理临时目录
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1：获取插件包
        let package_path = self
            .fetch_package(&request.source, request.version_constraint.as_deref())
            .await?;

        // 步骤2：准备临时目录（如果是 zip 则解压，如果是文件夹则直接使用）
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .prepare_package_for_validation(&package_path, &temp_dir)
            .await?;

        // 使用 RAII 确保临时目录被清理
        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤3：验证插件安全性（在解压后的目录进行）
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 步骤4：解析插件定义（在解压后的目录进行）
        let plugin_def = self.parse_plugin_definition(&extract_path).await?;
        let plugin_id = plugin_def.id.clone();
        let version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤5：检查已安装状态
        if !request.force {
            if self.is_plugin_installed(&plugin_id).await? {
                return Err(PluginError::plugin_already_exists(&plugin_id));
            }
        }

        // 步骤6：检查依赖
        let dep_result = self.check_plugin_dependencies(&plugin_def).await?;
        if !dep_result.satisfied {
            let missing: Vec<String> = dep_result
                .missing
                .iter()
                .map(|m| format!("{} ({})", m.plugin_id, m.required_by))
                .collect();
            return Err(PluginError::Dependency(format!(
                "缺少依赖插件: {}",
                missing.join(", ")
            )));
        }

        // 步骤7：创建安装目录
        let install_path = self.deps.plugin_root.join(&plugin_id);
        if install_path.exists() && request.force {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 步骤8：复制文件（从解压目录到安装目录）
        self.copy_plugin_files(&extract_path, &install_path).await?;

        // 步骤9：创建数据库表（如果需要）
        let db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.deps.default_database_id.clone());
        if !plugin_def.table_config_files.is_empty() {
            self.create_plugin_tables(&plugin_def, &db_id, &install_path)
                .await?;
        }

        // 步骤10：注册插件
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

        // 步骤11：保存数据库记录
        let db_record = crate::infrastructure::database::repository::PluginDbRecord {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: version.clone(),
            wasm_path: install_path
                .join(&plugin_def.main_file)
                .to_string_lossy()
                .to_string(),
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

        self.deps.repository.insert_plugin(&db_record).await?;

        // 更新注册表和上下文
        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info.clone());
        }

        {
            let mut contexts = self.deps.contexts.write().await;
            let context = PluginContext::from_db_record(&db_record);
            contexts.insert(plugin_id.clone(), context);
        }

        // 步骤12：更新缓存
        self.deps
            .cache
            .set(
                &format!("plugin:{}", plugin_id),
                crate::infrastructure::cache::layered::CacheValue::Json(
                    serde_json::to_value(&plugin_info).unwrap(),
                ),
                None,
            )
            .await;

        // 步骤13：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Install,
        )
        .with_details(serde_json::json!({
            "version": version,
            "install_path": install_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        // 发布事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginInstalled,
                plugin_id.clone(),
                serde_json::json!({
                    "version": version,
                    "install_path": install_path.to_string_lossy().to_string(),
                }),
            ))
            .await;

        Ok(InstallResponse {
            plugin_id,
            install_path,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }

    /// 获取插件包
    async fn fetch_package(
        &self,
        source: &PluginSource,
        version_constraint: Option<&str>,
    ) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let fetcher = LocalFetcher::new(&self.deps.plugin_root);
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取本地插件包失败: {}", e)))
            }
            PluginSource::Remote { url, checksum } => {
                let fetcher = RemoteFetcher::new(self.deps.temp_root.clone());
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::remote(
                        url.clone(),
                        checksum.clone(),
                    ))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取远程插件包失败: {}", e)))
            }
            PluginSource::Registry {
                registry_url,
                package_name,
            } => {
                let registry_info = crate::fetcher::registry::RegistryInfo::new(
                    registry_url.clone().unwrap_or_default(),
                );
                let fetcher = crate::fetcher::registry::RegistryFetcher::new(
                    registry_info,
                    self.deps.temp_root.clone(),
                );

                fetcher
                    .fetch_by_name(package_name, version_constraint.map(|s| s.to_string()))
                    .await
                    .map_err(|e| PluginError::Install(format!("从注册表获取插件包失败: {}", e)))
            }
        }
    }

    /// 准备插件包用于验证
    ///
    /// 如果是 zip 包，解压到临时目录；如果是文件夹，直接返回路径。
    async fn prepare_package_for_validation(
        &self,
        package_path: &std::path::Path,
        temp_dir: &std::path::Path,
    ) -> PluginResult<(std::path::PathBuf, bool)> {
        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            std::fs::create_dir_all(temp_dir)
                .map_err(|e| PluginError::Install(format!("创建临时目录失败: {}", e)))?;

            self.extract_zip(package_path, temp_dir).await?;

            let extract_path = self.find_plugin_root_in_dir(temp_dir)?;

            tracing::info!("插件包已解压到临时目录: {}", extract_path.display());

            Ok((extract_path, true))
        } else if package_path.is_dir() {
            Ok((package_path.to_path_buf(), false))
        } else {
            Err(PluginError::Install(format!(
                "不支持的插件包格式: {}",
                package_path.display()
            )))
        }
    }

    /// 在解压目录中查找插件根目录
    fn find_plugin_root_in_dir(
        &self,
        dir: &std::path::Path,
    ) -> PluginResult<std::path::PathBuf> {
        if dir.join("manifest.json").exists() {
            return Ok(dir.to_path_buf());
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("manifest.json").exists() {
                        return Ok(path);
                    }
                    if let Ok(found) = self.find_plugin_root_in_dir(&path) {
                        return Ok(found);
                    }
                }
            }
        }

        Ok(dir.to_path_buf())
    }

    /// 解析插件定义
    ///
    /// 从 manifest.json 中解析插件定义。
    async fn parse_plugin_definition(
        &self,
        package_path: &std::path::Path,
    ) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let manifest_path = package_path.join("manifest.json");

        if !manifest_path.exists() {
            return Err(PluginError::Metadata(
                "插件定义文件 manifest.json 不存在".to_string(),
            ));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Metadata(format!("读取插件定义文件失败: {}", e)))?;

        let manifest: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析 manifest.json 失败: {}", e)))?;

        let plugin_value = manifest.get("plugin").ok_or_else(|| {
            PluginError::Metadata("manifest.json 缺少 plugin 对象".to_string())
        })?;

        let definition: cmx_core::model::meta::plugin::PluginDefinition =
            serde_json::from_value(plugin_value.clone())
                .map_err(|e| PluginError::Metadata(format!("解析 plugin 定义失败: {}", e)))?;

        Ok(definition)
    }

    /// 检查插件是否已安装
    async fn is_plugin_installed(&self, plugin_id: &str) -> PluginResult<bool> {
        self.deps.repository.plugin_exists(plugin_id).await
    }

    /// 检查插件依赖
    ///
    /// 验证插件的所有依赖是否已安装且版本满足约束。
    async fn check_plugin_dependencies(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<crate::domain::dependency::DependencyCheckResult> {
        use crate::domain::dependency::{
            DependencyCheckResult, DependencyConflict, MissingDependency,
        };

        let mut result = DependencyCheckResult::new();

        for dep in &plugin_def.dependencies {
            if dep.optional {
                continue;
            }

            let installed = self.is_plugin_installed(&dep.plugin_id).await?;

            if !installed {
                let version_constraint = dep
                    .version_constraint
                    .as_ref()
                    .and_then(|v| crate::domain::version::VersionConstraint::parse(v).ok());

                result.add_missing(MissingDependency {
                    plugin_id: dep.plugin_id.clone(),
                    version_constraint,
                    required_by: plugin_def.id.clone(),
                });
                continue;
            }

            if let Some(ref constraint_str) = dep.version_constraint {
                if let Ok(constraint) =
                    crate::domain::version::VersionConstraint::parse(constraint_str)
                {
                    if let Some(plugin_info) = GlobalPluginManager::get().await
                        .get_plugin(&dep.plugin_id).await? {
                        if let Ok(installed_version) =
                            crate::domain::version::SemanticVersion::parse(&plugin_info.version)
                        {
                            if !constraint.satisfies(&installed_version) {
                                result.add_conflict(DependencyConflict {
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

    /// 复制插件文件
    async fn copy_plugin_files(
        &self,
        source: &std::path::Path,
        target: &std::path::Path,
    ) -> PluginResult<()> {
        if source.is_dir() {
            self.deps
                .storage
                .copy_dir(source, target)
                .map_err(|e| PluginError::Install(format!("复制插件文件失败: {}", e)))?;
        }
        Ok(())
    }

    /// 解压 ZIP 文件
    async fn extract_zip(
        &self,
        zip_path: &std::path::Path,
        target: &std::path::Path,
    ) -> PluginResult<()> {
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
        install_path: &std::path::Path,
    ) -> PluginResult<()> {
        // 如果没有表配置文件，直接返回
        if plugin_def.table_config_files.is_empty() {
            return Ok(());
        }

        let mut table_config_manager = TableDefinesConfigManager::new();
        let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, None);
        for table_config_file in &plugin_def.table_config_files {
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

        //开始创建表
        // let table_defs = table_config_manager.load_all_tables(install_path)
        //     .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;

        // 遍历所有表配置文件
        // for table_config_file in &plugin_def.table_config_files {
        //     let config_path = install_path.join(table_config_file);
        //
        //     if !config_path.exists() {
        //         tracing::warn!(
        //             "表配置文件不存在: {}",
        //             config_path.display()
        //         );
        //         continue;
        //     }
        //
        //     // 读取配置文件内容
        //     let config_content = std::fs::read_to_string(&config_path)
        //         .map_err(|e| PluginError::Metadata(format!(
        //             "读取表配置文件失败: {} - {}",
        //             config_path.display(),
        //             e
        //         )))?;
        //
        //
        //
        //     // 解析表定义
        //     let table_def: cmx_core::model::cell::TableDefine =
        //         serde_json::from_str(&config_content)
        //             .or_else(|e| {
        //                 // `e` 就是 serde_json::from_str 返回的错误
        //                 eprintln!("TableDefine JSON 解析失败: {}", e);
        //
        //             Err(     PluginError::Metadata(format!(
        //                 "解析表配置文件失败: {} - {}",
        //                 config_path.display(),
        //                 e
        //             )))
        //                 // // 尝试作为 TOML 解析
        //                 // toml::from_str(&config_content)
        //                 //     .map_err(|e| PluginError::Metadata(format!(
        //                 //         "解析表配置文件失败: {} - {}",
        //                 //         config_path.display(),
        //                 //         e
        //                 //     )))
        //             })?;
        //
        //     // 创建表执行器
        //     let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, None);
        //
        //     // 执行建表
        //     use cmx_core::model::meta::base::TableDefineDbExecutor;
        //     executor.create_table(&table_def)
        //         .map_err(|e| PluginError::Metadata(format!(
        //             "创建表失败: {} - {}",
        //             table_def.table_name,
        //             e
        //         )))?;
        //
        //     tracing::info!(
        //         "成功创建插件表: {} ({})",
        //         table_def.table_name,
        //         plugin_def.id
        //     );
        // }

        Ok(())
    }
}

impl Default for InstallService {
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(InstallServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            security_validator: Arc::new(SecurityValidator::new()),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            plugin_root: PathBuf::from("./plugins"),
            temp_root: PathBuf::from("./temp"),
            default_database_id: "default".to_string(),
        })
    }
}
