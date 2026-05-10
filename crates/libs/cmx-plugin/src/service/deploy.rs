//! 部署服务模块
//!
//! 处理插件智能部署流程，自动判断安装或升级操作。
//!
//! # 功能概述
//!
//! - 获取并解析插件包元数据
//! - 查询当前插件安装状态
//! - 根据版本比较结果自动分发到安装、升级或覆盖安装流程

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::common::{DefinitionUtils, PackageUtils, PackageUtilsDeps};
use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::storage::file::FileStorage;
use crate::security::validator::SecurityValidator;

use super::install::InstallService;
use super::uninstall::UninstallService;
use super::upgrade::UpgradeService;

/// 部署操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeployAction {
    /// 全新安装
    Install,
    /// 升级安装
    Upgrade,
    /// 覆盖安装（卸载后重新安装）
    Reinstall,
    /// 已安装相同版本，无需操作
    AlreadyInstalled,
}

/// 部署请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    /// 插件来源（Local 模式，指向上传的 zip 文件路径）
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    /// 是否覆盖安装（当已安装版本与待安装版本相同时，先卸载再重新安装）
    #[serde(default)]
    pub force_reinstall: bool,
    /// 构建类型 debug release
    pub  build_type : Option<String>,
}

/// 部署响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 操作类型
    pub action: DeployAction,
    /// 旧版本（仅 upgrade/reinstall 时有值）
    pub old_version: Option<String>,
    /// 新版本
    pub new_version: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 部署服务依赖
#[derive(Clone)]
pub struct DeployServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 安全验证器
    pub security_validator: Arc<SecurityValidator>,
    /// 安装服务
    pub install_service: InstallService,
    /// 升级服务
    pub upgrade_service: UpgradeService,
    /// 卸载服务
    pub uninstall_service: UninstallService,
    /// 插件安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
}

/// 部署服务
///
/// 负责智能判断插件操作类型（安装/升级/覆盖安装），
/// 并分发到对应的 Service 执行实际操作。
#[derive(Clone)]
pub struct DeployService {
    deps: DeployServiceDeps,
    package_utils: PackageUtils,
}

impl DeployService {
    /// 创建新的部署服务
    pub fn new(deps: DeployServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: Some(deps.storage.clone()),
        });
        Self { deps, package_utils }
    }

    /// 执行部署操作
    ///
    /// 完整流程：
    /// 1. 获取并解压插件包到临时目录
    /// 2. 安全验证 + 元数据解析（获取 plugin_id 和 version）
    /// 3. 查询当前插件安装状态和版本
    /// 4. 版本比较，分发到对应操作：
    ///    - 未安装 → 调用 InstallService
    ///    - 新版本 > 旧版本 → 调用 UpgradeService
    ///    - 新版本 = 旧版本 && force_reinstall → 先 UninstallService 再 InstallService
    ///    - 新版本 = 旧版本 && !force_reinstall → 返回 AlreadyInstalled
    ///    - 新版本 < 旧版本 → 返回错误
    pub async fn deploy(&self, mut request: DeployRequest) -> PluginResult<DeployResponse> {
        // 步骤1: 获取插件包
        let package_path = self
            .package_utils
            .fetch_package(&request.source, None, "部署")
            .await?;

        // 步骤2: 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_deploy_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "部署")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤3: 安全验证
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 步骤4: 解析元数据
        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let plugin_id = plugin_def.id.clone();
        let new_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());
        //如果request中的数据库id为none,使用plugin_def中的datasource_id
        if request.db_id.is_none() {
            request.db_id = plugin_def.datasource_id.clone();
        }

        // 步骤5: 查询当前安装状态
        let existing_plugin = self.deps.repository.find_plugin(&plugin_id).await?;

        match existing_plugin {
            None => {
                // 未安装 → 执行安装
                self.execute_install(&request, &plugin_id, &new_version).await
            }
            Some(record) => {
                let old_version = record.version.clone();
                match new_version.cmp(&old_version) {
                    std::cmp::Ordering::Greater => {
                        // 新版本 > 旧版本 → 执行升级
                        self.execute_upgrade(&request, &plugin_id, &old_version, &new_version)
                            .await
                    }
                    std::cmp::Ordering::Equal => {
                        if request.force_reinstall {
                            // 版本相同 + 覆盖安装 → 先卸载再安装
                            self.execute_reinstall(&request, &plugin_id, &old_version, &new_version)
                                .await
                        } else {
                            // 版本相同 + 不覆盖 → 返回已安装
                            Ok(DeployResponse {
                                plugin_id,
                                action: DeployAction::AlreadyInstalled,
                                old_version: Some(old_version),
                                new_version,
                                install_path: PathBuf::from(&record.install_path),
                                success: true,
                                message: "插件已安装相同版本，无需操作".to_string(),
                            })
                        }
                    }
                    std::cmp::Ordering::Less => {
                        // 新版本 < 旧版本 → 返回错误
                        Err(PluginError::Deploy(format!(
                            "待安装版本 {} 低于已安装版本 {}，请使用降级接口",
                            new_version, old_version
                        )))
                    }
                }
            }
        }
    }

    /// 执行安装操作
    async fn execute_install(
        &self,
        request: &DeployRequest,
        _plugin_id: &str,
        _new_version: &str,
    ) -> PluginResult<DeployResponse> {
        let install_req = super::install::InstallRequest {
            source: request.source.clone(),
            db_id: request.db_id.clone(),
            auto_activate: false,
            version_constraint: None,
            build_type: request.build_type.clone(),
        };

        let result = self.deps.install_service.install(install_req).await?;

        Ok(DeployResponse {
            plugin_id: result.plugin_id,
            action: DeployAction::Install,
            old_version: None,
            new_version: result.version,
            install_path: result.install_path,
            success: true,
            message: result.message,
        })
    }

    /// 执行升级操作
    async fn execute_upgrade(
        &self,
        request: &DeployRequest,
        plugin_id: &str,
        _old_version: &str,
        _new_version: &str,
    ) -> PluginResult<DeployResponse> {
        let upgrade_req = super::upgrade::UpgradeRequest {
            plugin_id: plugin_id.to_string(),
            source: request.source.clone(),
            version_constraint: None,
            force: false,
            operator: Some("system".to_string()),
            build_type: request.build_type.clone(),
        };

        let result = self.deps.upgrade_service.upgrade(upgrade_req).await?;

        Ok(DeployResponse {
            plugin_id: result.plugin_id,
            action: DeployAction::Upgrade,
            old_version: Some(result.old_version),
            new_version: result.new_version,
            install_path: PathBuf::new(),
            success: true,
            message: "插件升级成功".to_string(),
        })
    }

    /// 执行覆盖安装操作（先卸载再安装）
    async fn execute_reinstall(
        &self,
        request: &DeployRequest,
        plugin_id: &str,
        old_version: &str,
        _new_version: &str,
    ) -> PluginResult<DeployResponse> {
        // 先卸载
        let uninstall_req = super::uninstall::UninstallRequest {
            plugin_id: plugin_id.to_string(),
            force: true,
            operator: "system".to_string(),
        };

        self.deps.uninstall_service.uninstall(uninstall_req).await?;

        // 再安装
        let install_req = super::install::InstallRequest {
            source: request.source.clone(),
            db_id: request.db_id.clone(),
            auto_activate: false,
            version_constraint: None,
            build_type: request.build_type.clone(),
        };

        let result = self.deps.install_service.install(install_req).await?;

        Ok(DeployResponse {
            plugin_id: result.plugin_id,
            action: DeployAction::Reinstall,
            old_version: Some(old_version.to_string()),
            new_version: result.version,
            install_path: result.install_path,
            success: true,
            message: "插件覆盖安装成功".to_string(),
        })
    }
}
