//! 通用事件总线模块
//!
//! 提供跨模块的事件发布订阅功能。
//!
//! # 设计目标
//!
//! - **通用性**：支持任意事件类型，不限于特定业务场景
//! - **类型安全**：事件载荷使用 JSON，可序列化任意数据结构
//! - **高性能**：异步处理，不阻塞发布者
//! - **易扩展**：事件类型使用字符串标识，方便新增事件类型
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use cmx_traits::event_bus::{GlobalEventBus, EventHandler};
//! use serde_json::json;
//!
//! // 初始化（应用启动时调用一次）
//! GlobalEventBus::initialize().unwrap();
//!
//! // 订阅事件
//! let handler: EventHandler = Arc::new(|topic, payload| {
//!     println!("收到事件: {} -> {:?}", topic, payload);
//! });
//! GlobalEventBus::get().subscribe("plugin.installed", handler).await;
//!
//! // 发布事件
//! GlobalEventBus::get().publish("plugin.installed", json!({
//!     "plugin_id": "my-plugin",
//!     "version": "1.0.0",
//! })).await;
//! ```

mod bus;
mod global;
mod types;

pub use bus::EventBus;
pub use global::GlobalEventBus;
pub use types::{EventTopic, EventPayload, EventHandler};
