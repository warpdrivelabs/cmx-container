//! 插件持久化操作层
//!
//! 只负责数据库操作（DML + DDL）和源文件处理（安装包解压 + 元数据提取 + 复制到安装目录），
//! 不涉及内存注册、缓存更新、事件发布。
//!
//! # 职责边界
//!
//! - ✅ 获取插件包 + 解压 + 安全验证 + 解析元数据
//! - ✅ 检查已安装状态
//! - ✅ 检查依赖
//! - ✅ 创建安装目录 + 复制文件
//! - ✅ DDL（使用 `execute_ddl_with_lock`，分布式锁保护）
//! - ✅ 事务：`upsert_plugin` + `upsert_version` + `set_current_version`
//! - ✅ 解析并存储服务定义
//! - ✅ 提交事务
//! - ✅ 物理删除（卸载时）
//! - ❌ 内存注册（Registry / Contexts 操作）
//! - ❌ 缓存更新
//! - ❌ 事件发布（GlobalEventBus + Redis Notifier）
//! - ❌ 审计日志（由 Executor 层处理）

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cmx_database::get_default_db_manager;

use crate::common::{
    DependencyUtils, DependencyUtilsDeps, DefinitionUtils, PackageUtils, PackageUtilsDeps,
    extract_source_info,
};
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::storage::TempDirCleanup;
use crate::service::data_parser::ServiceParseParams;
use crate::service::install::InstallRequest;
use crate::service::upgrade::UpgradeRequest;
use crate::service::downgrade::DowngradeRequest;
use crate::service::uninstall::UninstallRequest;
use crate::service::deploy::DeployRequest;

/// 持久化操作的统一结果。
///
/// 承载不同操作（安装/升级/降级/卸载/覆盖安装）的差异化信息，
/// 供 Executor、EventPublisher 和 RuntimeOps 使用。
///
/// 该结构体从 `event_publisher.rs` 迁移至此，因为持久化层不应依赖事件发布层。
#[derive(Debug, Clone)]
pub struct PersistResult {
    /// 插件 ID
    pub plugin_id: String,
    /// 应用 ID
    pub app_id: String,
    /// 操作后的版本
    pub version: String,
    /// 操作前的版本（升级/降级/覆盖安装时有值）
    pub old_version: Option<String>,
    /// 安装路径
    pub install_path: PathBuf,
    /// WASM 文件路径
    pub wasm_path: String,
    /// 插件名称
    pub plugin_name: Option<String>,
    /// 插件描述
    pub description: Option<String>,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 来源路径
    pub source_path: Option<String>,
}

/// 插件持久化服务
///
/// 封装所有插件生命周期操作的数据库和文件系统部分，
/// 返回 `PersistResult` 供上层 Executor 完成内存注册、缓存更新和事件发布。
#[derive(Clone)]
pub struct PluginPersistence {
    deps: InstallServiceDeps,
    package_utils: PackageUtils,
    dependency_utils: DependencyUtils,
}

/// 安装服务依赖（复用为持久化层依赖）
///
/// 因为安装操作需要的依赖最多（包含文件存储、安全验证器、包工具等），
/// 其他操作共享这些依赖。
type InstallServiceDeps = crate::service::install::InstallServiceDeps;

impl PluginPersistence {
    /// 创建新的持久化服务
    pub fn new(deps: InstallServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: Some(deps.storage.clone()),
        });
        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: deps.repository.clone(),
            registry: deps.registry.clone(),
        });
        Self {
            deps,
            package_utils,
            dependency_utils,
        }
    }

    /// 安装持久化
    ///
    /// 从安装请求中提取持久化逻辑：获取插件包 → 解压验证 → 检查状态和依赖 →
    /// 创建安装目录 → DDL → 事务写入数据库 → 解析服务定义 → 提交事务。
    pub async fn install_persist(&self, request: InstallRequest) -> PluginResult<PersistResult> {
        let build_type = request.build_type.unwrap_or("release".to_string());
        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());

        // 1. 获取插件包
        let package_path = self
            .package_utils
            .fetch_package(
                &request.source,
                request.version_constraint.as_deref(),
                "安装",
            )
            .await?;

        // 2. 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) =
            self.package_utils
                .prepare_package_for_validation(&package_path, &temp_dir, "安装")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 3. 安全验证
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 4. 解析元数据
        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let plugin_id = plugin_def.id.clone();
        let install_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 5. 检查已安装状态（数据库层面）
        if let Some(existing) = self.deps.repository.find_plugin(&plugin_id, &app_id).await? {
            if existing.version == install_version {
                return Ok(PersistResult {
                    plugin_id,
                    app_id,
                    version: install_version,
                    old_version: None,
                    install_path: PathBuf::from(&existing.install_path),
                    wasm_path: existing.wasm_path,
                    plugin_name: Some(existing.name),
                    description: existing.description,
                    domain_code: existing.domain_code.unwrap_or_default(),
                    application_code: existing.application_code.unwrap_or_default(),
                    module_code: existing.module_code.unwrap_or_default(),
                    plugin_type: existing.plugin_type,
                    source_path: existing.source_path,
                });
            } else if existing.version > install_version {
                return Err(PluginError::Install(format!(
                    "插件 {} 已安装版本 {}，要降级到 {} 请使用降级功能",
                    plugin_id, existing.version, install_version
                )));
            }
        }

        // 6. 检查依赖
        let dep_result = self
            .dependency_utils
            .check_plugin_dependencies(&plugin_def)
            .await?;
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

        // 7. 创建安装目录
        let install_path = self.deps.plugin_root.join(&app_id).join(&plugin_id).join(&install_version);
        if install_path.exists() {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 8. 复制文件到安装目录
        self.package_utils
            .copy_plugin_files(&extract_path, &install_path, "安装")?;

        // 9. 确定目标数据库ID
        let target_db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.deps.default_database_id.clone());

        let default_db_id = self.deps.default_database_id.clone();

        // 10. 开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 11. 种子数据初始化(建表 DDL 已迁移到模块安装流程,但 seeddata 保留在插件包内)
        // TODO(module): 建表 execute_ddl_with_lock 已迁移到模块安装流程(ModuleInstallService),
        // 单独安装插件时不再自动建表。但种子数据初始化仍随插件执行(seeddata 在插件包内)。
        // crate::service::utils::execute_ddl_with_lock(
        //     &self.deps.lock_manager,
        //     &target_db_id,
        //     &plugin_id,
        //     &app_id,
        //     &install_version,
        //     &install_path,
        //     &plugin_def,
        //     Some(txn_guard.txn_id()),
        // )
        // .await?;
        if let Err(e) = crate::service::utils::execute_seed_data(
            &target_db_id,
            &plugin_id,
            &install_path,
            &plugin_def,
        )
        .await
        {
            tracing::warn!("插件 {} 种子数据初始化失败(不阻断安装): {}", plugin_id, e);
        }

        // 12. 构建数据库记录
        let (zip_source_type, zip_source_url) = extract_source_info(&request.source);
        let source_info = super::record_builder::PluginSourceInfo::new(
            zip_source_url.as_deref(),
            zip_source_type.as_deref(),
            request.marketplace_source_id.as_deref(),
        );
        let db_record = super::record_builder::build_plugin_create_params(
            &plugin_def,
            &install_version,
            &install_path,
            &target_db_id,
            &source_info,
            &app_id,
        );

        // 13. 检查基线版本
        let baseline_version = self
            .deps
            .repository
            .get_baseline_version(&plugin_id)
            .await?;
        if let Some(ref db_version) = baseline_version
            && install_version < *db_version
        {
            return Err(PluginError::Install(format!(
                "插件 {} 已安装版本 {}，要降级到 {} 请使用降级功能",
                plugin_id, db_version, install_version
            )));
        }

        // 14. Upsert 插件记录
        self.deps
            .repository
            .upsert_plugin(&db_record, Some(txn_guard.txn_id()))
            .await?;

        // 15. 插入版本历史记录
        let wasm_path = super::record_builder::build_wasm_path(&install_path, &plugin_def);
        let version_record = super::record_builder::build_version_create_params(
            &plugin_id,
            &app_id,
            &install_version,
            &install_path.to_string_lossy(),
            &wasm_path,
            &source_info,
            Some(&plugin_def),
            build_type.as_str(),
        );
        self.deps
            .version_history_repository
            .upsert_version(&version_record, Some(txn_guard.txn_id()))
            .await?;

        // 16. 标记当前版本
        self.deps
            .version_history_repository
            .set_current_version(
                &plugin_id,
                &app_id,
                &install_version,
                install_path.to_string_lossy().to_string().as_str(),
                wasm_path.as_str(),
                Some(txn_guard.txn_id()),
            )
            .await?;

        // 17. 解析并存储服务定义
        let parse_params = ServiceParseParams {
            plugin_id: plugin_id.clone(),
            plugin_version: install_version.clone(),
            app_id: app_id.clone(),
            domain_code: plugin_def.domain_code.clone().unwrap_or_default(),
            application_code: plugin_def.application_code.clone().unwrap_or_default(),
            module_code: plugin_def.module_code.clone().unwrap_or_default(),
        };
        let parsed_services = crate::service::service_parser::parse_and_save_services(
            &install_path,
            &parse_params,
            &self.deps.service_storage,
            &self.deps.plugin_root,
            &self.deps.plugin_query,
            Some(txn_guard.txn_id()),
        )
        .await?;

        if !parsed_services.is_empty() {
            tracing::info!(
                "插件 {} 安装时解析到 {} 个服务定义",
                plugin_id,
                parsed_services.len()
            );
        }

        // 18. 提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        Ok(PersistResult {
            plugin_id,
            app_id,
            version: install_version,
            old_version: None,
            install_path,
            wasm_path,
            plugin_name: Some(plugin_def.name),
            description: plugin_def.description,
            domain_code: plugin_def.domain_code.unwrap_or_default(),
            application_code: plugin_def.application_code.clone().unwrap_or_default(),
            module_code: plugin_def.module_code.clone().unwrap_or_default(),
            plugin_type: Some(plugin_def.r#type),
            source_path: plugin_def.source_path,
        })
    }

    /// 升级持久化
    ///
    /// 从升级请求中提取持久化逻辑：检查插件存在 → 获取新版本包 → 解压验证 →
    /// 版本检查 → 检查依赖 → 创建安装目录 → DDL → 事务写入数据库 →
    /// 解析服务定义 → 提交事务。
    pub async fn upgrade_persist(&self, request: UpgradeRequest) -> PluginResult<PersistResult> {
        let build_type = request.build_type.unwrap_or("release".to_string());
        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());

        // 1. 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id, &app_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        let effective_marketplace_source_id = request
            .marketplace_source_id
            .clone()
            .or_else(|| plugin.marketplace_source_id.clone());

        // 2. 获取新版本插件包
        let package_path = self
            .package_utils
            .fetch_package(
                &request.source,
                request.version_constraint.as_deref(),
                "升级",
            )
            .await?;

        // 3. 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) =
            self.package_utils
                .prepare_package_for_validation(&package_path, &temp_dir, "升级")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 4. 安全验证
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 5. 解析元数据
        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let new_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 6. 版本检查
        if !request.force && new_version <= old_version {
            return Err(PluginError::Upgrade(format!(
                "升级版本必须大于当前版本: 当前 {}, 新版本 {}",
                old_version, new_version
            )));
        }

        let plugin_id = request.plugin_id.clone();

        // 7. 检查依赖
        let dep_result = self
            .dependency_utils
            .check_plugin_dependencies(&plugin_def)
            .await?;
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

        // 8. 创建新版本目录
        let install_path = self.deps.plugin_root.join(&app_id).join(&plugin_id).join(&new_version);
        if install_path.exists() {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 9. 复制文件到新版本目录
        self.package_utils
            .copy_plugin_files(&extract_path, &install_path, "升级")?;

        let target_db_id = plugin.db_id.clone();
        let default_db_id = self.deps.default_database_id.clone();

        // 10. 开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 11. 种子数据初始化(建表 DDL 已迁移到模块安装流程,但 seeddata 保留在插件包内)
        // TODO(module): 建表 execute_ddl_with_lock 已迁移到模块安装流程,升级插件时不再自动建表。
        // 但种子数据初始化仍随插件执行(seeddata 在插件包内)。
        // crate::service::utils::execute_ddl_with_lock(
        //     &self.deps.lock_manager,
        //     &target_db_id,
        //     &plugin_id,
        //     &app_id,
        //     &new_version,
        //     &install_path,
        //     &plugin_def,
        //     Some(txn_guard.txn_id()),
        // )
        // .await?;
        if let Err(e) = crate::service::utils::execute_seed_data(
            &target_db_id,
            &plugin_id,
            &install_path,
            &plugin_def,
        )
        .await
        {
            tracing::warn!("插件 {} 种子数据初始化失败(不阻断升级): {}", plugin_id, e);
        }

        // 12. 构建数据库记录
        let (zip_source_type, zip_source_url) = extract_source_info(&request.source);
        let source_info = super::record_builder::PluginSourceInfo::new(
            zip_source_url.as_deref(),
            zip_source_type.as_deref(),
            effective_marketplace_source_id.as_deref(),
        );
        let db_record = super::record_builder::build_plugin_create_params(
            &plugin_def,
            &new_version,
            &install_path,
            &target_db_id,
            &source_info,
            &app_id,
        );

        // 13. Upsert 插件记录
        self.deps
            .repository
            .upsert_plugin(&db_record, Some(txn_guard.txn_id()))
            .await?;

        // 14. 插入版本历史记录
        let wasm_path = super::record_builder::build_wasm_path(&install_path, &plugin_def);
        let version_record = super::record_builder::build_version_create_params(
            &plugin_id,
            &app_id,
            &new_version,
            &install_path.to_string_lossy(),
            &wasm_path,
            &source_info,
            Some(&plugin_def),
            build_type.as_str(),
        );
        self.deps
            .version_history_repository
            .upsert_version(&version_record, Some(txn_guard.txn_id()))
            .await?;

        // 15. 标记当前版本
        self.deps
            .version_history_repository
            .set_current_version(
                &plugin_id,
                &app_id,
                new_version.as_str(),
                install_path.to_string_lossy().to_string().as_str(),
                wasm_path.as_str(),
                Some(txn_guard.txn_id()),
            )
            .await?;

        // 16. 解析并存储服务定义
        let parse_params = ServiceParseParams {
            plugin_id: plugin_id.clone(),
            plugin_version: new_version.clone(),
            app_id: app_id.clone(),
            domain_code: plugin_def.domain_code.clone().unwrap_or_default(),
            application_code: plugin_def.application_code.clone().unwrap_or_default(),
            module_code: plugin_def.module_code.clone().unwrap_or_default(),
        };
        let parsed_services = crate::service::service_parser::parse_and_save_services(
            &install_path,
            &parse_params,
            &self.deps.service_storage,
            &self.deps.plugin_root,
            &self.deps.plugin_query,
            Some(txn_guard.txn_id()),
        )
        .await?;

        if !parsed_services.is_empty() {
            tracing::info!(
                "插件 {} 升级时解析到 {} 个服务定义",
                plugin_id,
                parsed_services.len()
            );
        }

        // 17. 提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        Ok(PersistResult {
            plugin_id,
            app_id,
            version: new_version,
            old_version: Some(old_version),
            install_path,
            wasm_path,
            plugin_name: Some(plugin_def.name),
            description: plugin_def.description,
            domain_code: plugin_def.domain_code.unwrap_or_default(),
            application_code: plugin_def.application_code.clone().unwrap_or_default(),
            module_code: plugin_def.module_code.clone().unwrap_or_default(),
            plugin_type: Some(plugin_def.r#type),
            source_path: plugin_def.source_path,
        })
    }

    /// 降级持久化
    ///
    /// 从降级请求中提取持久化逻辑：检查插件存在 → 查找目标版本 →
    /// 事务更新版本和主表 → 处理服务定义 → 提交事务。
    ///
    /// 降级只是切换版本目录，不涉及文件拷贝。
    pub async fn downgrade_persist(&self, request: DowngradeRequest) -> PluginResult<PersistResult> {
        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());

        // 1. 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id, &app_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        // 2. 查找目标版本信息
        let target_version_record = self
            .deps
            .version_history_repository
            .find_version(&request.plugin_id, &app_id, &request.target_version, None)
            .await?
            .ok_or_else(|| {
                PluginError::Downgrade(format!("未找到版本 {} 的记录", request.target_version))
            })?;

        let plugin_id = request.plugin_id.clone();
        let default_db_id = self.deps.default_database_id.clone();

        // 3. 开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 4. 更新 cmx_plugin_versions（原子操作）
        self.deps
            .version_history_repository
            .set_current_version(
                &plugin_id,
                &app_id,
                &request.target_version,
                &target_version_record.install_path,
                &target_version_record.wasm_path,
                Some(txn_guard.txn_id()),
            )
            .await?;

        // 5. 更新 cmx_plugin 主表
        let fields = crate::infrastructure::database::repository::PluginUpdateParams {
            version: Some(request.target_version.clone()),
            wasm_path: Some(target_version_record.wasm_path.clone()),
            install_path: Some(target_version_record.install_path.clone()),
            marketplace_source_id: target_version_record.marketplace_source_id.clone(),
            ..Default::default()
        };
        self.deps
            .repository
            .update_plugin(&plugin_id, &app_id, &fields, Some(txn_guard.txn_id()))
            .await?;

        // 6. 更新 cmx_meta_table_define version 字段
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

        // 7. 处理降级时的服务定义
        {
            let install_path = PathBuf::from(&target_version_record.install_path);
            let parse_params = ServiceParseParams {
                plugin_id: plugin_id.clone(),
                plugin_version: request.target_version.clone(),
                app_id: app_id.clone(),
                domain_code: plugin.domain_code.clone().unwrap_or_default(),
                application_code: plugin.application_code.clone().unwrap_or_default(),
                module_code: plugin.module_code.clone().unwrap_or_default(),
            };
            let old_version_services = crate::service::service_parser::parse_services_from_plugin_dir(
                &install_path,
                &parse_params,
            )?;
            let old_service_keys: HashSet<String> = old_version_services
                .iter()
                .map(|s| s.service_key.clone())
                .collect();

            // 7a. 查询数据库中该插件的所有服务
            let db_services = self.deps.service_query
                .get_services_by_plugin(&plugin_id)
                .await
                .map_err(|e| PluginError::Database(format!("查询服务定义失败: {}", e)))?;

            // 7b. 删除在新版本中存在但旧版本中不存在的服务，更新保留服务的版本号
            let mut deleted_count = 0;
            let mut updated_count = 0;
            for service in db_services {
                if !old_service_keys.contains(&service.service_key) {
                    self.deps
                        .service_storage
                        .delete_service(&service.service_key, &app_id, Some(txn_guard.txn_id()), None)
                        .await
                        .map_err(|e| {
                            PluginError::Database(format!(
                                "删除服务定义 {} 失败: {}",
                                service.service_key, e
                            ))
                        })?;
                    deleted_count += 1;
                } else {
                    let mut updated_service = service;
                    updated_service.version = request.target_version.clone();
                    self.deps
                        .service_storage
                        .save_service(&updated_service, Some(txn_guard.txn_id()))
                        .await
                        .map_err(|e| PluginError::Database(format!("更新服务定义版本失败: {}", e)))?;
                    updated_count += 1;
                }
            }
            tracing::info!(
                "插件 {} 降级时服务处理完成: 删除 {} 个服务，更新 {} 个服务",
                plugin_id,
                deleted_count,
                updated_count
            );
        }

        // 8. 提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        Ok(PersistResult {
            plugin_id,
            app_id,
            version: request.target_version.clone(),
            old_version: Some(old_version),
            install_path: PathBuf::from(&target_version_record.install_path),
            wasm_path: target_version_record.wasm_path,
            plugin_name: Some(plugin.name),
            description: plugin.description,
            domain_code: plugin.domain_code.unwrap_or_default(),
            application_code: plugin.application_code.unwrap_or_default(),
            module_code: plugin.module_code.unwrap_or_default(),
            plugin_type: target_version_record.plugin_type,
            source_path: target_version_record.source_path,
        })
    }

    /// 卸载持久化
    ///
    /// 从卸载请求中提取持久化逻辑：检查插件存在 → 开启事务 →
    /// 删除版本历史 → 删除主表记录 → 删除表元数据 → 清理服务定义 →
    /// 提交事务 → 物理删除安装目录。
    ///
    /// # 事务保护设计
    ///
    /// 所有数据库操作（删除版本历史、主表记录、表元数据、服务定义）包裹在同一个事务中，
    /// 保证原子性。物理删除安装目录在事务提交**之后**执行，避免事务回滚后文件已删除
    /// 导致数据不一致。
    pub async fn uninstall_persist(&self, request: UninstallRequest) -> PluginResult<PersistResult> {
        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());

        // 1. 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id, &app_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let version = plugin.version.clone();
        let plugin_id = request.plugin_id.clone();
        let install_path = plugin.install_path.clone();
        let wasm_path = plugin.wasm_path.clone();
        let default_db_id = self.deps.default_database_id.clone();

        // 2. 开启事务，保证数据库操作的原子性
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 3. 删除 cmx_plugin_versions 版本历史记录
        self.deps
            .version_history_repository
            .delete_versions_by_plugin_id(&plugin_id, &app_id, Some(txn_guard.txn_id()))
            .await?;

        // 4. 删除 cmx_plugin 主表记录
        self.deps
            .repository
            .delete_plugin(&plugin_id, &app_id)
            .await?;

        // 5. 删除 cmx_meta_table_define 和 cmx_meta_table_define_version
        {
            let dbm = get_default_db_manager();
            crate::infrastructure::database::table_metadata::TableMetadataService::delete_by_plugin_id(
                dbm,
                default_db_id.as_str(),
                Some(txn_guard.txn_id()),
                &plugin_id,
                &app_id,
            )
            .await
            .map_err(|e| PluginError::Database(format!("删除表元数据失败: {}", e)))?;
        }

        // 6. 清理此插件关联的服务定义
        if let Err(e) = self
            .deps
            .service_storage
            .delete_services_by_plugin(&plugin_id, &app_id, Some(txn_guard.txn_id()))
            .await
        {
            tracing::warn!("清理插件 {} 的服务定义失败: {:?}", plugin_id, e);
        } else {
            tracing::info!("已清理插件 {} 的服务定义", plugin_id);
        }

        // 7. 提交事务（所有 DB 操作原子提交）
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 8. 物理删除安装目录（事务提交后执行，使用异步文件操作）
        if let Some(parent_path) =
            Path::new(&install_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
        {
            match tokio::fs::remove_dir_all(&parent_path).await {
                Ok(()) => tracing::info!("删除插件安装目录成功: {}", parent_path),
                Err(e) => tracing::error!("删除插件安装目录失败: {} - {}", parent_path, e),
            }
        }

        Ok(PersistResult {
            plugin_id,
            app_id,
            version,
            old_version: None,
            install_path: PathBuf::from(&install_path),
            wasm_path,
            plugin_name: Some(plugin.name),
            description: plugin.description,
            domain_code: plugin.domain_code.unwrap_or_default(),
            application_code: plugin.application_code.unwrap_or_default(),
            module_code: plugin.module_code.unwrap_or_default(),
            plugin_type: plugin.plugin_type,
            source_path: plugin.source_path,
        })
    }

    /// 覆盖安装持久化
    ///
    /// ⚠️ **非原子操作**：先卸载再安装，中间状态依赖一致性校验任务补偿。
    /// 卸载和安装各自有独立事务，无法合并为一个原子事务，因此若安装失败，
    /// 插件将处于"已卸载但未安装"的中间状态，需由一致性校验任务补偿。
    ///
    /// 从部署请求中提取覆盖安装的持久化逻辑：
    /// 卸载持久化（不发事件） → 安装持久化（不发事件）。
    ///
    /// # Arguments
    ///
    /// * `request` - 部署请求
    /// * `plugin_id` - 待覆盖安装的插件 ID（由调用方从元数据解析中获取）
    /// * `old_version` - 旧版本号（用于 `PersistResult::old_version`）
    pub async fn reinstall_persist(
        &self,
        request: DeployRequest,
        plugin_id: &str,
        old_version: &str,
    ) -> PluginResult<PersistResult> {
        let app_id = request.app_id.clone().unwrap_or_else(|| "default".to_string());

        // 1. 先卸载（⚠️ 非原子操作：卸载与安装是独立事务，中间状态不可回滚）
        let uninstall_req = UninstallRequest {
            plugin_id: plugin_id.to_string(),
            force: true,
            operator: "system".to_string(),
            app_id: Some(app_id.clone()),
        };
        self.uninstall_persist(uninstall_req).await?;

        // 2. 再安装
        let install_req = InstallRequest {
            source: request.source.clone(),
            db_id: request.db_id.clone(),
            auto_activate: false,
            version_constraint: None,
            build_type: request.build_type.clone(),
            marketplace_source_id: request.marketplace_source_id.clone(),
            app_id: Some(app_id),
        };
        let result = self.install_persist(install_req).await?;

        // 3. 补充 old_version（覆盖安装场景下记录旧版本）
        Ok(PersistResult {
            old_version: Some(old_version.to_string()),
            ..result
        })
    }
}
