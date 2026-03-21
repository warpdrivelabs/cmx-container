//! 安装服务模块
//!
//! 处理插件安装流程，提供完整的插件安装功能。
//!
//! # 功能概述
//!
//! - 从不同来源获取插件包（本地、远程、注册表）
//! - 安全验证插件包
//! - 检查依赖关系
//! - 创建数据库表
//! - 注册插件到系统
//!
//! # 安装流程
//!
//! 1. 获取插件包
//! 2. 解压到临时目录（如果是 ZIP）
//! 3. 安全验证
//! 4. 解析插件定义
//! 5. 检查已安装状态
//! 6. 检查依赖
//! 7. 创建安装目录
//! 8. 复制文件
//! 9. 创建数据库表
//! 10. 注册插件
//! 11. 保存数据库记录
//! 12. 更新缓存
//! 13. 记录审计日志
//! 14. 清理临时目录

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_metadata::config::{load_table_defines_config_from_path, TableDefinesConfigManager};
use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
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
use crate::common::{PackageUtils, DefinitionUtils, DependencyUtils, PackageUtilsDeps, DependencyUtilsDeps};
use crate::GlobalPluginManager;

/// 安装请求
///
/// 包含插件安装所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件来源
    ///
    /// 支持三种来源类型：
    /// - `Local { path }`: 本地文件路径
    /// - `Remote { url, checksum }`: 远程 URL
    /// - `Registry { registry_url, package_name }`: 插件注册表
    pub source: PluginSource,

    /// 目标数据库ID
    ///
    /// 指定插件数据表创建的目标数据库。
    /// 如果未指定，使用默认数据库。
    pub db_id: Option<String>,

    /// 是否强制安装
    ///
    /// - `true`: 覆盖已存在的同名插件
    /// - `false`: 如果插件已存在则返回错误
    pub force: bool,

    /// 是否自动激活
    ///
    /// 安装完成后是否自动激活插件。
    /// 注意：自动激活需要所有依赖已满足。
    pub auto_activate: bool,

    /// 版本约束
    ///
    /// 仅对注册表来源有效。
    /// 支持语义化版本约束，如 "^1.0.0"、">=2.0.0"。
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 安装响应
///
/// 包含安装操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 插件ID
    ///
    /// 安装成功的插件唯一标识符。
    pub plugin_id: String,

    /// 安装路径
    ///
    /// 插件文件在文件系统中的安装位置。
    pub install_path: PathBuf,

    /// 是否成功
    ///
    /// 指示安装操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 安装结果的描述性消息。
    pub message: String,
}

/// 安装服务依赖
///
/// 包含安装服务运行所需的所有依赖项。
pub struct InstallServiceDeps {
    /// 数据仓库
    ///
    /// 用于持久化插件元数据和查询插件信息。
    pub repository: Arc<PluginRepository>,

    /// 缓存管理器
    ///
    /// 用于缓存插件信息，提高查询性能。
    pub cache: Arc<LayeredCacheManager>,

    /// 文件存储
    ///
    /// 用于执行文件系统操作（创建目录、复制文件等）。
    pub storage: Arc<FileStorage>,

    /// 备份管理器
    ///
    /// 用于管理插件备份（安装过程中暂不使用）。
    pub backup_manager: Arc<BackupManager>,

    /// 安全验证器
    ///
    /// 用于验证插件包的安全性。
    pub security_validator: Arc<SecurityValidator>,

    /// 事件总线
    ///
    /// 用于发布插件安装事件。
    pub event_bus: Arc<EventBus>,

    /// 审计日志
    ///
    /// 用于记录安装操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 插件注册表
    ///
    /// 用于在内存中管理已注册的插件。
    pub registry: Arc<RwLock<PluginRegistry>>,

    /// 插件上下文映射
    ///
    /// 存储插件的运行时上下文信息。
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,

    /// 安装根目录
    ///
    /// 所有插件的安装根路径。
    pub plugin_root: PathBuf,

    /// 临时目录
    ///
    /// 用于存储临时文件（解压、下载等）。
    pub temp_root: PathBuf,

    /// 默认数据库ID
    ///
    /// 未指定数据库时使用的默认数据库标识。
    pub default_database_id: String,
}

/// 安装服务
///
/// 提供插件安装功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::install::{InstallService, InstallServiceDeps, InstallRequest};
/// use cmx_plugin::domain::plugin::PluginSource;
/// use std::path::PathBuf;
///
/// # async fn example(service: &InstallService) -> Result<(), cmx_plugin::error::PluginError> {
/// let request = InstallRequest {
///     source: PluginSource::Local {
///         path: PathBuf::from("./my-plugin.zip"),
///     },
///     db_id: None,
///     force: false,
///     auto_activate: false,
///     version_constraint: None,
/// };
///
/// let response = service.install(request).await?;
/// println!("插件 {} 安装成功", response.plugin_id);
/// # Ok(())
/// # }
/// ```
pub struct InstallService {
    deps: InstallServiceDeps,
    package_utils: PackageUtils,
    dependency_utils: DependencyUtils,
}

impl InstallService {
    /// 创建新的安装服务
    ///
    /// # 参数
    ///
    /// * `deps` - 安装服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的安装服务实例
    pub fn new(deps: InstallServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: Some(deps.storage.clone()),
        });
        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: deps.repository.clone(),
        });
        Self { deps, package_utils, dependency_utils }
    }

    /// 执行安装操作
    ///
    /// 执行完整的插件安装流程。
    ///
    /// # 参数
    ///
    /// * `request` - 安装请求，包含来源、选项等参数
    ///
    /// # 返回值
    ///
    /// 返回安装响应，包含插件ID、安装路径等信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::Install`: 获取插件包失败
    /// - `PluginError::Security`: 安全验证失败
    /// - `PluginError::PluginAlreadyExists`: 插件已存在（非强制模式）
    /// - `PluginError::Dependency`: 依赖检查失败
    /// - `PluginError::Metadata`: 元数据解析失败
    ///
    /// # 流程说明
    ///
    /// 1. **获取插件包**: 根据来源类型获取插件包路径
    /// 2. **准备验证环境**: 如果是 ZIP，解压到临时目录
    /// 3. **安全验证**: 验证插件包的安全性
    /// 4. **解析定义**: 读取 manifest.json 获取插件元数据
    /// 5. **检查已安装**: 验证插件是否已存在
    /// 6. **检查依赖**: 验证所有依赖是否满足
    /// 7. **创建目录**: 创建插件安装目录
    /// 8. **复制文件**: 将插件文件复制到安装目录
    /// 9. **创建表**: 根据配置创建数据库表
    /// 10. **注册插件**: 更新内存注册表
    /// 11. **保存记录**: 持久化插件信息到数据库
    /// 12. **更新缓存**: 更新插件缓存
    /// 13. **审计日志**: 记录安装操作
    /// 14. **发布事件**: 通知其他组件
    /// 15. **清理临时文件**: 自动清理临时目录
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_plugin::service::install::{InstallService, InstallRequest};
    /// # use cmx_plugin::domain::plugin::PluginSource;
    /// # async fn example(service: &InstallService) -> Result<(), cmx_plugin::error::PluginError> {
    /// let request = InstallRequest {
    ///     source: PluginSource::Remote {
    ///         url: "https://example.com/plugin.zip".to_string(),
    ///         checksum: Some("sha256:abc123".to_string()),
    ///     },
    ///     db_id: Some("main_db".to_string()),
    ///     force: false,
    ///     auto_activate: true,
    ///     version_constraint: None,
    /// };
    ///
    /// let response = service.install(request).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();

        let package_path = self
            .package_utils
            .fetch_package(&request.source, request.version_constraint.as_deref(), "安装")
            .await?;

        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "安装")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let plugin_id = plugin_def.id.clone();
        let version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        if !request.force {
            if self.is_plugin_installed(&plugin_id).await? {
                return Err(PluginError::plugin_already_exists(&plugin_id));
            }
        }

        let dep_result = self.dependency_utils.check_plugin_dependencies(&plugin_def, |plugin_id| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                GlobalPluginManager::get().await.get_plugin(plugin_id).await
            })
        }).await?;
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

        let install_path = self.deps.plugin_root.join(&plugin_id);
        if install_path.exists() && request.force {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        self.package_utils.copy_plugin_files(&extract_path, &install_path, "安装")?;

        let db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.deps.default_database_id.clone());
        if !plugin_def.table_config_files.is_empty() {
            self.create_plugin_tables(&plugin_def, &db_id, &install_path)
                .await?;
        }

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

        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info.clone());
        }

        {
            let mut contexts = self.deps.contexts.write().await;
            let context = PluginContext::from_db_record(&db_record);
            contexts.insert(plugin_id.clone(), context);
        }

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

    /// 检查插件是否已安装
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要检查的插件 ID
    ///
    /// # 返回值
    ///
    /// - `true`: 插件已安装
    /// - `false`: 插件未安装
    async fn is_plugin_installed(&self, plugin_id: &str) -> PluginResult<bool> {
        self.deps.repository.plugin_exists(plugin_id).await
    }

    /// 创建插件数据库表
    ///
    /// 使用 cmx-metadata 解析表定义并创建数据库表。
    ///
    /// # 参数
    ///
    /// * `plugin_def` - 插件定义，包含表配置文件列表
    /// * `db_id` - 目标数据库 ID
    /// * `install_path` - 插件安装路径，用于定位表配置文件
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - `PluginError::Metadata`: 加载表配置文件失败
    /// - `PluginError::Metadata`: 创建或升级表失败
    ///
    /// # 说明
    ///
    /// 此方法会：
    /// 1. 遍历 `table_config_files` 列表
    /// 2. 加载每个表配置文件
    /// 3. 解析表定义
    /// 4. 在目标数据库中创建或升级表
    async fn create_plugin_tables(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        db_id: &str,
        install_path: &std::path::Path,
    ) -> PluginResult<()> {
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

        Ok(())
    }
}

impl Default for InstallService {
    /// 创建默认配置的安装服务
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
