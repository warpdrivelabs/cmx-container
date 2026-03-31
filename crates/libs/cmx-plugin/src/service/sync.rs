//! 节点同步服务模块
//!
//! 处理节点启动时的插件版本同步

use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::error::PluginResult;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::repository::PluginRepository;
use crate::audit::logger::AuditLogger;

/// 同步结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    /// 需要升级的插件列表 (plugin_id, current_version, target_version)
    pub upgrades: Vec<(String, String, String)>,
    /// 需要降级的插件列表 (plugin_id, current_version, target_version)
    pub downgrades: Vec<(String, String, String)>,
    /// 已同步的插件列表
    pub synced: Vec<String>,
}

/// 同步服务依赖
pub struct NodeSyncServiceDeps {
    /// 插件仓库
    pub repository: Arc<PluginRepository>,
    /// 部署仓库
    pub deployment_repository: Arc<DeploymentRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 节点ID
    pub node_id: String,
    /// 节点名称
    pub node_name: Option<String>,
    /// 节点类型
    pub node_type: Option<String>,
}

/// 节点同步服务
pub struct NodeSyncService {
    deps: NodeSyncServiceDeps,
}

impl NodeSyncService {
    /// 创建新的节点同步服务
    pub fn new(deps: NodeSyncServiceDeps) -> Self {
        Self { deps }
    }

    /// 同步节点上的所有插件
    pub async fn sync_node_plugins(&self) -> PluginResult<SyncResult> {
        let deployments = self.deps.deployment_repository
            .list_node_deployments(&self.deps.node_id)
            .await?;

        let plugins = self.deps.repository.list_plugins(&Default::default()).await?;

        let mut upgrades = Vec::new();
        let mut downgrades = Vec::new();
        let mut synced = Vec::new();

        for plugin in plugins {
            let deployment = deployments.iter().find(|d| d.plugin_id == plugin.plugin_id);

            match deployment {
                Some(d) => {
                    let cmp = plugin.version.cmp(&d.version);
                    match cmp {
                        std::cmp::Ordering::Less => {
                            upgrades.push((plugin.plugin_id.clone(), d.version.clone(), plugin.version.clone()));
                        }
                        std::cmp::Ordering::Greater => {
                            downgrades.push((plugin.plugin_id.clone(), d.version.clone(), plugin.version.clone()));
                        }
                        std::cmp::Ordering::Equal => {
                            synced.push(plugin.plugin_id.clone());
                        }
                    }
                }
                None => {
                    upgrades.push((plugin.plugin_id.clone(), "none".to_string(), plugin.version.clone()));
                }
            }
        }

        Ok(SyncResult { upgrades, downgrades, synced })
    }

    /// 获取需要升级的插件列表
    pub async fn get_upgrades(&self) -> PluginResult<Vec<(String, String, String)>> {
        let result = self.sync_node_plugins().await?;
        Ok(result.upgrades)
    }

    /// 获取需要降级的插件列表
    pub async fn get_downgrades(&self) -> PluginResult<Vec<(String, String, String)>> {
        let result = self.sync_node_plugins().await?;
        Ok(result.downgrades)
    }

    /// 获取节点当前部署状态
    pub async fn get_node_deployment_status(&self) -> PluginResult<Vec<NodeDeploymentStatus>> {
        let deployments = self.deps.deployment_repository
            .list_node_deployments(&self.deps.node_id)
            .await?;

        let mut statuses = Vec::new();
        for deployment in deployments {
            let baseline_version = self.deps.repository
                .get_baseline_version(&deployment.plugin_id)
                .await?;

            let sync_state = match &baseline_version {
                Some(bv) => {
                    match bv.cmp(&deployment.version) {
                        std::cmp::Ordering::Less => SyncState::BehindBaseline,
                        std::cmp::Ordering::Greater => SyncState::AheadBaseline,
                        std::cmp::Ordering::Equal => SyncState::InSync,
                    }
                }
                None => SyncState::NotInstalled,
            };

            statuses.push(NodeDeploymentStatus {
                plugin_id: deployment.plugin_id,
                deployed_version: deployment.version,
                baseline_version,
                sync_state,
                deployed_at: deployment.create_time,
                status: deployment.status,
            });
        }

        Ok(statuses)
    }
}

/// 节点部署状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDeploymentStatus {
    /// 插件ID
    pub plugin_id: String,
    /// 部署版本
    pub deployed_version: String,
    /// 基线版本
    pub baseline_version: Option<String>,
    /// 同步状态
    pub sync_state: SyncState,
    /// 部署时间
    pub deployed_at: chrono::DateTime<chrono::Utc>,
    /// 状态
    pub status: String,
}

/// 同步状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    /// 已同步
    InSync,
    /// 落后于基线（需要升级）
    BehindBaseline,
    /// 领先于基线（需要降级）
    AheadBaseline,
    /// 未安装
    NotInstalled,
    /// 未知状态
    Unknown,
}

impl std::fmt::Display for SyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncState::InSync => write!(f, "in_sync"),
            SyncState::BehindBaseline => write!(f, "behind_baseline"),
            SyncState::AheadBaseline => write!(f, "ahead_baseline"),
            SyncState::NotInstalled => write!(f, "not_installed"),
            SyncState::Unknown => write!(f, "unknown"),
        }
    }
}
