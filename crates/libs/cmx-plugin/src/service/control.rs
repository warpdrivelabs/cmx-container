//! 插件管控服务。
//!
//! 提供集中式插件元数据初始化能力，仅执行 DDL/DML 操作，
//! 不触发本地运行时加载。完成后发布 RuntimeLoad 通知。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::common::{DefinitionUtils, PackageUtils, PackageUtilsDeps};
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::storage::TempDirCleanup;
use crate::service::executor::PluginOperationExecutor;

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
pub struct ControlServiceDeps {
    /// 插件操作编排器
    pub executor: Arc<PluginOperationExecutor>,
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 应用ID
    pub app_id: String,
}

/// 插件管控服务。
///
/// 通过 PluginOperationExecutor 统一编排持久化 → 运行时 → 事件发布流程，
/// 管控模式仅执行 DDL/DML 操作，不触发本地运行时加载，
/// 完成后由 executor 内部发布 RuntimeLoad 通知。
///
/// ## 版本一致性约束
///
/// 同一插件在所有 app_id 下必须安装相同版本（DDL 物理表共享）。
/// 安装/升级/降级前会校验版本一致性，冲突时拒绝操作。
pub struct ControlService {
    /// 依赖注入
    deps: ControlServiceDeps,
    /// 包处理工具（仅 deploy 方法使用，用于解析插件包元数据）
    package_utils: PackageUtils,
}

impl ControlService {
    /// 创建管控服务实例。
    pub fn new(deps: ControlServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: PathBuf::from("/tmp/cmx_plugin_control"),
            temp_root: PathBuf::from("/tmp/cmx_plugin_control_tmp"),
            storage: None,
        });
        Self {
            deps,
            package_utils,
        }
    }

    /// 创建管控服务实例（带包处理依赖）。
    ///
    /// deploy 方法需要 PackageUtils 来获取和解析插件包元数据，
    /// 以决定执行安装、升级还是覆盖安装。
    pub fn with_package_utils(deps: ControlServiceDeps, package_utils: PackageUtils) -> Self {
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

    /// 管控部署（自动判断安装/升级/覆盖安装）。
    ///
    /// 完整流程：
    /// 1. 获取并解压插件包到临时目录
    /// 2. 解析元数据（获取 plugin_id 和 version）
    /// 3. 查询当前插件安装状态
    /// 4. 根据状态决定调用 executor 的对应管控方法
    pub async fn deploy(&self, req: ControlDeployRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.clone().unwrap_or_else(|| self.deps.app_id.clone());

        // 步骤1: 获取插件包
        let package_path = self
            .package_utils
            .fetch_package(&req.source, None, "管控部署")
            .await?;

        // 步骤2: 解压到临时目录
        let temp_dir = PathBuf::from(format!("/tmp/plugin_control_{}", uuid::Uuid::new_v4()));
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
                // 未安装，执行管控安装
                info!(
                    "管控部署: 插件 {} 未安装，执行安装流程, version={}",
                    plugin_id, new_version
                );

                let control_req = ControlInstallRequest {
                    source: req.source.clone(),
                    db_id: req.db_id.or(plugin_def.datasource_id.clone()),
                    build_type: req.build_type.clone(),
                    app_id: Some(app_id.clone()),
                };

                self.deps.executor.execute_control_install(control_req).await
            }
            Some(existing) => {
                // 已安装，判断版本关系
                let old_version = &existing.version;

                match new_version.cmp(old_version) {
                    std::cmp::Ordering::Greater => {
                        // 新版本 > 旧版本，执行管控升级
                        info!(
                            "管控部署: 插件 {} 已安装 version={}，目标 version={}，执行升级",
                            plugin_id, old_version, new_version
                        );

                        let control_req = ControlUpgradeRequest {
                            plugin_id: plugin_id.clone(),
                            target_version: new_version.clone(),
                            source: req.source.clone(),
                            build_type: req.build_type.clone(),
                            app_id: Some(app_id.clone()),
                        };

                        self.deps.executor.execute_control_upgrade(control_req).await
                    }
                    std::cmp::Ordering::Equal => {
                        // 版本相同，执行覆盖安装
                        info!(
                            "管控部署: 插件 {} 已安装相同版本 {}, 执行覆盖安装",
                            plugin_id, new_version
                        );

                        let deploy_req = crate::service::deploy::DeployRequest {
                            source: req.source.clone(),
                            db_id: req.db_id.or(plugin_def.datasource_id.clone()),
                            force_reinstall: true,
                            build_type: req.build_type.clone(),
                            publish_to_marketplace: false,
                            app_id: Some(app_id.clone()),
                            send_event: false,
                            marketplace_source_id: None,
                            marketplace_publish_info: None,
                        };

                        let result = self
                            .deps
                            .executor
                            .execute_reinstall(deploy_req, &plugin_id, old_version)
                            .await?;

                        Ok(ControlDeployResponse {
                            plugin_id: result.plugin_id,
                            version: result.new_version,
                            action: "reinstalled".to_string(),
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
    /// 事件发布由 executor 内部统一处理。
    pub async fn install(&self, req: ControlInstallRequest) -> PluginResult<ControlDeployResponse> {
        self.deps.executor.execute_control_install(req).await
    }

    /// 管控升级。
    ///
    /// 升级前校验版本一致性。
    /// 事件发布由 executor 内部统一处理。
    pub async fn upgrade(&self, req: ControlUpgradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.clone().unwrap_or_else(|| self.deps.app_id.clone());

        // 升级前先校验版本一致性（使用目标版本）
        self.check_version_consistency(&req.plugin_id, &req.target_version, &app_id).await?;

        self.deps.executor.execute_control_upgrade(req).await
    }

    /// 管控降级。
    ///
    /// 降级前校验版本一致性。
    /// 事件发布由 executor 内部统一处理。
    pub async fn downgrade(&self, req: ControlDowngradeRequest) -> PluginResult<ControlDeployResponse> {
        let app_id = req.app_id.clone().unwrap_or_else(|| self.deps.app_id.clone());

        self.check_version_consistency(&req.plugin_id, &req.target_version, &app_id).await?;

        self.deps.executor.execute_control_downgrade(req).await
    }

    /// 管控卸载。
    ///
    /// 执行数据库清理（删除 cmx_plugin 记录、版本历史、服务定义），
    /// 完成后由 executor 内部发布 RuntimeUnload 通知。
    pub async fn uninstall(&self, req: ControlUninstallRequest) -> PluginResult<()> {
        self.deps.executor.execute_control_uninstall(req).await
    }
}
