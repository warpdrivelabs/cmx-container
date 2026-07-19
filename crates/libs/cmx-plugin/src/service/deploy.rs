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

use crate::common::{DefinitionUtils, PackageUtils, PackageUtilsDeps};
use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::storage::file::FileStorage;
use crate::security::validator::SecurityValidator;
use cmx_database::get_default_db_manager;
use serde::{Deserialize, Serialize};

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
    pub build_type: Option<String>,
    /// 是否发布到插件市场
    #[serde(default)]
    pub publish_to_marketplace: bool,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
    /// 插件市场版本ID（由 handler 发布后传入）
    pub marketplace_source_id: Option<String>,
    /// 插件市场发布信息（由 handler 发布后传入）
    pub marketplace_publish_info: Option<super::marketplace_publisher::MarketplacePublishInfo>,
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
    /// 市场发布信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace_publish: Option<super::marketplace_publisher::MarketplacePublishInfo>,
}

/// 部署服务依赖
#[derive(Clone)]
pub struct DeployServiceDeps {
    /// 插件操作编排器
    pub executor: Arc<crate::service::executor::PluginOperationExecutor>,
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 安全验证器
    pub security_validator: Arc<SecurityValidator>,
    /// 插件安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 当前应用 ID
    pub app_id: String,
}

/// 部署服务
///
/// 负责智能判断插件操作类型（安装/升级/覆盖安装），
/// 并分发到 PluginOperationExecutor 执行实际操作。
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
        Self {
            deps,
            package_utils,
        }
    }

    /// 执行部署操作
    ///
    /// 完整流程：
    /// 1. 获取并解压插件包到临时目录
    /// 2. 安全验证 + 元数据解析（获取 plugin_id 和 version）
    /// 3. 查询当前插件安装状态和版本
    /// 4. 版本比较，分发到对应操作：
    ///    - 未安装 → 调用 executor.execute_install
    ///    - 新版本 > 旧版本 → 调用 executor.execute_upgrade
    ///    - 新版本 = 旧版本 && force_reinstall → 调用 executor.execute_reinstall
    ///    - 新版本 = 旧版本 && !force_reinstall → 返回 AlreadyInstalled
    ///    - 新版本 < 旧版本 → 返回错误
    pub async fn deploy(&self, mut request: DeployRequest) -> PluginResult<DeployResponse> {
        if request.db_id.is_none() {
            request.db_id = Some(get_default_db_manager().get_biz_db_id().await);
        }

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
        let (extract_path, needs_cleanup) =
            self.package_utils
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

        // 步骤4.5: 若 source 是 Local，上传到 cmx-storage 后转为 Storage(集群同步必需)
        // 所有调用 deploy 的入口(API deploy/模块安装/市场安装)统一在此上传 OSS，
        // 确保其他节点能通过 zip_source_url 从 OSS 拉取插件包。
        if let crate::domain::plugin::PluginSource::Local { ref path } = request.source {
            let zip_path = if package_path.is_file() {
                package_path.clone()
            } else {
                path.clone()
            };
            match Self::upload_to_storage(&zip_path, &plugin_id, &new_version).await {
                Ok((file_id, _file_url)) => {
                    tracing::info!(
                        plugin_id = %plugin_id,
                        file_id = %file_id,
                        "插件包已上传到 cmx-storage(deploy 内部统一上传)"
                    );
                    request.source = crate::domain::plugin::PluginSource::Storage {
                        file_id,
                        checksum: None,
                    };
                }
                Err(e) => {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "上传 cmx-storage 失败，保留 Local source(单节点环境可正常工作)"
                    );
                }
            }
        }

        let existing_plugin = self
            .deps
            .repository
            .find_plugin(
                &plugin_id,
                request.app_id.as_deref().unwrap_or(&self.deps.app_id),
            )
            .await?;

        match existing_plugin {
            None => {
                let mut resp = self
                    .execute_install(
                        &request,
                        &plugin_id,
                        &new_version,
                        request.marketplace_source_id.as_deref(),
                    )
                    .await?;
                resp.marketplace_publish = request.marketplace_publish_info.clone();
                Ok(resp)
            }
            Some(record) => {
                let old_version = record.version.clone();
                match new_version.cmp(&old_version) {
                    std::cmp::Ordering::Greater => {
                        let mut resp = self
                            .execute_upgrade(
                                &request,
                                &plugin_id,
                                &old_version,
                                &new_version,
                                request.marketplace_source_id.as_deref(),
                            )
                            .await?;
                        resp.marketplace_publish = request.marketplace_publish_info.clone();
                        Ok(resp)
                    }
                    std::cmp::Ordering::Equal => {
                        if request.force_reinstall {
                            let mut resp = self
                                .execute_reinstall(
                                    &request,
                                    &plugin_id,
                                    &old_version,
                                    &new_version,
                                    request.marketplace_source_id.as_deref(),
                                )
                                .await?;
                            resp.marketplace_publish = request.marketplace_publish_info.clone();
                            Ok(resp)
                        } else {
                            Ok(DeployResponse {
                                plugin_id,
                                action: DeployAction::AlreadyInstalled,
                                old_version: Some(old_version),
                                new_version,
                                install_path: PathBuf::from(&record.install_path),
                                success: true,
                                message: "插件已安装相同版本，无需操作".to_string(),
                                marketplace_publish: request.marketplace_publish_info.clone(),
                            })
                        }
                    }
                    std::cmp::Ordering::Less => Err(PluginError::Deploy(format!(
                        "待安装版本 {} 低于已安装版本 {}，请使用降级接口",
                        new_version, old_version
                    ))),
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
        marketplace_source_id: Option<&str>,
    ) -> PluginResult<DeployResponse> {
        let install_req = super::install::InstallRequest {
            source: request.source.clone(),
            db_id: request.db_id.clone(),
            version_constraint: None,
            build_type: request.build_type.clone(),
            marketplace_source_id: marketplace_source_id.map(|s| s.to_string()),
            app_id: request.app_id.clone(),
        };

        let result = self.deps.executor.execute_install(install_req).await?;

        Ok(DeployResponse {
            plugin_id: result.plugin_id,
            action: DeployAction::Install,
            old_version: None,
            new_version: result.version,
            install_path: result.install_path,
            success: true,
            message: result.message,
            marketplace_publish: None,
        })
    }

    /// 执行升级操作
    async fn execute_upgrade(
        &self,
        request: &DeployRequest,
        plugin_id: &str,
        _old_version: &str,
        _new_version: &str,
        marketplace_source_id: Option<&str>,
    ) -> PluginResult<DeployResponse> {
        let upgrade_req = super::upgrade::UpgradeRequest {
            plugin_id: plugin_id.to_string(),
            source: request.source.clone(),
            version_constraint: None,
            force: false,
            operator: Some("system".to_string()),
            build_type: request.build_type.clone(),
            marketplace_source_id: marketplace_source_id.map(|s| s.to_string()),
            app_id: request.app_id.clone(),
        };

        let result = self.deps.executor.execute_upgrade(upgrade_req).await?;

        Ok(DeployResponse {
            plugin_id: result.plugin_id,
            action: DeployAction::Upgrade,
            old_version: Some(result.old_version),
            new_version: result.new_version,
            install_path: PathBuf::new(),
            success: true,
            message: "插件升级成功".to_string(),
            marketplace_publish: None,
        })
    }

    /// 执行覆盖安装操作（先卸载再安装）。
    ///
    /// 由 executor 统一编排卸载+安装+事件发布的完整流程。
    async fn execute_reinstall(
        &self,
        request: &DeployRequest,
        plugin_id: &str,
        old_version: &str,
        _new_version: &str,
        _marketplace_source_id: Option<&str>,
    ) -> PluginResult<DeployResponse> {
        self.deps
            .executor
            .execute_reinstall(request.clone(), plugin_id, old_version)
            .await
    }

    /// 上传插件 zip 到 cmx-storage，返回 (file_id, file_url)。
    ///
    /// 复用 GlobalStorageService 全局单例，无需通过 DeployServiceDeps 注入。
    /// 失败时返回错误(由调用方决定是否降级)。
    async fn upload_to_storage(
        zip_path: &std::path::Path,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<(String, String)> {
        let zip_bytes = tokio::fs::read(zip_path)
            .await
            .map_err(|e| PluginError::Plugin(format!("读取插件 zip 失败: {e}")))?;
        let file_name = zip_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("plugin-{plugin_id}-{version}.zip"));
        let storage_service = cmx_storage::global::GlobalStorageService::get().service();
        let upload_request = cmx_storage::types::UploadRequest {
            data: zip_bytes.into(),
            original_filename: Some(file_name),
            content_type: Some("application/zip".to_string()),
            object_type: Some("deployed_plugin".to_string()),
            object_id: Some(plugin_id.to_string()),
            platform: None,
            user_metadata: None,
            acl: None,
        };
        let file_info = storage_service
            .upload(upload_request)
            .await
            .map_err(|e| PluginError::Plugin(format!("上传插件包到存储失败: {e}")))?;
        Ok((file_info.id, file_info.url))
    }
}
