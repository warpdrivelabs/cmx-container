//! 节点管理模块
//! 
//! 管理集群节点

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 节点状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 维护中
    Maintenance,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeStatus::Online => write!(f, "online"),
            NodeStatus::Offline => write!(f, "offline"),
            NodeStatus::Maintenance => write!(f, "maintenance"),
        }
    }
}

/// 节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// 节点ID
    pub id: String,
    /// 节点名称
    pub name: String,
    /// 节点地址
    pub address: String,
    /// 节点状态
    pub status: NodeStatus,
    /// 最后心跳时间
    pub last_heartbeat: DateTime<Utc>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl NodeInfo {
    /// 创建新的节点信息
    pub fn new(id: String, name: String, address: String) -> Self {
        Self {
            id,
            name,
            address,
            status: NodeStatus::Online,
            last_heartbeat: Utc::now(),
            metadata: HashMap::new(),
        }
    }
    
    /// 检查节点是否健康
    pub fn is_healthy(&self, timeout_seconds: i64) -> bool {
        if self.status != NodeStatus::Online {
            return false;
        }
        
        let elapsed = (Utc::now() - self.last_heartbeat).num_seconds();
        elapsed < timeout_seconds
    }
}

/// 节点管理器配置
#[derive(Debug, Clone)]
pub struct NodeManagerConfig {
    /// 心跳超时时间（秒）
    pub heartbeat_timeout_seconds: i64,
    /// 健康检查间隔（秒）
    pub health_check_interval_seconds: u64,
    /// 是否启用分布式锁
    pub enable_distributed_lock: bool,
}

impl Default for NodeManagerConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_seconds: 30,
            health_check_interval_seconds: 10,
            enable_distributed_lock: true,
        }
    }
}

/// 节点管理器
pub struct NodeManager {
    /// 当前节点ID
    current_node_id: String,
    /// 节点列表
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    /// 配置
    config: NodeManagerConfig,
    /// 分布式锁管理器（可选）
    lock_manager: Option<Arc<cmx_buffer::LockManager>>,
}

impl NodeManager {
    /// 创建新的节点管理器
    pub fn new(current_node_id: String) -> Self {
        Self {
            current_node_id,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            config: NodeManagerConfig::default(),
            lock_manager: None,
        }
    }
    
    /// 使用配置创建节点管理器
    pub fn with_config(current_node_id: String, config: NodeManagerConfig) -> Self {
        Self {
            current_node_id,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            config,
            lock_manager: None,
        }
    }
    
    /// 设置分布式锁管理器
    pub fn with_lock_manager(mut self, lock_manager: Arc<cmx_buffer::LockManager>) -> Self {
        self.lock_manager = Some(lock_manager);
        self
    }
    
    /// 获取当前节点ID
    pub fn current_node_id(&self) -> &str {
        &self.current_node_id
    }
    
    /// 注册节点
    pub async fn register_node(&self, node: NodeInfo) {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node.id.clone(), node);
    }
    
    /// 注销节点
    pub async fn unregister_node(&self, node_id: &str) -> Option<NodeInfo> {
        let mut nodes = self.nodes.write().await;
        nodes.remove(node_id)
    }
    
    /// 获取节点
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
            .filter(|n| n.status == NodeStatus::Online)
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
    
    /// 更新节点状态
    pub async fn update_node_status(&self, node_id: &str, status: NodeStatus) {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(node_id) {
            node.status = status;
            node.last_heartbeat = Utc::now();
        }
    }
    
    /// 更新心跳
    pub async fn update_heartbeat(&self, node_id: &str) {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(node_id) {
            node.last_heartbeat = Utc::now();
        }
    }
    
    /// 检查节点健康状态并更新
    pub async fn check_and_update_health(&self) -> Vec<String> {
        let mut nodes = self.nodes.write().await;
        let mut unhealthy = Vec::new();
        
        for (id, node) in nodes.iter_mut() {
            if node.status == NodeStatus::Online && !node.is_healthy(self.config.heartbeat_timeout_seconds) {
                node.status = NodeStatus::Offline;
                unhealthy.push(id.clone());
            }
        }
        
        unhealthy
    }
    
    /// 获取节点数量
    pub async fn node_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.len()
    }
    
    /// 获取在线节点数量
    pub async fn online_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.values().filter(|n| n.status == NodeStatus::Online).count()
    }
    
    /// 获取健康节点数量
    pub async fn healthy_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        nodes.values()
            .filter(|n| n.is_healthy(self.config.heartbeat_timeout_seconds))
            .count()
    }
    
    /// 选择最优节点（负载最低）
    /// 
    /// 基于负载选择最优节点：
    /// 1. 优先选择活跃插件数量最少的节点
    /// 2. 如果插件数量相同，选择最近心跳时间最近的节点
    pub async fn select_best_node(&self) -> Option<NodeInfo> {
        let nodes = self.get_healthy_nodes().await;
        
        // 按负载排序（活跃插件数量升序，心跳时间降序）
        nodes.into_iter()
            .min_by(|a, b| {
                let a_load = a.metadata.get("active_plugins")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                let b_load = b.metadata.get("active_plugins")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                
                // 先比较负载
                match a_load.cmp(&b_load) {
                    std::cmp::Ordering::Equal => {
                        // 负载相同，选择心跳时间最近的
                        b.last_heartbeat.cmp(&a.last_heartbeat)
                    }
                    other => other,
                }
            })
    }
    
    /// 选择指定数量的节点
    pub async fn select_nodes(&self, count: usize) -> Vec<NodeInfo> {
        let nodes = self.get_healthy_nodes().await;
        nodes.into_iter().take(count).collect()
    }
    
    /// 选择主节点
    /// 
    /// 使用一致性哈希算法选择主节点。
    pub async fn select_master_node(&self) -> Option<NodeInfo> {
        let nodes = self.get_healthy_nodes().await;
        
        if nodes.is_empty() {
            return None;
        }
        
        // 使用节点ID的最小哈希值作为主节点
        nodes.into_iter()
            .min_by_key(|n| {
                // 简单哈希：使用节点ID的哈希值
                let mut hash: u64 = 0;
                for c in n.id.chars() {
                    hash = hash.wrapping_mul(31).wrapping_add(c as u64);
                }
                hash
            })
    }
    
    /// 检查当前节点是否为主节点
    pub async fn is_master(&self) -> bool {
        if let Some(master) = self.select_master_node().await {
            master.id == self.current_node_id
        } else {
            false
        }
    }
    
    /// 更新节点负载信息
    /// 
    /// 更新节点的活跃插件数量等负载信息。
    pub async fn update_node_load(&self, node_id: &str, active_plugins: u32) {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(node_id) {
            node.metadata.insert("active_plugins".to_string(), active_plugins.to_string());
            node.last_heartbeat = Utc::now();
        }
    }
    
    /// 获取节点负载信息
    pub async fn get_node_load(&self, node_id: &str) -> Option<u32> {
        let nodes = self.nodes.read().await;
        nodes.get(node_id)
            .and_then(|n| n.metadata.get("active_plugins"))
            .and_then(|v| v.parse::<u32>().ok())
    }
    
    /// 使用分布式锁执行操作
    pub async fn with_lock<F, T>(&self, lock_key: &str, f: F) -> crate::error::PluginResult<T>
    where
        F: std::future::Future<Output = crate::error::PluginResult<T>>,
    {
        if let Some(ref lock_manager) = self.lock_manager {
            let _guard = lock_manager.lock(lock_key, cmx_buffer::LockOptions::default()).await
                .map_err(|e| crate::error::PluginError::Node(format!("获取分布式锁失败: {}", e)))?;
            
            f.await
        } else {
            f.await
        }
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::new("default-node".to_string())
    }
}
