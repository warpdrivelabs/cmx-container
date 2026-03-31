//! 状态同步模块
//! 
//! 同步插件状态到各节点

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::status::PluginStatus;

/// 插件状态记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStateRecord {
    /// 插件ID
    pub plugin_id: String,
    /// 版本
    pub version: String,
    /// 状态
    pub status: PluginStatus,
    /// 节点ID
    pub node_id: String,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

/// 同步消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// 消息类型
    pub msg_type: SyncMessageType,
    /// 状态记录
    pub record: Option<PluginStateRecord>,
    /// 节点ID
    pub source_node_id: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// 同步消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessageType {
    /// 状态更新
    StateUpdate,
    /// 状态删除
    StateDelete,
    /// 全量同步请求
    FullSyncRequest,
    /// 全量同步响应
    FullSyncResponse,
}

/// 状态同步管理器
/// 
/// 使用 Redis Pub/Sub 实现跨节点状态同步。
pub struct SyncManager {
    /// 本地节点ID
    local_node_id: String,
    /// 状态存储
    states: Arc<RwLock<HashMap<String, PluginStateRecord>>>,
    /// Redis Pub/Sub（可选）
    pubsub: Option<Arc<cmx_buffer::PubSubOps>>,
    /// 同步通道名称
    sync_channel: String,
}

impl SyncManager {
    /// 创建新的状态同步管理器
    pub fn new(local_node_id: String) -> Self {
        Self {
            local_node_id,
            states: Arc::new(RwLock::new(HashMap::new())),
            pubsub: None,
            sync_channel: "cmx:plugin:sync".to_string(),
        }
    }
    
    /// 设置 Redis Pub/Sub
    pub fn with_pubsub(mut self, pubsub: Arc<cmx_buffer::PubSubOps>) -> Self {
        self.pubsub = Some(pubsub);
        self
    }
    
    /// 更新本地状态
    /// 
    /// 更新本地状态并同步到远程节点。
    pub async fn update_local_state(
        &self,
        plugin_id: String,
        version: String,
        status: PluginStatus,
    ) {
        let record = PluginStateRecord {
            plugin_id: plugin_id.clone(),
            version,
            status,
            node_id: self.local_node_id.clone(),
            updated_at: Utc::now(),
        };
        
        // 更新本地状态
        {
            let mut states = self.states.write().await;
            states.insert(plugin_id.clone(), record.clone());
        }
        
        // 同步到远程节点
        let _ = self.sync_to_remote(&plugin_id).await;
    }
    
    /// 获取本地状态
    pub async fn get_local_state(&self, plugin_id: &str) -> Option<PluginStateRecord> {
        let states = self.states.read().await;
        states.get(plugin_id).cloned()
    }
    
    /// 获取所有本地状态
    pub async fn get_all_local_states(&self) -> Vec<PluginStateRecord> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }
    
    /// 同步状态到远程节点
    /// 
    /// 通过 Redis Pub/Sub 发布状态更新消息。
    pub async fn sync_to_remote(&self, plugin_id: &str) -> Result<(), String> {
        let states = self.states.read().await;
        let record = states.get(plugin_id).cloned();
        drop(states);
        
        if let Some(ref pubsub) = self.pubsub {
            let message = SyncMessage {
                msg_type: SyncMessageType::StateUpdate,
                record,
                source_node_id: self.local_node_id.clone(),
                timestamp: Utc::now(),
            };
            
            let message_json = serde_json::to_string(&message)
                .map_err(|e| format!("序列化消息失败: {}", e))?;
            
            pubsub.publish(&self.sync_channel, &message_json).await
                .map_err(|e| format!("发布消息失败: {}", e))?;
            
            tracing::info!("已同步插件状态到远程节点: {}", plugin_id);
        }
        
        Ok(())
    }
    
    /// 从远程节点同步状态
    /// 
    /// 请求全量同步或订阅远程更新。
    pub async fn sync_from_remote(&self) -> Result<Vec<PluginStateRecord>, String> {
        if let Some(ref pubsub) = self.pubsub {
            // 发布全量同步请求
            let message = SyncMessage {
                msg_type: SyncMessageType::FullSyncRequest,
                record: None,
                source_node_id: self.local_node_id.clone(),
                timestamp: Utc::now(),
            };
            
            let message_json = serde_json::to_string(&message)
                .map_err(|e| format!("序列化消息失败: {}", e))?;
            
            pubsub.publish(&self.sync_channel, &message_json).await
                .map_err(|e| format!("发布消息失败: {}", e))?;
            
            tracing::info!("已请求全量同步");
        }
        
        // 返回当前本地状态
        Ok(self.get_all_local_states().await)
    }
    
    /// 处理远程同步消息
    /// 
    /// 处理从 Redis Pub/Sub 接收到的消息。
    pub async fn handle_sync_message(&self, message: &str) -> Result<(), String> {
        let sync_msg: SyncMessage = serde_json::from_str(message)
            .map_err(|e| format!("解析消息失败: {}", e))?;
        
        // 忽略自己发送的消息
        if sync_msg.source_node_id == self.local_node_id {
            return Ok(());
        }
        
        match sync_msg.msg_type {
            SyncMessageType::StateUpdate => {
                if let Some(record) = sync_msg.record {
                    let mut states = self.states.write().await;
                    states.insert(record.plugin_id.clone(), record.clone());
                    tracing::info!("已从远程节点同步状态: {} ({})", record.plugin_id, record.status);
                }
            }
            SyncMessageType::StateDelete => {
                if let Some(record) = sync_msg.record {
                    let mut states = self.states.write().await;
                    states.remove(&record.plugin_id);
                    tracing::info!("已从远程节点删除状态: {}", record.plugin_id);
                }
            }
            SyncMessageType::FullSyncRequest => {
                // 响应全量同步请求
                let _ = self.respond_full_sync().await;
            }
            SyncMessageType::FullSyncResponse => {
                // 处理全量同步响应（已通过其他机制处理）
            }
        }
        
        Ok(())
    }
    
    /// 响应全量同步请求
    async fn respond_full_sync(&self) -> Result<(), String> {
        if let Some(ref pubsub) = self.pubsub {
            let states = self.get_all_local_states().await;
            
            for record in states {
                let message = SyncMessage {
                    msg_type: SyncMessageType::StateUpdate,
                    record: Some(record),
                    source_node_id: self.local_node_id.clone(),
                    timestamp: Utc::now(),
                };
                
                let message_json = serde_json::to_string(&message)
                    .map_err(|e| format!("序列化消息失败: {}", e))?;
                
                pubsub.publish(&self.sync_channel, &message_json).await
                    .map_err(|e| format!("发布消息失败: {}", e))?;
            }
            
            tracing::info!("已响应全量同步请求");
        }
        
        Ok(())
    }
    
    /// 删除状态
    /// 
    /// 删除本地状态并同步到远程节点。
    pub async fn remove_state(&self, plugin_id: &str) {
        // 删除本地状态
        {
            let mut states = self.states.write().await;
            states.remove(plugin_id);
        }
        
        // 同步删除到远程节点
        if let Some(ref pubsub) = self.pubsub {
            let message = SyncMessage {
                msg_type: SyncMessageType::StateDelete,
                record: Some(PluginStateRecord {
                    plugin_id: plugin_id.to_string(),
                    version: String::new(),
                    status: PluginStatus::Installed,
                    node_id: self.local_node_id.clone(),
                    updated_at: Utc::now(),
                }),
                source_node_id: self.local_node_id.clone(),
                timestamp: Utc::now(),
            };
            
            if let Ok(message_json) = serde_json::to_string(&message) {
                let _ = pubsub.publish(&self.sync_channel, &message_json).await;
            }
        }
        
        tracing::info!("已删除插件状态: {}", plugin_id);
    }
    
    /// 获取节点ID
    pub fn node_id(&self) -> &str {
        &self.local_node_id
    }
}
