//! 事件发布模块
//! 
//! 提供事件发布订阅功能

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 事件类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// 系统启动
    SystemStarted,
    /// 系统停止
    SystemStopped,
    /// 插件已安装
    PluginInstalled,
    /// 插件已卸载
    PluginUninstalled,
    /// 插件已激活
    PluginActivated,
    /// 插件已停用
    PluginDeactivated,
    /// 插件已升级
    PluginUpgraded,
    /// 插件已降级
    PluginDowngraded,
    /// 插件错误
    PluginError,
    /// 自定义事件
    Custom(String),
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::SystemStarted => write!(f, "system.started"),
            EventType::SystemStopped => write!(f, "system.stopped"),
            EventType::PluginInstalled => write!(f, "plugin.installed"),
            EventType::PluginUninstalled => write!(f, "plugin.uninstalled"),
            EventType::PluginActivated => write!(f, "plugin.activated"),
            EventType::PluginDeactivated => write!(f, "plugin.deactivated"),
            EventType::PluginUpgraded => write!(f, "plugin.upgraded"),
            EventType::PluginDowngraded => write!(f, "plugin.downgraded"),
            EventType::PluginError => write!(f, "plugin.error"),
            EventType::Custom(name) => write!(f, "custom.{}", name),
        }
    }
}

/// 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 事件ID
    pub id: String,
    /// 事件类型
    pub event_type: EventType,
    /// 插件ID
    pub plugin_id: String,
    /// 事件数据
    pub data: serde_json::Value,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

impl Event {
    /// 创建新事件
    pub fn new(event_type: EventType, plugin_id: String, data: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type,
            plugin_id,
            data,
            timestamp: Utc::now(),
        }
    }
}

/// 事件处理器
pub type EventHandler = Box<dyn Fn(Event) + Send + Sync>;

/// 事件总线
pub struct EventBus {
    /// 事件处理器列表
    handlers: Arc<RwLock<HashMap<String, Vec<EventHandler>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 发布事件
    pub async fn publish(&self, event: Event) {
        let event_type = event.event_type.to_string();
        let handlers = self.handlers.read().await;
        
        if let Some(handlers) = handlers.get(&event_type) {
            for handler in handlers {
                handler(event.clone());
            }
        }
    }
    
    /// 订阅事件
    pub async fn subscribe(&self, event_type: &EventType, handler: EventHandler) {
        let event_type_str = event_type.to_string();
        let mut handlers = self.handlers.write().await;
        
        handlers
            .entry(event_type_str)
            .or_insert_with(Vec::new)
            .push(handler);
    }
    
    /// 取消订阅
    pub async fn unsubscribe_all(&self, event_type: &EventType) {
        let event_type_str = event_type.to_string();
        let mut handlers = self.handlers.write().await;
        handlers.remove(&event_type_str);
    }
    
    /// 获取订阅者数量
    pub async fn subscriber_count(&self, event_type: &EventType) -> usize {
        let event_type_str = event_type.to_string();
        let handlers = self.handlers.read().await;
        handlers.get(&event_type_str).map(|h| h.len()).unwrap_or(0)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
