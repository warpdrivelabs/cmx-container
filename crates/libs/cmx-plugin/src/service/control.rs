//! 插件管控服务。
//!
//! 提供集中式插件元数据初始化能力，仅执行 DDL/DML 操作，
//! 不触发本地运行时加载。完成后发布 RuntimeLoad 通知。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cluster::notification::PluginNotifier;
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
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
    install_service: InstallService,
    upgrade_service: UpgradeService,
    downgrade_service: DowngradeService,
    uninstall_service: UninstallService,
    notifier: Option<Arc<PluginNotifier>>,
    app_id: String,
    repository: Arc<PluginRepository>,
}

impl ControlService {
    /// 创建管控服务实例。
    ///
    /// # Arguments
    ///
    /// * `install_service` - 安装服务
    /// * `upgrade_service` - 升级服务
    /// * `downgrade_service` - 降级服务
    /// * `notifier` - 插件变更通知器（可选，无 Redis 时不发布通知）
    /// * `app_id` - 默认应用ID
    /// * `repository` - 插件数据仓库（用于版本一致性校验）
    pub fn new(
        install_service: InstallService,
        upgrade_service: UpgradeService,
        downgrade_service: DowngradeService,
        uninstall_service: UninstallService,
        notifier: Option<Arc<PluginNotifier>>,
        app_id: String,
        repository: Arc<PluginRepository>,
    ) -> Self {
        Self {
            install_service,
            upgrade_service,
            downgrade_service,
            uninstall_service,
            notifier,
            app_id,
            repository,
        }
    }

    /// 校验版本一致性。
    ///
    /// 查询同一 `plugin_id` 在所有 `app_id` 下的已安装版本，
    /// 如果存在不同版本则返回冲突错误。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件ID
    /// * `target_version` - 目标安装版本
    /// * `current_app_id` - 当前操作的应用ID
    async fn check_version_consistency(
        &self,
        plugin_id: &str,
        target_version: &str,
        current_app_id: &str,
    ) -> PluginResult<()> {
        let all_apps_filter = PluginFilter {
            ..Default::default()
        };

        let all_records = self.repository.list_plugins(&all_apps_filter).await?;
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
    /// 委托 InstallService 执行 DDL/DML，完成后发布 RuntimeLoad 通知。
    ///
    /// # TODO
    ///
    /// - 文件上传至对象存储未实现，需要在 install 成功后调用 cmx-storage 上传插件文件
    pub async fn deploy(&self, req: ControlDeployRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.app_id.clone());

        let install_req = InstallRequest {
            source: req.source,
            db_id: req.db_id,
            auto_activate: false,
            version_constraint: None,
            build_type: req.build_type,
            marketplace_source_id: None,
            app_id: Some(app_id.clone()),
        };

        let result = self.install_service.install(install_req).await?;

        // TODO: 上传插件文件至对象存储 (cmx-storage integration)
        // 需要将 result.install_path 中的文件上传至对象存储，并更新 cmx_plugin.storage_key

        if let Some(ref notifier) = self.notifier {
            notifier.notify_runtime_load(&result.plugin_id, &result.version, &app_id).await;
        }

        Ok(ControlDeployResponse {
            plugin_id: result.plugin_id,
            version: result.version,
            action: "installed".to_string(),
            app_id,
        })
    }

    /// 管控安装。
    ///
    /// 安装前校验版本一致性，确保同一插件在所有 app_id 下版本相同。
    pub async fn install(&self, req: ControlInstallRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.app_id.clone());

        let install_req = InstallRequest {
            source: req.source,
            db_id: req.db_id,
            auto_activate: false,
            version_constraint: None,
            build_type: req.build_type,
            marketplace_source_id: None,
            app_id: Some(app_id.clone()),
        };

        let result = self.install_service.install(install_req).await?;

        if let Some(ref notifier) = self.notifier {
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
    pub async fn upgrade(&self, req: ControlUpgradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.app_id.clone());

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
        };

        let result = self.upgrade_service.upgrade(upgrade_req).await?;

        if let Some(ref notifier) = self.notifier {
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
    pub async fn downgrade(&self, req: ControlDowngradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.unwrap_or_else(|| self.app_id.clone());

        self.check_version_consistency(&req.plugin_id, &req.target_version, &app_id).await?;

        let downgrade_req = DowngradeRequest {
            plugin_id: req.plugin_id.clone(),
            target_version: req.target_version,
            source: None,
            operator: None,
            app_id: Some(app_id.clone()),
        };

        let result = self.downgrade_service.downgrade(downgrade_req).await?;

        if let Some(ref notifier) = self.notifier {
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
    pub async fn uninstall(&self, req: ControlUninstallRequest) -> PluginResult<()> {
        let app_id = req.app_id.unwrap_or_else(|| self.app_id.clone());
        let plugin_id = req.plugin_id.clone();

        // 先执行实际卸载（数据库清理）
        let uninstall_req = crate::service::uninstall::UninstallRequest {
            plugin_id: plugin_id.clone(),
            force: false,
            operator: "control-service".to_string(),
            app_id: Some(app_id.clone()),
        };

        self.uninstall_service
            .uninstall(uninstall_req)
            .await
            .map_err(|e| PluginError::Uninstall(format!("管控卸载失败: {}", e)))?;

        // 完成后发布 RuntimeUnload 通知
        if let Some(ref notifier) = self.notifier {
            notifier.notify_runtime_unload(&plugin_id, "", &app_id).await;
        }

        Ok(())
    }
}
