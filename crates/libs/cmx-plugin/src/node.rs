//! 节点管理器模块 - 集群节点管理
//!
//! 提供集群节点的注册、发现、健康检查和负载均衡功能。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 节点状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 维护中
    Maintenance,
    /// 不可达
    Unreachable,
}

/// 节点类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// 主节点
    Master,
    /// 工作节点
    Worker,
    /// 边缘节点
    Edge,
}

/// 节点能力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapability {
    /// CPU 核心数
    pub cpu_cores: u32,
    /// 内存大小 (MB)
    pub memory_mb: u64,
    /// 磁盘空间 (GB)
    pub disk_gb: u64,
    /// 支持的运行时
    pub runtimes: Vec<String>,
    /// 标签
    pub labels: HashMap<String, String>,
}

impl Default for NodeCapability {
    fn default() -> Self {
        Self {
            cpu_cores: 4,
            memory_mb: 8192,
            disk_gb: 100,
            runtimes: vec!["wasm".to_string()],
            labels: HashMap::new(),
        }
    }
}

/// 节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// 节点 ID
    pub node_id: String,
    /// 节点名称
    pub node_name: String,
    /// 节点类型
    pub node_type: NodeType,
    /// 节点状态
    pub state: NodeState,
    /// 主机地址
    pub host: String,
    /// 端口
    pub port: u16,
    /// 节点能力
    pub capabilities: NodeCapability,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 最后心跳时间
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// 注册时间
    pub registered_at: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
}

impl NodeInfo {
    /// 创建新节点
    pub fn new(node_id: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let now = Utc::now();
        Self {
            node_id: node_id.into(),
            node_name: String::new(),
            node_type: NodeType::Worker,
            state: NodeState::Online,
            host: host.into(),
            port,
            capabilities: NodeCapability::default(),
            metadata: HashMap::new(),
            last_heartbeat: Some(now),
            registered_at: now,
            update_time: now,
        }
    }

    /// 设置节点名称
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.node_name = name.into();
        self
    }

    /// 设置节点类型
    pub fn with_type(mut self, node_type: NodeType) -> Self {
        self.node_type = node_type;
        self
    }

    /// 设置节点能力
    pub fn with_capabilities(mut self, capabilities: NodeCapability) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// 更新心跳
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        self.update_time = Utc::now();
    }

    /// 检查节点是否健康
    pub fn is_healthy(&self, timeout_seconds: u64) -> bool {
        if self.state != NodeState::Online {
            return false;
        }

        if let Some(last_heartbeat) = self.last_heartbeat {
            let elapsed = (Utc::now() - last_heartbeat).num_seconds() as u64;
            elapsed < timeout_seconds
        } else {
            false
        }
    }

    /// 获取节点地址
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// 节点统计信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeStats {
    /// 总节点数
    pub total: usize,
    /// 在线节点数
    pub online: usize,
    /// 离线节点数
    pub offline: usize,
    /// 维护中节点数
    pub maintenance: usize,
    /// 主节点数
    pub masters: usize,
    /// 工作节点数
    pub workers: usize,
}

/// 节点选择策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeSelectionStrategy {
    /// 随机选择
    Random,
    /// 轮询
    RoundRobin,
    /// 最少连接
    LeastConnections,
    /// 资源最充足
    MostResources,
}

/// 节点管理器配置
#[derive(Debug, Clone)]
pub struct NodeManagerConfig {
    /// 心跳超时时间（秒）
    pub heartbeat_timeout_seconds: u64,
    /// 健康检查间隔（秒）
    pub health_check_interval_seconds: u64,
    /// 节点选择策略
    pub selection_strategy: NodeSelectionStrategy,
}

impl Default for NodeManagerConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_seconds: 30,
            health_check_interval_seconds: 10,
            selection_strategy: NodeSelectionStrategy::RoundRobin,
        }
    }
}

/// 节点管理器 - 管理集群节点
pub struct NodeManager {
    config: NodeManagerConfig,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    round_robin_index: Arc<RwLock<usize>>,
}

impl NodeManager {
    /// 创建新的节点管理器
    pub fn new(config: NodeManagerConfig) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            round_robin_index: Arc::new(RwLock::new(0)),
        }
    }

    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        Self::new(NodeManagerConfig::default())
    }

    /// 注册节点
    pub async fn register(&self, node: NodeInfo) -> Result<(), NodeError> {
        let node_id = node.node_id.clone();

        let mut nodes = self.nodes.write().await;

        if nodes.contains_key(&node_id) {
            return Err(NodeError::AlreadyRegistered(node_id));
        }

        log::info!("注册节点: {} ({})", node_id, node.address());
        nodes.insert(node_id, node);

        Ok(())
    }

    /// 注销节点
    pub async fn unregister(&self, node_id: &str) -> Result<(), NodeError> {
        let mut nodes = self.nodes.write().await;

        if nodes.remove(node_id).is_some() {
            log::info!("注销节点: {}", node_id);
            Ok(())
        } else {
            Err(NodeError::NotFound(node_id.to_string()))
        }
    }

    /// 更新节点心跳
    pub async fn heartbeat(&self, node_id: &str) -> Result<(), NodeError> {
        let mut nodes = self.nodes.write().await;

        match nodes.get_mut(node_id) {
            Some(node) => {
                node.update_heartbeat();
                node.state = NodeState::Online;
                Ok(())
            }
            None => Err(NodeError::NotFound(node_id.to_string())),
        }
    }

    /// 设置节点状态
    pub async fn set_state(&self, node_id: &str, state: NodeState) -> Result<(), NodeError> {
        let mut nodes = self.nodes.write().await;

        match nodes.get_mut(node_id) {
            Some(node) => {
                node.state = state;
                node.update_time = Utc::now();
                log::info!("节点 {} 状态变更为 {:?}", node_id, state);
                Ok(())
            }
            None => Err(NodeError::NotFound(node_id.to_string())),
        }
    }

    /// 获取节点信息
    pub async fn get_node(&self, node_id: &str) -> Option<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id).cloned()
    }

    /// 获取所有节点
    pub async fn get_all_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values().cloned().collect()
    }

    /// 获取在线节点
    pub async fn get_online_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| n.state == NodeState::Online)
            .cloned()
            .collect()
    }

    /// 获取健康节点
    pub async fn get_healthy_nodes(&self) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| n.is_healthy(self.config.heartbeat_timeout_seconds))
            .cloned()
            .collect()
    }

    /// 选择节点（根据策略）
    pub async fn select_node(&self) -> Option<NodeInfo> {
        let healthy_nodes = self.get_healthy_nodes().await;

        if healthy_nodes.is_empty() {
            return None;
        }

        match self.config.selection_strategy {
            NodeSelectionStrategy::Random => {
                let idx = rand::random::<usize>() % healthy_nodes.len();
                Some(healthy_nodes[idx].clone())
            }
            NodeSelectionStrategy::RoundRobin => {
                let mut index = self.round_robin_index.write().await;
                let idx = *index % healthy_nodes.len();
                *index = (*index + 1) % healthy_nodes.len();
                Some(healthy_nodes[idx].clone())
            }
            NodeSelectionStrategy::LeastConnections => {
                // TODO: 实现基于连接数的选择
                healthy_nodes.first().cloned()
            }
            NodeSelectionStrategy::MostResources => {
                // TODO: 实现基于资源的选择
                healthy_nodes.first().cloned()
            }
        }
    }

    /// 选择指定数量的节点
    pub async fn select_nodes(&self, count: usize) -> Vec<NodeInfo> {
        let healthy_nodes = self.get_healthy_nodes().await;
        healthy_nodes.into_iter().take(count).collect()
    }

    /// 按类型选择节点
    pub async fn select_nodes_by_type(&self, node_type: NodeType) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| n.node_type == node_type && n.is_healthy(self.config.heartbeat_timeout_seconds))
            .cloned()
            .collect()
    }

    /// 按标签选择节点
    pub async fn select_nodes_by_label(&self, key: &str, value: &str) -> Vec<NodeInfo> {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| {
                n.is_healthy(self.config.heartbeat_timeout_seconds) &&
                n.capabilities.labels.get(key).map_or(false, |v| v == value)
            })
            .cloned()
            .collect()
    }

    /// 获取节点统计
    pub async fn get_stats(&self) -> NodeStats {
        let nodes = self.nodes.read().await;

        let mut stats = NodeStats::default();
        stats.total = nodes.len();

        for node in nodes.values() {
            match node.state {
                NodeState::Online => stats.online += 1,
                NodeState::Offline => stats.offline += 1,
                NodeState::Maintenance => stats.maintenance += 1,
                NodeState::Unreachable => stats.offline += 1,
            }

            match node.node_type {
                NodeType::Master => stats.masters += 1,
                NodeType::Worker => stats.workers += 1,
                NodeType::Edge => stats.workers += 1,
            }
        }

        stats
    }

    /// 健康检查 - 标记不健康的节点
    pub async fn health_check(&self) -> Vec<String> {
        let mut unhealthy = Vec::new();
        let mut nodes = self.nodes.write().await;

        for (node_id, node) in nodes.iter_mut() {
            if !node.is_healthy(self.config.heartbeat_timeout_seconds) {
                if node.state == NodeState::Online {
                    node.state = NodeState::Unreachable;
                    node.update_time = Utc::now();
                    unhealthy.push(node_id.clone());
                    log::warn!("节点 {} 心跳超时，标记为不可达", node_id);
                }
            }
        }

        unhealthy
    }

    /// 获取节点数量
    pub async fn node_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.len()
    }

    /// 检查节点是否存在
    pub async fn has_node(&self, node_id: &str) -> bool {
        let nodes = self.nodes.read().await;
        nodes.contains_key(node_id)
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::with_default_config()
    }
}

/// 节点错误
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("节点已注册: {0}")]
    AlreadyRegistered(String),
    #[error("节点不存在: {0}")]
    NotFound(String),
    #[error("节点不可用: {0}")]
    Unavailable(String),
    #[error("节点选择失败: {0}")]
    SelectionFailed(String),
}
