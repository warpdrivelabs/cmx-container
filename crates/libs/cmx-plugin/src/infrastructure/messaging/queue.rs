//! 消息队列模块
//! 
//! 提供消息队列功能

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 消息
#[derive(Debug, Clone)]
pub struct Message {
    /// 消息ID
    pub id: String,
    /// 消息类型
    pub msg_type: String,
    /// 消息数据
    pub data: serde_json::Value,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Message {
    /// 创建新消息
    pub fn new(msg_type: String, data: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            msg_type,
            data,
            created_at: chrono::Utc::now(),
        }
    }
}

/// 消息队列
pub struct MessageQueue {
    /// 队列名称
    name: String,
    /// 队列数据
    queue: Arc<Mutex<VecDeque<Message>>>,
    /// 最大长度
    max_length: usize,
}

impl MessageQueue {
    /// 创建新的消息队列
    pub fn new(name: String, max_length: usize) -> Self {
        Self {
            name,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            max_length,
        }
    }
    
    /// 获取队列名称
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// 入队
    pub async fn enqueue(&self, message: Message) -> Result<(), String> {
        let mut queue = self.queue.lock().await;
        
        if queue.len() >= self.max_length {
            return Err("队列已满".to_string());
        }
        
        queue.push_back(message);
        Ok(())
    }
    
    /// 出队
    pub async fn dequeue(&self) -> Option<Message> {
        let mut queue = self.queue.lock().await;
        queue.pop_front()
    }
    
    /// 查看队首
    pub async fn peek(&self) -> Option<Message> {
        let queue = self.queue.lock().await;
        queue.front().cloned()
    }
    
    /// 获取队列长度
    pub async fn len(&self) -> usize {
        let queue = self.queue.lock().await;
        queue.len()
    }
    
    /// 检查队列是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
    
    /// 清空队列
    pub async fn clear(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();
    }
}

/// 消息队列管理器
pub struct MessageQueueManager {
    /// 队列列表
    queues: Arc<RwLock<std::collections::HashMap<String, Arc<MessageQueue>>>>,
}

impl MessageQueueManager {
    /// 创建新的消息队列管理器
    pub fn new() -> Self {
        Self {
            queues: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// 创建或获取队列
    pub async fn get_or_create(&self, name: &str, max_length: usize) -> Arc<MessageQueue> {
        let mut queues = self.queues.write().await;
        
        queues.entry(name.to_string())
            .or_insert_with(|| Arc::new(MessageQueue::new(name.to_string(), max_length)))
            .clone()
    }
    
    /// 获取队列
    pub async fn get(&self, name: &str) -> Option<Arc<MessageQueue>> {
        let queues = self.queues.read().await;
        queues.get(name).cloned()
    }
    
    /// 删除队列
    pub async fn remove(&self, name: &str) -> Option<Arc<MessageQueue>> {
        let mut queues = self.queues.write().await;
        queues.remove(name)
    }
}

impl Default for MessageQueueManager {
    fn default() -> Self {
        Self::new()
    }
}
