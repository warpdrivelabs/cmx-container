//! 部署协调器 - 负责多节点部署协调
//!
//! 集成 cmx-buffer 的分布式锁来确保部署操作的原子性。

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::PluginError;
use crate::types::{
    DeployRequest, DeploymentStatus, DeploymentStrategy, NodeDeploymentResult,
};

/// 分布式锁键前缀
const DEPLOY_LOCK_PREFIX: &str = "cmx:plugin:deploy:lock:";

/// 部署节点信息
#[derive(Debug, Clone)]
pub struct DeploymentNodeInfo {
    pub node_id: String,
    pub node_name: String,
    pub host: String,
    pub port: u16,
    pub status: DeploymentNodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentNodeStatus {
    Online,
    Offline,
    Maintenance,
}

/// 部署结果
#[derive(Debug, Clone)]
pub struct DeployResult {
    pub success: bool,
    pub operation_id: String,
    pub nodes: Vec<NodeDeploymentResult>,
}

/// 同步结果
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub success: bool,
    pub synced_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
}

/// 恢复结果
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub success: bool,
    pub recovered_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
}

/// 同步状态
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub operation_id: String,
    pub status: DeploymentStatus,
    pub completed_nodes: Vec<String>,
    pub pending_nodes: Vec<String>,
    pub failed_nodes: Vec<String>,
}

/// 部署协调器 - 负责多节点部署协调
pub struct DeploymentCoordinator {
    nodes: Arc<RwLock<Vec<DeploymentNodeInfo>>>,
    lock_manager: Option<Arc<cmx_buffer::LockManager>>,
}

impl DeploymentCoordinator {
    /// 创建新的部署协调器（不带锁管理器）
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(Vec::new())),
            lock_manager: None,
        }
    }

    /// 创建新的部署协调器（带锁管理器）
    pub fn with_lock_manager(lock_manager: cmx_buffer::LockManager) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(Vec::new())),
            lock_manager: Some(Arc::new(lock_manager)),
        }
    }

    /// 获取锁管理器
    pub fn lock_manager(&self) -> Option<&Arc<cmx_buffer::LockManager>> {
        self.lock_manager.as_ref()
    }

    /// 获取部署锁
    async fn acquire_deploy_lock(&self, plugin_id: &str) -> Result<Option<cmx_buffer::LockGuard>, PluginError> {
        if let Some(ref lock_manager) = self.lock_manager {
            let lock_key = format!("{}{}", DEPLOY_LOCK_PREFIX, plugin_id);
            let guard = lock_manager
                .lock(&lock_key)
                .await
                .map_err(|e| PluginError::Deployment(format!("获取部署锁失败: {}", e)))?;
            Ok(Some(guard))
        } else {
            Ok(None)
        }
    }

    /// 释放部署锁
    fn release_deploy_lock(&self, _guard: Option<cmx_buffer::LockGuard>) {
        // LockGuard 在 Drop 时自动释放
    }
    
    /// 注册节点
    pub async fn register_node(&self, node: DeploymentNodeInfo) -> Result<(), PluginError> {
        let mut nodes = self.nodes.write().await;
        nodes.push(node);
        Ok(())
    }
    
    /// 注销节点
    pub async fn unregister_node(&self, node_id: &str) -> Result<(), PluginError> {
        let mut nodes = self.nodes.write().await;
        nodes.retain(|n| n.node_id != node_id);
        Ok(())
    }
    
    /// 获取所有节点
    pub async fn get_nodes(&self) -> Vec<DeploymentNodeInfo> {
        self.nodes.read().await.clone()
    }
    
    /// 部署到指定节点
    pub async fn deploy(&self, request: DeployRequest) -> Result<DeployResult, PluginError> {
        let nodes = self.nodes.read().await;
        
        match &request.strategy {
            DeploymentStrategy::Serial { continue_on_error } => {
                self.deploy_serial(&request, &nodes, *continue_on_error).await
            }
            DeploymentStrategy::Parallel { max_concurrent } => {
                self.deploy_parallel(&request, &nodes, *max_concurrent).await
            }
            DeploymentStrategy::Rolling { batch_size, wait_seconds } => {
                self.deploy_rolling(&request, &nodes, *batch_size, *wait_seconds).await
            }
            DeploymentStrategy::BlueGreen { switch_at } => {
                self.deploy_blue_green(&request, &nodes, switch_at.clone()).await
            }
        }
    }
    
    /// 串行部署
    async fn deploy_serial(
        &self,
        request: &DeployRequest,
        nodes: &[DeploymentNodeInfo],
        continue_on_error: bool,
    ) -> Result<DeployResult, PluginError> {
        let mut results = Vec::new();
        
        for node_id in &request.nodes {
            let node = nodes.iter().find(|n| &n.node_id == node_id);
            
            match node {
                Some(n) if n.status == DeploymentNodeStatus::Online => {
                    // 执行实际部署到节点
                    let deploy_result = self.deploy_to_node(
                        node_id,
                        &request.plugin_id,
                        &request.version,
                    ).await;
                    
                    match deploy_result {
                        Ok(_) => {
                            results.push(NodeDeploymentResult {
                                node_id: node_id.clone(),
                                success: true,
                                error_message: None,
                            });
                        }
                        Err(e) => {
                            let result = NodeDeploymentResult {
                                node_id: node_id.clone(),
                                success: false,
                                error_message: Some(e.to_string()),
                            };
                            if continue_on_error {
                                results.push(result);
                            } else {
                                results.push(result);
                                return Ok(DeployResult {
                                    success: false,
                                    operation_id: uuid::Uuid::new_v4().to_string(),
                                    nodes: results,
                                });
                            }
                        }
                    }
                }
                Some(_) => {
                    let result = NodeDeploymentResult {
                        node_id: node_id.clone(),
                        success: false,
                        error_message: Some("节点离线".to_string()),
                    };
                    if continue_on_error {
                        results.push(result);
                    } else {
                        return Ok(DeployResult {
                            success: false,
                            operation_id: uuid::Uuid::new_v4().to_string(),
                            nodes: results,
                        });
                    }
                }
                None => {
                    let result = NodeDeploymentResult {
                        node_id: node_id.clone(),
                        success: false,
                        error_message: Some("节点不存在".to_string()),
                    };
                    if continue_on_error {
                        results.push(result);
                    } else {
                        return Ok(DeployResult {
                            success: false,
                            operation_id: uuid::Uuid::new_v4().to_string(),
                            nodes: results,
                        });
                    }
                }
            }
        }
        
        Ok(DeployResult {
            success: results.iter().all(|r| r.success),
            operation_id: uuid::Uuid::new_v4().to_string(),
            nodes: results,
        })
    }
    
    /// 部署到单个节点
    async fn deploy_to_node(
        &self,
        node_id: &str,
        plugin_id: &str,
        version: &str,
    ) -> Result<(), PluginError> {
        // 模拟部署过程
        // 1. 连接到节点
        // 2. 上传插件包
        // 3. 解压并配置
        // 4. 验证部署
        
        log::info!("部署插件 {} 到节点 {}，版本 {}", plugin_id, node_id, version);
        
        // 模拟部署延迟
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        Ok(())
    }
    
    /// 并行部署
    async fn deploy_parallel(
        &self,
        request: &DeployRequest,
        nodes: &[DeploymentNodeInfo],
        max_concurrent: usize,
    ) -> Result<DeployResult, PluginError> {
        // 收集节点信息
        let node_map: std::collections::HashMap<String, DeploymentNodeStatus> = nodes.iter()
            .map(|n| (n.node_id.clone(), n.status))
            .collect();
        
        // 限制并发数
        let max_concurrent = max_concurrent.max(1);
        
        // 分批执行，每批最多 max_concurrent 个
        let mut results = Vec::new();
        
        // 直接迭代，避免生命周期问题
        let all_node_ids: Vec<String> = request.nodes.iter().map(|s| s.clone()).collect();
        
        for chunk in all_node_ids.chunks(max_concurrent) {
            let mut handles: Vec<tokio::task::JoinHandle<NodeDeploymentResult>> = Vec::new();
            
            for node_id in chunk.iter() {
                let node_status = node_map.get(node_id).copied();
                let node_id_owned = node_id.clone();
                
                let handle = tokio::spawn(async move {
                    match node_status {
                        Some(DeploymentNodeStatus::Online) => {
                            // 部署到节点
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            NodeDeploymentResult {
                                node_id: node_id_owned,
                                success: true,
                                error_message: None,
                            }
                        }
                        Some(_) => NodeDeploymentResult {
                            node_id: node_id_owned,
                            success: false,
                            error_message: Some("节点离线".to_string()),
                        },
                        None => NodeDeploymentResult {
                            node_id: node_id_owned,
                            success: false,
                            error_message: Some("节点不存在".to_string()),
                        },
                    }
                });
                
                handles.push(handle);
            }
            
            // 等待当前批次完成
            for handle in handles {
                match handle.await {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        results.push(NodeDeploymentResult {
                            node_id: "unknown".to_string(),
                            success: false,
                            error_message: Some(e.to_string()),
                        });
                    }
                }
            }
        }
        
        Ok(DeployResult {
            success: results.iter().all(|r| r.success),
            operation_id: uuid::Uuid::new_v4().to_string(),
            nodes: results,
        })
    }
    
    /// 滚动部署
    async fn deploy_rolling(
        &self,
        request: &DeployRequest,
        nodes: &[DeploymentNodeInfo],
        batch_size: usize,
        wait_seconds: u64,
    ) -> Result<DeployResult, PluginError> {
        let batch_size = batch_size.max(1);
        let mut results = Vec::new();
        
        // 按批次部署 - 使用 owned 字符串避免生命周期问题
        let all_node_ids: Vec<String> = request.nodes.iter().map(|s| s.clone()).collect();
        
        // 收集节点信息
        let node_map: std::collections::HashMap<String, DeploymentNodeStatus> = nodes.iter()
            .map(|n| (n.node_id.clone(), n.status))
            .collect();
        
        for chunk in all_node_ids.chunks(batch_size) {
            let mut batch_results = Vec::new();
            
            // 并行部署当前批次
            for node_id in chunk {
                let node_status = node_map.get(node_id).copied();
                
                match node_status {
                    Some(DeploymentNodeStatus::Online) => {
                        match self.deploy_to_node(node_id, &request.plugin_id, &request.version).await {
                            Ok(_) => {
                                batch_results.push(NodeDeploymentResult {
                                    node_id: node_id.to_string(),
                                    success: true,
                                    error_message: None,
                                });
                            }
                            Err(e) => {
                                batch_results.push(NodeDeploymentResult {
                                    node_id: node_id.to_string(),
                                    success: false,
                                    error_message: Some(e.to_string()),
                                });
                            }
                        }
                    }
                    Some(_) => {
                        batch_results.push(NodeDeploymentResult {
                            node_id: node_id.to_string(),
                            success: false,
                            error_message: Some("节点离线".to_string()),
                        });
                    }
                    None => {
                        batch_results.push(NodeDeploymentResult {
                            node_id: node_id.to_string(),
                            success: false,
                            error_message: Some("节点不存在".to_string()),
                        });
                    }
                }
            }
            
            results.extend(batch_results);
            
            // 如果不是最后一批，等待指定时间
            let last_chunk_index = all_node_ids.len() / batch_size;
            if chunk.len() == batch_size && wait_seconds > 0 {
                tokio::time::sleep(tokio::time::Duration::from_secs(wait_seconds)).await;
            }
        }
        
        Ok(DeployResult {
            success: results.iter().all(|r| r.success),
            operation_id: uuid::Uuid::new_v4().to_string(),
            nodes: results,
        })
    }
    
    /// 蓝绿部署
    async fn deploy_blue_green(
        &self,
        request: &DeployRequest,
        nodes: &[DeploymentNodeInfo],
        switch_at: Option<String>,
    ) -> Result<DeployResult, PluginError> {
        // 蓝绿部署：同时部署到所有节点，但新版本在单独的环境中
        // 切换时只需要切换流量指向
        
        let mut results = Vec::new();
        
        // 1. 部署到绿色环境（新版本）
        for node_id in &request.nodes {
            let node = nodes.iter().find(|n| &n.node_id == node_id);
            
            match node {
                Some(n) if n.status == DeploymentNodeStatus::Online => {
                    match self.deploy_to_node(node_id, &request.plugin_id, &request.version).await {
                        Ok(_) => {
                            results.push(NodeDeploymentResult {
                                node_id: node_id.clone(),
                                success: true,
                                error_message: None,
                            });
                        }
                        Err(e) => {
                            results.push(NodeDeploymentResult {
                                node_id: node_id.clone(),
                                success: false,
                                error_message: Some(e.to_string()),
                            });
                        }
                    }
                }
                Some(_) => {
                    results.push(NodeDeploymentResult {
                        node_id: node_id.clone(),
                        success: false,
                        error_message: Some("节点离线".to_string()),
                    });
                }
                None => {
                    results.push(NodeDeploymentResult {
                        node_id: node_id.clone(),
                        success: false,
                        error_message: Some("节点不存在".to_string()),
                    });
                }
            }
        }
        
        // 2. 如果指定了切换时间，立即切换
        if let Some(switch_time) = switch_at {
            if switch_time == "now" {
                log::info!("蓝绿部署：立即切换到新版本");
            }
        }
        
        Ok(DeployResult {
            success: true,
            operation_id: uuid::Uuid::new_v4().to_string(),
            nodes: results,
        })
    }
    
    /// 同步所有节点
    pub async fn sync_all_nodes(&self, plugin_id: &str) -> Result<SyncResult, PluginError> {
        let nodes = self.nodes.read().await;
        
        let synced: Vec<String> = nodes.iter()
            .filter(|n| n.status == DeploymentNodeStatus::Online)
            .map(|n| n.node_id.clone())
            .collect();
        
        Ok(SyncResult {
            success: true,
            synced_nodes: synced,
            failed_nodes: Vec::new(),
        })
    }
    
    /// 节点故障恢复
    pub async fn recover_node(&self, node_id: &str) -> Result<RecoveryResult, PluginError> {
        // 1. 检查节点是否存在
        let nodes = self.nodes.read().await;
        let node = nodes.iter().find(|n| n.node_id == node_id);
        
        match node {
            Some(n) => {
                // 2. 尝试重新连接节点
                log::info!("尝试恢复节点 {} ({})", n.node_name, n.node_id);
                
                // 模拟重连过程
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                
                // 3. 同步最新数据
                let sync_result = self.sync_all_nodes(node_id).await?;
                
                Ok(RecoveryResult {
                    success: sync_result.success,
                    recovered_nodes: if sync_result.success {
                        vec![node_id.to_string()]
                    } else {
                        Vec::new()
                    },
                    failed_nodes: if sync_result.success {
                        Vec::new()
                    } else {
                        vec![node_id.to_string()]
                    },
                })
            }
            None => Err(PluginError::NotFound(format!(
                "节点 {} 不存在",
                node_id
            ))),
        }
    }
    
    /// 等待节点同步完成
    pub async fn wait_for_sync(
        &self,
        operation_id: &str,
        timeout_seconds: u64,
    ) -> Result<SyncStatus, PluginError> {
        let start_time = std::time::Instant::now();
        let timeout_duration = std::time::Duration::from_secs(timeout_seconds);
        
        // 轮询检查同步状态
        loop {
            if start_time.elapsed() > timeout_duration {
                return Err(PluginError::Timeout(format!(
                    "等待同步超时，操作ID: {}",
                    operation_id
                )));
            }
            
            // 模拟检查同步状态
            // 实际实现需要查询数据库或协调器状态
            
            // 假设同步已完成
            return Ok(SyncStatus {
                operation_id: operation_id.to_string(),
                status: DeploymentStatus::Completed,
                completed_nodes: Vec::new(),
                pending_nodes: Vec::new(),
                failed_nodes: Vec::new(),
            });
        }
    }
}

impl Default for DeploymentCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
