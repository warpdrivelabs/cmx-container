//! 插件管控服务。
//!
//! 提供集中式插件元数据初始化能力，仅执行 DDL/DML 操作，
//! 不触发本地运行时加载。完成后发布 RuntimeLoad 通知。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::cluster::notification::PluginNotifier;
use crate::common::{DefinitionUtils, PackageUtils, PackageUtilsDeps};
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::TempDirCleanup;
use cmx_traits::{GlobalEventBus, plugin_events, PluginLifecyclePayload};

use crate::service::downgrade::{DowngradeRequest, DowngradeService};
use crate::service::install::{InstallRequest, InstallService};
use crate::service::uninstall::UninstallService;
use crate::service::upgrade::{UpgradeRequest, UpgradeService};

/// 管控部署请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlDeployRequest {
    /// 插件来源
    pub source: crate::domain::plugin::PluginSource,
    /// 目标数据库ID
    pub db_id: Option<String>,
    /// 构建类型
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控安装请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlInstallRequest {
    /// 插件来源
    pub source: crate::domain::plugin::PluginSource,
    /// 目标数据库ID
    pub db_id: Option<String>,
    /// 构建类型
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控升级请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlUpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本（用于版本一致性校验）
    pub target_version: String,
    /// 插件来源
    pub source: crate::domain::plugin::PluginSource,
    /// 构建类型
    pub build_type: Option<String>,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控降级请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlDowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控卸载请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlUninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 应用ID
    pub app_id: Option<String>,
}

/// 管控部署响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlDeployResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 执行动作
    pub action: String,
    /// 应用ID
    pub app_id: String,
}

/// 管控服务依赖
#[derive(Clone)]
pub struct ControlServiceDeps {
    /// 安装服务
    pub install_service: InstallService,
    /// 升级服务
    pub upgrade_service: UpgradeService,
    /// 降级服务
    pub downgrade_service: DowngradeService,
    /// 卸载服务
    pub uninstall_service: UninstallService,
    /// 插件变更通知器
    pub notifier: Option<Arc<PluginNotifier>>,
    /// 应用ID
    pub app_id: String,
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 插件安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 文件存储
    pub storage: Arc<FileStorage>,
}

/// 插件管控服务。
///
/// 封装已有的 InstallService/UpgradeService，但跳过本地运行时加载
/// （不注册 Registry、不加载 Service/WASM），完成 DDL/DML 后上传
/// 文件至对象存储，并发布 RuntimeLoad 通知。
///
/// ## 版本一致性约束
///
/// 同一插件在所有 app_id 下必须安装相同版本（DDL 物理表共享）。
/// 安装/升级/降级前会校验版本一致性，冲突时拒绝操作。
pub struct ControlService {
    /// 依赖注入
    deps: ControlServiceDeps,
    /// 包处理工具
    package_utils: PackageUtils,
}

impl ControlService {
    /// 创建管控服务实例。
    pub fn new(deps: ControlServiceDeps) -> Self {
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

    /// 校验版本一致性。
    ///
    /// 查询同一 `plugin_id` 在所有 `app_id` 下的已安装版本，
    /// 如果存在不同版本则返回冲突错误。
    async fn check_version_consistency(
        &self,
        plugin_id: &str,
        target_version: &str,
        current_app_id: &str,
    ) -> PluginResult<()> {
        let all_apps_filter = PluginFilter {
            ..Default::default()
        };

        let all_records = self.deps.repository.list_plugins(&all_apps_filter).await?;
        let conflicting: Vec<(String, String)> = all_records
            .iter()
            .filter(|r| r.plugin_id == plugin_id && r.app_id != current_app_id)
            .filter(|r| r.version != target_version)
            .map(|r| (r.app_id.clone(), r.version.clone()))
            .collect();

        if !conflicting.is_empty() {
            let details: Vec<String> = conflicting
                .iter()
                .map(|(aid, ver)| format!("app_id={} has version {}", aid, ver))
                .collect();
            return Err(PluginError::Install(format!(
                "Version conflict for plugin {}: target version {} conflicts with existing installations: {}",
                plugin_id,
                target_version,
                details.join(", ")
            )));
        }

        Ok(())
    }

    /// 管控部署（自动判断安装/升级）。
    ///
    /// 完整流程：
    /// 1. 获取并解压插件包到临时目录
    /// 2. 解析元数据（获取 plugin_id 和 version）
    /// 3. 查询当前插件安装状态
    /// 4. 根据状态决定调用 InstallService 或 UpgradeService
    ///
    /// 所有底层服务调用均传入 `send_event=false`，
    /// 由管控服务统一发布 RuntimeLoad 通知。
    pub async fn deploy(&self, req: ControlDeployRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.deps.app_id.clone());

        // 步骤1: 获取插件包
        let package_path = self
            .package_utils
            .fetch_package(&req.source, None, "管控部署")
            .await?;

        // 步骤2: 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_control_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "管控部署")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤3: 解析元数据
        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let plugin_id = plugin_def.id.clone();
        let new_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        info!(
            "管控部署: 解析插件包 plugin_id={}, version={}",
            plugin_id, new_version
        );

        // 步骤4: 查询当前安装状态
        let existing_plugin = self
            .deps
            .repository
            .find_plugin(&plugin_id, &app_id)
            .await?;

        match existing_plugin {
            None => {
                // 未安装，执行安装
                info!(
                    "管控部署: 插件 {} 未安装，执行安装流程, version={}",
                    plugin_id, new_version
                );

                let install_req = InstallRequest {
                    source: req.source.clone(),
                    db_id: req.db_id.or(plugin_def.datasource_id.clone()),
                    auto_activate: false,
                    version_constraint: None,
                    build_type: req.build_type.clone(),
                    marketplace_source_id: None,
                    app_id: Some(app_id.clone()),
                    send_event: false,
                };

                let result = self.deps.install_service.install(install_req).await?;

                let payload = PluginLifecyclePayload::new(&app_id, &result.plugin_id, &result.version)
                    .with_install_path(PathBuf::from(&result.install_path));
                GlobalEventBus::get()
                    .publish(plugin_events::INSTALLED, serde_json::to_value(&payload).unwrap())
                    .await;

                // 发布 RuntimeLoad 通知
                if let Some(ref notifier) = self.deps.notifier {
                    notifier.notify_runtime_load(&result.plugin_id, &result.version, &app_id).await;
                }

                Ok(ControlDeployResponse {
                    plugin_id: result.plugin_id,
                    version: result.version,
                    action: "installed".to_string(),
                    app_id,
                })
            }
            Some(existing) => {
                // 已安装，判断版本关系
                let old_version = &existing.version;

                // 版本比较
                match new_version.cmp(old_version) {
                    std::cmp::Ordering::Greater => {
                        // 新版本 > 旧版本，执行升级
                        info!(
                            "管控部署: 插件 {} 已安装 version={}，目标 version={}，执行升级",
                            plugin_id, old_version, new_version
                        );

                        let upgrade_req = UpgradeRequest {
                            plugin_id: plugin_id.clone(),
                            source: req.source.clone(),
                            version_constraint: None,
                            force: false,
                            operator: None,
                            build_type: req.build_type.clone(),
                            marketplace_source_id: None,
                            app_id: Some(app_id.clone()),
                            send_event: false,
                        };

                        let result = self.deps.upgrade_service.upgrade(upgrade_req).await?;

                        let payload = PluginLifecyclePayload::new(&app_id, &result.plugin_id, &result.new_version)
                            .with_old_version(&result.old_version);
                        GlobalEventBus::get()
                            .publish(plugin_events::UPGRADED, serde_json::to_value(&payload).unwrap())
                            .await;

                        // 发布 RuntimeLoad 通知
                        if let Some(ref notifier) = self.deps.notifier {
                            notifier.notify_runtime_load(&result.plugin_id, &result.new_version, &app_id).await;
                        }

                        Ok(ControlDeployResponse {
                            plugin_id: result.plugin_id,
                            version: result.new_version,
                            action: "upgraded".to_string(),
                            app_id,
                        })
                    }
                    std::cmp::Ordering::Equal => {
                        // 版本相同，返回已安装
                        info!(
                            "管控部署: 插件 {} 已安装相同版本 {}, 无需操作",
                            plugin_id, new_version
                        );
                        Ok(ControlDeployResponse {
                            plugin_id,
                            version: new_version,
                            action: "already_installed".to_string(),
                            app_id,
                        })
                    }
                    std::cmp::Ordering::Less => {
                        // 新版本 < 旧版本，降级场景
                        Err(PluginError::Deploy(format!(
                            "插件 {} 已安装更高版本 {}，不允许降级到 {}",
                            plugin_id, old_version, new_version
                        )))
                    }
                }
            }
        }
    }

    /// 管控安装。
    ///
    /// 安装前校验版本一致性，确保同一插件在所有 app_id 下版本相同。
    /// 底层服务调用传入 `send_event=false`，由管控服务统一发布通知。
    pub async fn install(&self, req: ControlInstallRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.deps.app_id.clone());

        let install_req = InstallRequest {
            source: req.source,
            db_id: req.db_id,
            auto_activate: false,
            version_constraint: None,
            build_type: req.build_type,
            marketplace_source_id: None,
            app_id: Some(app_id.clone()),
            send_event: false,
        };

        let result = self.deps.install_service.install(install_req).await?;

        let payload = PluginLifecyclePayload::new(&app_id, &result.plugin_id, &result.version)
            .with_install_path(PathBuf::from(&result.install_path));
        GlobalEventBus::get()
            .publish(plugin_events::INSTALLED, serde_json::to_value(&payload).unwrap())
            .await;

        // 发布 RuntimeLoad 通知
        if let Some(ref notifier) = self.deps.notifier {
            notifier.notify_runtime_load(&result.plugin_id, &result.version, &app_id).await;
        }

        Ok(ControlDeployResponse {
            plugin_id: result.plugin_id,
            version: result.version,
            action: "installed".to_string(),
            app_id,
        })
    }

    /// 管控升级。
    ///
    /// 升级前校验版本一致性。
    /// 底层服务调用传入 `send_event=false`，由管控服务统一发布通知。
    pub async fn upgrade(&self, req: ControlUpgradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.deps.app_id.clone());

        // 升级前先校验版本一致性（使用目标版本）
        self.check_version_consistency(&req.plugin_id, &req.target_version, &app_id).await?;

        let upgrade_req = UpgradeRequest {
            plugin_id: req.plugin_id.clone(),
            source: req.source,
            version_constraint: None,
            force: false,
            operator: None,
            build_type: req.build_type,
            marketplace_source_id: None,
            app_id: Some(app_id.clone()),
            send_event: false,
        };

        let result = self.deps.upgrade_service.upgrade(upgrade_req).await?;

        let payload = PluginLifecyclePayload::new(&app_id, &result.plugin_id, &result.new_version)
            .with_old_version(&result.old_version);
        GlobalEventBus::get()
            .publish(plugin_events::UPGRADED, serde_json::to_value(&payload).unwrap())
            .await;

        // 发布 RuntimeLoad 通知
        if let Some(ref notifier) = self.deps.notifier {
            notifier.notify_runtime_load(&result.plugin_id, &result.new_version, &app_id).await;
        }

        Ok(ControlDeployResponse {
            plugin_id: result.plugin_id,
            version: result.new_version,
            action: "upgraded".to_string(),
            app_id,
        })
    }

    /// 管控降级。
    ///
    /// 降级前校验版本一致性。
    /// 底层服务调用传入 `send_event=false`，由管控服务统一发布通知。
    pub async fn downgrade(&self, req: ControlDowngradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.deps.app_id.clone());

        self.check_version_consistency(&req.plugin_id, &req.target_version, &app_id).await?;

        let downgrade_req = DowngradeRequest {
            plugin_id: req.plugin_id.clone(),
            target_version: req.target_version,
            source: None,
            operator: None,
            app_id: Some(app_id.clone()),
            send_event: false,
        };

        let result = self.deps.downgrade_service.downgrade(downgrade_req).await?;

        let payload = PluginLifecyclePayload::new(&app_id, &result.plugin_id, &result.new_version)
            .with_old_version(&result.old_version);
        GlobalEventBus::get()
            .publish(plugin_events::DOWNGRADED, serde_json::to_value(&payload).unwrap())
            .await;

        // 发布 RuntimeLoad 通知
        if let Some(ref notifier) = self.deps.notifier {
            notifier.notify_runtime_load(&result.plugin_id, &result.new_version, &app_id).await;
        }

        Ok(ControlDeployResponse {
            plugin_id: result.plugin_id,
            version: result.new_version,
            action: "downgraded".to_string(),
            app_id,
        })
    }

    /// 管控卸载。
    ///
    /// 执行数据库清理（删除 cmx_plugin 记录、版本历史、服务定义），
    /// 完成后发布 RuntimeUnload 通知。
    /// 底层服务调用传入 `send_event=false`，由管控服务统一发布通知。
    pub async fn uninstall(&self, req: ControlUninstallRequest) -> PluginResult<()> {
        let app_id = req.app_id.unwrap_or_else(|| self.deps.app_id.clone());
        let plugin_id = req.plugin_id.clone();

        let existing = self.deps.repository.find_plugin(&plugin_id, &app_id).await?;
        let version = existing
            .as_ref()
            .map(|p| p.version.clone())
            .unwrap_or_default();

        let uninstall_req = crate::service::uninstall::UninstallRequest {
            plugin_id: plugin_id.clone(),
            force: false,
            operator: "control-service".to_string(),
            app_id: Some(app_id.clone()),
            send_event: false,
        };

        self.deps
            .uninstall_service
            .uninstall(uninstall_req)
            .await
            .map_err(|e| PluginError::Uninstall(format!("管控卸载失败: {}", e)))?;

        let payload = PluginLifecyclePayload::new(&app_id, &plugin_id, &version);
        GlobalEventBus::get()
            .publish(plugin_events::UNINSTALLED, serde_json::to_value(&payload).unwrap())
            .await;

        if let Some(ref notifier) = self.deps.notifier {
            notifier.notify_runtime_unload(&plugin_id, &version, &app_id).await;
        }

        Ok(())
    }
}
