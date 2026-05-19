//! 插件自动安装服务模块
//!
//! 支持在配置文件中声明式指定需要安装的插件列表，
//! 应用启动时自动检测并安装缺失的插件，确保多节点部署时插件一致性。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::error::PluginResult;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::install::{InstallRequest, InstallService};
use crate::service::upgrade::{UpgradeRequest, UpgradeService};
use crate::domain::plugin::PluginSource;

/// 自动安装配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoInstallConfig {
    /// 是否启用自动安装
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 插件列表
    #[serde(default)]
    pub plugins: Vec<AutoInstallPlugin>,
}

impl Default for AutoInstallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plugins: Vec::new(),
        }
    }
}

/// 自动安装插件配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoInstallPlugin {
    /// 插件ID
    pub plugin_id: String,
    /// 期望版本
    pub version: String,
    /// 来源类型：local / remote / marketplace / storage
    pub source_type: String,
    /// 来源地址（根据 source_type 解释为不同含义）
    /// - local: 文件系统路径
    /// - remote: 远程 URL
    /// - marketplace: 插件市场地址 例如 http://marketplace.yunext.com
    /// - storage: 文件 ID
    pub source_path: String,
    /// 是否关键插件（安装失败阻止启动）
    #[serde(default)]
    pub is_critical: bool,
    /// 目标数据库ID
    #[serde(default)]
    pub db_id: Option<String>,
    /// 安装超时时间（秒）
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

/// 默认值：true
fn default_true() -> bool {
    true
}

/// 默认超时时间：120秒
fn default_timeout() -> u64 {
    120
}

/// 安装动作
#[derive(Debug, Clone, PartialEq)]
pub enum InstallAction {
    /// 新安装
    Installed,
    /// 升级
    Upgraded,
    /// 跳过
    Skipped,
}

/// 自动安装结果
#[derive(Debug, Clone, Default)]
pub struct AutoInstallResult {
    /// 成功安装的插件
    pub installed: Vec<String>,
    /// 成功升级的插件
    pub upgraded: Vec<String>,
    /// 跳过的插件（已安装且版本一致）
    pub skipped: Vec<String>,
    /// 失败的插件及错误信息
    pub failed: Vec<(String, String)>,
    /// 是否有关键插件失败
    pub has_critical_failure: bool,
}

/// 插件自动安装服务
///
/// 根据配置文件中声明的插件列表，在启动时自动检测并安装缺失的插件。
/// 支持幂等操作（已安装且版本一致的插件自动跳过）和容错处理（区分关键/非关键插件）。
pub struct AutoInstallService {
    /// 插件数据仓库
    repository: Arc<PluginRepository>,
    /// 安装服务
    install_service: InstallService,
    /// 升级服务
    upgrade_service: UpgradeService,
}

impl AutoInstallService {
    /// 创建自动安装服务
    ///
    /// # 参数
    /// * `repository` - 插件数据仓库
    /// * `install_service` - 安装服务
    /// * `upgrade_service` - 升级服务
    pub fn new(
        repository: Arc<PluginRepository>,
        install_service: InstallService,
        upgrade_service: UpgradeService,
    ) -> Self {
        Self {
            repository,
            install_service,
            upgrade_service,
        }
    }

    /// 执行自动安装
    ///
    /// 遍历配置中的插件列表，对每个插件执行检测和安装/升级操作。
    /// 已安装且版本一致的插件自动跳过，版本不同的插件执行升级。
    ///
    /// # 参数
    /// * `config` - 自动安装配置
    ///
    /// # 返回值
    /// * `Ok(AutoInstallResult)` - 自动安装结果
    /// * `Err(PluginError)` - 执行过程中的错误
    pub async fn run(&self, config: &AutoInstallConfig) -> PluginResult<AutoInstallResult> {
        if !config.enabled {
            info!("插件自动安装已禁用，跳过");
            return Ok(AutoInstallResult::default());
        }

        if config.plugins.is_empty() {
            info!("插件自动安装列表为空，跳过");
            return Ok(AutoInstallResult::default());
        }

        info!("开始执行插件自动安装，共 {} 个插件", config.plugins.len());

        let mut result = AutoInstallResult::default();

        for plugin_config in &config.plugins {
            match self.process_plugin(plugin_config).await {
                Ok(action) => {
                    match action {
                        InstallAction::Installed => {
                            info!("插件 [{}] 自动安装成功", plugin_config.plugin_id);
                            result.installed.push(plugin_config.plugin_id.clone());
                        }
                        InstallAction::Upgraded => {
                            info!("插件 [{}] 自动升级成功", plugin_config.plugin_id);
                            result.upgraded.push(plugin_config.plugin_id.clone());
                        }
                        InstallAction::Skipped => {
                            info!("插件 [{}] 已安装且版本一致，跳过", plugin_config.plugin_id);
                            result.skipped.push(plugin_config.plugin_id.clone());
                        }
                    }
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    error!("插件 [{}] 自动安装失败: {}", plugin_config.plugin_id, err_msg);
                    if plugin_config.is_critical {
                        result.has_critical_failure = true;
                    }
                    result.failed.push((plugin_config.plugin_id.clone(), err_msg));
                }
            }
        }

        Ok(result)
    }

    /// 处理单个插件
    ///
    /// 检查插件是否已安装，根据状态决定执行安装、升级或跳过操作。
    async fn process_plugin(&self, config: &AutoInstallPlugin) -> PluginResult<InstallAction> {
        // 检查插件是否已安装
        let existing = self.repository.find_plugin(&config.plugin_id).await?;

        match existing {
            Some(plugin) => {
                // 已安装，比较版本
                if plugin.version == config.version {
                    info!(
                        "插件 [{}] 已安装且版本一致 ({})，跳过",
                        config.plugin_id, config.version
                    );
                    return Ok(InstallAction::Skipped);
                }

                // 版本不同，需要升级
                info!(
                    "插件 [{}] 需要升级: {} -> {}",
                    config.plugin_id, plugin.version, config.version
                );
                let source = self.build_source(config);
                let request = UpgradeRequest {
                    plugin_id: config.plugin_id.clone(),
                    source,
                    version_constraint: None,
                    force: false,
                    operator: Some("auto_install".to_string()),
                    build_type: None,
                    marketplace_source_id: None,
                };
                self.upgrade_service.upgrade(request).await?;
                Ok(InstallAction::Upgraded)
            }
            None => {
                // 未安装，执行安装
                info!(
                    "插件 [{}] 未安装，开始安装版本 {}",
                    config.plugin_id, config.version
                );
                let source = self.build_source(config);
                let request = InstallRequest {
                    source,
                    db_id: config.db_id.clone(),
                    auto_activate: true,
                    version_constraint: None,
                    build_type: None,
                    marketplace_source_id: None,
                };
                self.install_service.install(request).await?;
                Ok(InstallAction::Installed)
            }
        }
    }

    /// 根据配置构建 PluginSource
    ///
    /// 将配置中的 source_type 和 source_path 转换为 PluginSource 枚举。
    fn build_source(&self, config: &AutoInstallPlugin) -> PluginSource {
        match config.source_type.as_str() {
            "local" => PluginSource::Local {
                path: PathBuf::from(&config.source_path),
            },
            "url" | "remote" => PluginSource::Remote {
                url: config.source_path.clone(),
                checksum: None,
            },
            "registry" | "marketplace" => PluginSource::Marketplace {
                //fixme:这里需要填写插件服务的地址
                marketplace_url: Some(config.source_path.clone()),
                plugin_id: config.plugin_id.clone(),
            },
            "storage" => PluginSource::Storage {
                file_id: config.source_path.clone(),
                checksum: None,
            },
            _ => {
                warn!(
                    "未知的来源类型 '{}'，默认使用 local",
                    config.source_type
                );
                PluginSource::Local {
                    path: PathBuf::from(&config.source_path),
                }
            }
        }
    }
}
