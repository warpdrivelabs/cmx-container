//! 部署协调模块
//! 
//! 协调多节点部署

use std::sync::Arc;
use serde::{Deserialize, Serialize};

use super::node::NodeManager;

/// 部署策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeploymentStrategy {
    /// 所有节点
    AllNodes,
    /// 指定节点
    SpecificNodes(Vec<String>),
    /// 主节点
    PrimaryOnly,
    /// 随机N个节点
    RandomNodes(usize),
}

/// 部署状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentStatus {
    /// 待部署
    Pending,
    /// 部署中
    Deploying,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 回滚中
    RollingBack,
}

/// 部署任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentTask {
    /// 任务ID
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 部署策略
    pub strategy: DeploymentStrategy,
    /// 部署状态
    pub status: DeploymentStatus,
    /// 目标节点
    pub target_nodes: Vec<String>,
    /// 已完成节点
    pub completed_nodes: Vec<String>,
    /// 失败节点
    pub failed_nodes: Vec<String>,
}

/// 部署协调器
pub struct DeploymentCoordinator {
    /// 节点管理器
    node_manager: Arc<NodeManager>,
}

impl DeploymentCoordinator {
    /// 创建新的部署协调器
    pub fn new(node_manager: Arc<NodeManager>) -> Self {
        Self { node_manager }
    }
    
    /// 创建部署任务
    pub async fn create_deployment_task(
        &self,
        plugin_id: String,
        version: String,
        strategy: DeploymentStrategy,
    ) -> DeploymentTask {
        let target_nodes = self.resolve_target_nodes(&strategy).await;
        
        DeploymentTask {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id,
            version,
            strategy,
            status: DeploymentStatus::Pending,
            target_nodes,
            completed_nodes: Vec::new(),
            failed_nodes: Vec::new(),
        }
    }
    
    /// 解析目标节点
    async fn resolve_target_nodes(&self, strategy: &DeploymentStrategy) -> Vec<String> {
        match strategy {
            DeploymentStrategy::AllNodes => {
                self.node_manager.get_online_nodes().await
                    .into_iter()
                    .map(|n| n.id)
                    .collect()
            }
            DeploymentStrategy::SpecificNodes(nodes) => nodes.clone(),
            DeploymentStrategy::PrimaryOnly => {
                // 选择主节点
                if let Some(master) = self.node_manager.select_master_node().await {
                    vec![master.id]
                } else {
                    // 没有健康节点，使用当前节点
                    vec![self.node_manager.current_node_id().to_string()]
                }
            }
            DeploymentStrategy::RandomNodes(count) => {
                let mut nodes: Vec<_> = self.node_manager.get_online_nodes().await
                    .into_iter()
                    .map(|n| n.id)
                    .collect();
                nodes.truncate(*count);
                nodes
            }
        }
    }
}
