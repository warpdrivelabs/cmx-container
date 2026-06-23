//! 事件总线核心实现。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::{EventTopic, EventPayload, EventHandler};

/// 事件总线。
///
/// 提供发布-订阅模式的事件分发功能。
///
/// # 特性
///
/// - **通用性**：支持任意事件类型，使用字符串主题标识。
/// - **类型安全**：事件载荷使用 JSON，可序列化任意数据结构。
/// - **高性能**：异步处理，不阻塞发布者。
/// - **线程安全**：使用 `Arc<RwLock>` 保证并发安全。
pub struct EventBus {
    /// 事件处理器映射（主题 -> 处理器列表）。
    handlers: Arc<RwLock<HashMap<EventTopic, Vec<EventHandler>>>>,
}

impl EventBus {
    /// 创建新的事件总线。
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布事件（异步，不等待处理器完成）。
    ///
    /// 事件会被分发给所有订阅该主题的处理器。
    /// 每个处理器在独立的 tokio 任务中执行，不阻塞发布者。
    ///
    /// # Arguments
    ///
    /// * `topic` - 事件主题。
    /// * `payload` - 事件载荷。
    pub async fn publish(&self, topic: impl Into<EventTopic>, payload: impl Into<EventPayload>) {
        let topic = topic.into();
        let payload = payload.into();

        let handlers = self.handlers.read().await;
        if let Some(handlers) = handlers.get(&topic) {
            tracing::debug!("发布事件: {}，订阅者数量: {}", topic, handlers.len());
            for handler in handlers {
                let handler = handler.clone();
                let topic = topic.clone();
                let payload = payload.clone();
                tokio::spawn(async move {
                    handler(topic, payload);
                });
            }
        } else {
            tracing::trace!("发布事件: {}，无订阅者", topic);
        }
    }

    /// 发布事件（同步，等待所有处理器完成）。
    ///
    /// 与 [`Self::publish`] 不同，此方法会等待所有处理器执行完成。
    /// 适用于需要确保事件处理完成的场景。
    ///
    /// # Arguments
    ///
    /// * `topic` - 事件主题。
    /// * `payload` - 事件载荷。
    pub async fn publish_sync(&self, topic: impl Into<EventTopic>, payload: impl Into<EventPayload>) {
        let topic = topic.into();
        let payload = payload.into();

        let handlers = self.handlers.read().await;
        if let Some(handlers) = handlers.get(&topic) {
            tracing::debug!("发布事件(同步): {}，订阅者数量: {}", topic, handlers.len());
            let mut tasks = Vec::new();
            for handler in handlers {
                let handler = handler.clone();
                let topic = topic.clone();
                let payload = payload.clone();
                tasks.push(tokio::spawn(async move {
                    handler(topic, payload);
                }));
            }
            // 等待所有任务完成
            for task in tasks {
                let _ = task.await;
            }
        }
    }

    /// 订阅事件。
    ///
    /// 注册一个处理器，当指定主题的事件发布时会被调用。
    ///
    /// # Arguments
    ///
    /// * `topic` - 事件主题。
    /// * `handler` - 事件处理器。
    pub async fn subscribe(&self, topic: impl Into<EventTopic>, handler: EventHandler) {
        let topic = topic.into();
        let mut handlers = self.handlers.write().await;
        handlers.entry(topic).or_insert_with(Vec::new).push(handler);
    }

    /// 取消订阅指定主题的所有处理器。
    ///
    /// # Arguments
    ///
    /// * `topic` - 事件主题。
    pub async fn unsubscribe_all(&self, topic: impl Into<EventTopic>) {
        let topic = topic.into();
        let mut handlers = self.handlers.write().await;
        handlers.remove(&topic);
    }

    /// 获取指定主题的订阅者数量。
    ///
    /// # Arguments
    ///
    /// * `topic` - 事件主题。
    ///
    /// # Returns
    ///
    /// 返回该主题的订阅者数量，无订阅者时返回 0。
    pub async fn subscriber_count(&self, topic: impl Into<EventTopic>) -> usize {
        let topic = topic.into();
        let handlers = self.handlers.read().await;
        handlers.get(&topic).map(|h| h.len()).unwrap_or(0)
    }

    /// 获取所有主题。
    ///
    /// # Returns
    ///
    /// 返回当前已注册的所有主题列表。
    pub async fn topics(&self) -> Vec<EventTopic> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
