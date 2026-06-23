//! 事件总线类型定义。

/// 事件主题（字符串标识）。
pub type EventTopic = String;

/// 事件载荷（JSON 格式）。
pub type EventPayload = serde_json::Value;

/// 事件处理器。
pub type EventHandler = std::sync::Arc<dyn Fn(EventTopic, EventPayload) + Send + Sync>;
