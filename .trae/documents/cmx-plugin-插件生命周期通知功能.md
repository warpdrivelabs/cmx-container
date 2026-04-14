# 插件生命周期通知功能实现方案

## 一、需求分析

### 1.1 功能需求
1. **cmx-traits/lifecycle.rs**：
   - 新增 `on_plugin_installed`（安装完成）事件
   - 新增 `on_plugin_upgraded`（升级完成）事件
   - 注释掉 `on_plugin_activated` 和 `on_plugin_deactivated`（暂无此功能）

2. **cmx-plugin**：
   - 在插件安装、升级、卸载时发送生命周期通知
   - 迁移现有 EventBus 到 cmx-core，使用全局 EventBus

3. **cmx-service**：
   - 监听插件生命周期事件
   - 同步新增、卸载、更新服务缓存的服务数据

4. **cmx-runtime**：
   - 监听插件生命周期事件
   - 在插件升级、卸载时清除 Extism Plugin 实例缓存
   - 下次调用时重新初始化 WASM 模块

### 1.2 实现方式选择

#### 方案对比

| 方案 | 优点 | 缺点 |
|------|------|------|
| **方案A：web-server 统一注册** | 简单直接，容易理解 | 耦合度高，web-server 需要知道所有监听器 |
| **方案B：全局 EventBus** | 解耦彻底，扩展性好 | 需要在 cmx-core 新增模块 |

#### 推荐方案：通用全局 EventBus

**理由**：
1. cmx-plugin 已有 EventBus 实现，可迁移到 cmx-core 作为全局组件
2. 解耦彻底：cmx-plugin 和 cmx-service/cmx-runtime 不直接依赖
3. 扩展性好：设计通用 EventBus，支持任意事件类型
4. 符合事件驱动架构设计原则

## 二、架构设计

### 2.1 模块依赖关系

```
cmx-core (通用全局 EventBus)
    ↑
    ├── cmx-traits (定义 LifecycleEvent, PluginLifecycleListener)
    │
    ├── cmx-plugin (发布事件，迁移原有 EventBus 到 cmx-core)
    │
    ├── cmx-service (订阅事件，同步服务缓存)
    │
    └── cmx-runtime (订阅事件，清除 WASM 实例缓存)
```

### 2.2 通用 EventBus 设计

#### 2.2.1 设计原则

1. **通用性**：支持任意事件类型，不限于插件生命周期
2. **类型安全**：事件载荷使用 `serde_json::Value`，可序列化任意数据结构
3. **高性能**：使用 `Arc<RwLock>` 保证线程安全，异步处理不阻塞发布者
4. **易扩展**：事件类型使用字符串标识，方便新增事件类型

#### 2.2.2 核心 API

```rust
// cmx-core/src/event_bus/mod.rs

/// 事件类型（字符串标识，如 "plugin.installed", "cache.invalidated"）
pub type EventTopic = String;

/// 事件载荷（JSON 格式，支持任意数据结构）
pub type EventPayload = serde_json::Value;

/// 事件处理器
pub type EventHandler = Arc<dyn Fn(EventTopic, EventPayload) + Send + Sync>;

/// 全局事件总线
pub struct GlobalEventBus;

impl GlobalEventBus {
    /// 初始化全局事件总线
    pub fn initialize() -> Result<(), String>;
    
    /// 获取事件总线引用
    pub fn get() -> &'static EventBus;
}

/// 事件总线
pub struct EventBus {
    handlers: Arc<RwLock<HashMap<EventTopic, Vec<EventHandler>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self;
    
    /// 发布事件（异步，不等待处理器完成）
    pub async fn publish(&self, topic: impl Into<EventTopic>, payload: impl Into<EventPayload>);
    
    /// 发布事件（同步，等待所有处理器完成）
    pub async fn publish_sync(&self, topic: impl Into<EventTopic>, payload: impl Into<EventPayload>);
    
    /// 订阅事件
    pub async fn subscribe(&self, topic: impl Into<EventTopic>, handler: EventHandler);
    
    /// 取消订阅
    pub async fn unsubscribe_all(&self, topic: impl Into<EventTopic>);
    
    /// 获取订阅者数量
    pub async fn subscriber_count(&self, topic: impl Into<EventTopic>) -> usize;
}
```

#### 2.2.3 事件类型定义规范

```
{模块}.{动作}

示例：
- plugin.installed     - 插件已安装
- plugin.upgraded      - 插件已升级
- plugin.uninstalled   - 插件已卸载
- cache.invalidated    - 缓存已失效
- database.connected   - 数据库已连接
- system.started       - 系统已启动
```

### 2.3 生命周期事件定义

#### 2.3.1 事件类型常量（cmx-traits/src/lifecycle.rs）

```rust
/// 插件生命周期事件类型
pub mod plugin_events {
    /// 插件已安装
    pub const INSTALLED: &str = "plugin.installed";
    /// 插件已升级
    pub const UPGRADED: &str = "plugin.upgraded";
    /// 插件已卸载
    pub const UNINSTALLED: &str = "plugin.uninstalled";
    // 暂时注释
    // pub const ACTIVATED: &str = "plugin.activated";
    // pub const DEACTIVATED: &str = "plugin.deactivated";
}

/// 生命周期事件载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLifecyclePayload {
    /// 插件ID
    pub plugin_id: String,
    /// 当前版本
    pub version: String,
    /// 旧版本（仅升级事件）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// WASM 文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_path: Option<String>,
    /// 安装路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    /// 事件时间戳
    pub timestamp: DateTime<Utc>,
}
```

#### 2.3.2 PluginLifecycleListener trait 更新

```rust
// cmx-traits/src/lifecycle.rs
#[async_trait]
pub trait PluginLifecycleListener: Send + Sync {
    /// 插件已安装
    async fn on_plugin_installed(&self, event: PluginLifecyclePayload);
    
    /// 插件已升级
    async fn on_plugin_upgraded(&self, event: PluginLifecyclePayload);
    
    /// 插件已卸载
    async fn on_plugin_uninstalled(&self, event: PluginLifecyclePayload);
    
    // 暂时注释
    // async fn on_plugin_activated(&self, event: PluginLifecyclePayload);
    // async fn on_plugin_deactivated(&self, event: PluginLifecyclePayload);
}
```

### 2.4 数据流

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  InstallService │     │ UpgradeService  │     │UninstallService │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         │ publish("plugin.      │ publish("plugin.      │ publish("plugin.
         │   installed", ...)    │   upgraded", ...)     │   uninstalled", ...)
         ▼                       ▼                       ▼
┌─────────────────────────────────────────────────────────────────┐
│              GlobalEventBus (cmx-core) - 通用事件总线           │
│                                                                 │
│  支持: plugin.*, cache.*, database.*, system.* 等任意事件类型  │
└─────────┬───────────────────────────────────────┬───────────────┘
          │                                       │
          │ dispatch by topic                     │
          ▼                                       ▼
┌─────────────────────────────┐   ┌─────────────────────────────┐
│ ServiceLifecycleListener    │   │ RuntimeLifecycleListener    │
│ (cmx-service)               │   │ (cmx-runtime)               │
│                             │   │                             │
│ 订阅: plugin.installed      │   │ 订阅: plugin.upgraded       │
│       plugin.upgraded       │   │       plugin.uninstalled    │
│       plugin.uninstalled    │   │                             │
└─────────────────────────────┘   └─────────────────────────────┘
```

### 2.5 WASM 实例缓存清理逻辑

| 事件 | 服务缓存操作 | WASM 实例缓存操作 |
|------|-------------|------------------|
| plugin.installed | 加载服务定义到缓存 | 无操作（新插件无缓存） |
| plugin.upgraded | 更新服务定义缓存 | 清除旧版本实例缓存 |
| plugin.uninstalled | 清理服务定义缓存 | 清除实例缓存 |

## 三、详细实现方案

### 3.1 cmx-core 模块修改

#### 新增目录：`cmx-core/src/event_bus/`

```
cmx-core/src/event_bus/
├── mod.rs           # 模块入口，导出公共 API
├── bus.rs           # EventBus 核心实现
├── global.rs        # GlobalEventBus 单例管理
└── types.rs         # 类型定义（EventTopic, EventPayload, EventHandler）
```

#### 新增文件：`cmx-core/src/event_bus/types.rs`

```rust
//! 事件总线类型定义

use serde::{Deserialize, Serialize};

/// 事件主题（字符串标识）
pub type EventTopic = String;

/// 事件载荷（JSON 格式）
pub type EventPayload = serde_json::Value;

/// 事件处理器
pub type EventHandler = std::sync::Arc<dyn Fn(EventTopic, EventPayload) + Send + Sync>;
```

#### 新增文件：`cmx-core/src/event_bus/bus.rs`

```rust
//! 事件总线核心实现

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

use super::types::{EventTopic, EventPayload, EventHandler};

/// 事件总线
///
/// 提供发布-订阅模式的事件分发功能。
///
/// # 特性
///
/// - **通用性**：支持任意事件类型，使用字符串主题标识
/// - **类型安全**：事件载荷使用 JSON，可序列化任意数据结构
/// - **高性能**：异步处理，不阻塞发布者
/// - **线程安全**：使用 `Arc<RwLock>` 保证并发安全
pub struct EventBus {
    /// 事件处理器映射（主题 -> 处理器列表）
    handlers: Arc<RwLock<HashMap<EventTopic, Vec<EventHandler>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 发布事件（异步，不等待处理器完成）
    ///
    /// 事件会被分发给所有订阅该主题的处理器。
    /// 每个处理器在独立的 tokio 任务中执行，不阻塞发布者。
    ///
    /// # 参数
    ///
    /// * `topic` - 事件主题
    /// * `payload` - 事件载荷
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

    /// 发布事件（同步，等待所有处理器完成）
    ///
    /// 与 `publish` 不同，此方法会等待所有处理器执行完成。
    /// 适用于需要确保事件处理完成的场景。
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

    /// 订阅事件
    ///
    /// 注册一个处理器，当指定主题的事件发布时会被调用。
    ///
    /// # 参数
    ///
    /// * `topic` - 事件主题
    /// * `handler` - 事件处理器
    pub async fn subscribe(&self, topic: impl Into<EventTopic>, handler: EventHandler) {
        let topic = topic.into();
        let mut handlers = self.handlers.write().await;
        handlers.entry(topic).or_insert_with(Vec::new).push(handler);
    }

    /// 取消订阅指定主题的所有处理器
    pub async fn unsubscribe_all(&self, topic: impl Into<EventTopic>) {
        let topic = topic.into();
        let mut handlers = self.handlers.write().await;
        handlers.remove(&topic);
    }

    /// 获取指定主题的订阅者数量
    pub async fn subscriber_count(&self, topic: impl Into<EventTopic>) -> usize {
        let topic = topic.into();
        let handlers = self.handlers.read().await;
        handlers.get(&topic).map(|h| h.len()).unwrap_or(0)
    }

    /// 获取所有主题
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
```

#### 新增文件：`cmx-core/src/event_bus/global.rs`

```rust
//! 全局事件总线单例管理

use std::sync::OnceLock;

use super::bus::EventBus;
use crate::error::CoreError;

/// 全局事件总线实例
static GLOBAL_EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

/// 全局事件总线访问器
///
/// 提供全局单例模式的 EventBus 访问。
/// 必须在应用启动时调用 `initialize()` 进行初始化。
pub struct GlobalEventBus;

impl GlobalEventBus {
    /// 初始化全局事件总线
    ///
    /// # 错误
    ///
    /// 如果全局事件总线已经初始化，返回错误。
    pub fn initialize() -> Result<(), CoreError> {
        GLOBAL_EVENT_BUS
            .set(EventBus::new())
            .map_err(|_| CoreError::AlreadyInitialized("全局事件总线已初始化".to_string()))
    }

    /// 获取全局事件总线引用
    ///
    /// # Panic
    ///
    /// 如果全局事件总线未初始化，将 panic。
    pub fn get() -> &'static EventBus {
        GLOBAL_EVENT_BUS
            .get()
            .expect("全局事件总线未初始化，请先调用 GlobalEventBus::initialize()")
    }

    /// 检查全局事件总线是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_EVENT_BUS.get().is_some()
    }
}
```

#### 新增文件：`cmx-core/src/event_bus/mod.rs`

```rust
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
//! ```rust
//! use cmx_core::{GlobalEventBus, EventHandler};
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
```

#### 新增文件：`cmx-core/src/error.rs`

```rust
//! 核心错误类型定义

use thiserror::Error;

/// 核心模块错误
#[derive(Debug, Error)]
pub enum CoreError {
    /// 已初始化
    #[error("{0}")]
    AlreadyInitialized(String),
}
```

#### 修改文件：`cmx-core/src/lib.rs`

```rust
pub mod error;
pub mod event_bus;
pub mod model;
pub mod wasm_types;

pub use error::CoreError;
pub use event_bus::{EventBus, GlobalEventBus, EventTopic, EventPayload, EventHandler};
pub use model::data::request::params::*;
pub use model::service::*;
pub use wasm_types::*;
```

### 3.2 cmx-traits 模块修改

#### 修改文件：`cmx-traits/src/lifecycle.rs`

```rust
//! 插件生命周期事件定义
//!
//! 定义插件生命周期事件的主题常量和载荷结构。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use async_trait::async_trait;

// ==================== 事件主题常量 ====================

/// 插件生命周期事件主题
pub mod plugin_events {
    /// 插件已安装
    pub const INSTALLED: &str = "plugin.installed";
    /// 插件已升级
    pub const UPGRADED: &str = "plugin.upgraded";
    /// 插件已卸载
    pub const UNINSTALLED: &str = "plugin.uninstalled";
    
    // 暂时注释，暂无此功能
    // /// 插件已激活
    // pub const ACTIVATED: &str = "plugin.activated";
    // /// 插件已停用
    // pub const DEACTIVATED: &str = "plugin.deactivated";
}

// ==================== 事件载荷 ====================

/// 插件生命周期事件载荷
///
/// 在插件生命周期变更时携带的事件数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLifecyclePayload {
    /// 插件ID
    pub plugin_id: String,
    /// 当前版本
    pub version: String,
    /// 旧版本（仅升级事件）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// WASM 文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_path: Option<String>,
    /// 安装路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    /// 事件时间戳
    pub timestamp: DateTime<Utc>,
}

impl PluginLifecyclePayload {
    /// 创建新的生命周期事件载荷
    pub fn new(plugin_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            version: version.into(),
            old_version: None,
            wasm_path: None,
            install_path: None,
            timestamp: Utc::now(),
        }
    }

    /// 设置旧版本（用于升级事件）
    pub fn with_old_version(mut self, old_version: impl Into<String>) -> Self {
        self.old_version = Some(old_version.into());
        self
    }

    /// 设置 WASM 文件路径
    pub fn with_wasm_path(mut self, path: PathBuf) -> Self {
        self.wasm_path = Some(path.to_string_lossy().to_string());
        self
    }

    /// 设置安装路径
    pub fn with_install_path(mut self, path: PathBuf) -> Self {
        self.install_path = Some(path.to_string_lossy().to_string());
        self
    }
}

// ==================== 生命周期监听器 Trait ====================

/// 插件生命周期监听器 trait
///
/// cmx-plugin 在插件状态变更时调用此 trait 的方法通知监听者。
/// cmx-service 实现此 trait，在收到通知后加载/卸载 WASM 模块。
///
/// # 注意
///
/// trait 方法不返回 Result，监听者内部自行处理错误并记录日志，
/// 不应阻塞插件生命周期流程。
#[async_trait]
pub trait PluginLifecycleListener: Send + Sync {
    /// 插件已安装 — 通知监听者加载服务定义
    async fn on_plugin_installed(&self, event: PluginLifecyclePayload);

    /// 插件已升级 — 通知监听者更新服务定义
    async fn on_plugin_upgraded(&self, event: PluginLifecyclePayload);

    /// 插件已卸载 — 通知监听者清理资源
    async fn on_plugin_uninstalled(&self, event: PluginLifecyclePayload);

    // 暂时注释，暂无此功能
    // /// 插件已激活 — 通知监听者加载 WASM 模块
    // async fn on_plugin_activated(&self, event: PluginLifecyclePayload);
    // 
    // /// 插件已停用 — 通知监听者卸载 WASM 模块
    // async fn on_plugin_deactivated(&self, event: PluginLifecyclePayload);
}

// ==================== 向后兼容 ====================

/// 生命周期事件（向后兼容，已废弃）
#[deprecated(since = "0.2.0", note = "请使用 PluginLifecyclePayload")]
pub type LifecycleEvent = PluginLifecyclePayload;
```

### 3.3 cmx-plugin 模块修改

#### 删除文件：`cmx-plugin/src/infrastructure/messaging/event.rs`

迁移到 cmx-core，cmx-plugin 使用 cmx-core 提供的全局 EventBus。

#### 修改文件：`cmx-plugin/src/service/install.rs`

```rust
// 在文件顶部添加导入
use cmx_core::GlobalEventBus;
use cmx_traits::{plugin_events, PluginLifecyclePayload};

// 在安装成功后发布事件（替换原有的 event_bus.publish 调用）
// 步骤14: 发布安装完成事件
let payload = PluginLifecyclePayload::new(&plugin_id, &install_version)
    .with_install_path(install_path.clone())
    .with_wasm_path(wasm_path);

GlobalEventBus::get()
    .publish(plugin_events::INSTALLED, serde_json::to_value(&payload).unwrap())
    .await;
```

#### 修改文件：`cmx-plugin/src/service/upgrade.rs`

```rust
// 在文件顶部添加导入
use cmx_core::GlobalEventBus;
use cmx_traits::{plugin_events, PluginLifecyclePayload};

// 在升级成功后发布事件
let payload = PluginLifecyclePayload::new(&plugin_id, &new_version)
    .with_old_version(&old_version)
    .with_install_path(install_path.clone())
    .with_wasm_path(wasm_path);

GlobalEventBus::get()
    .publish(plugin_events::UPGRADED, serde_json::to_value(&payload).unwrap())
    .await;
```

#### 修改文件：`cmx-plugin/src/service/uninstall.rs`

```rust
// 在文件顶部添加导入
use cmx_core::GlobalEventBus;
use cmx_traits::{plugin_events, PluginLifecyclePayload};

// 在卸载成功后发布事件
let payload = PluginLifecyclePayload::new(&plugin_id, &version);

GlobalEventBus::get()
    .publish(plugin_events::UNINSTALLED, serde_json::to_value(&payload).unwrap())
    .await;
```

#### 修改文件：`cmx-plugin/src/core/manager.rs`

移除对 `crate::infrastructure::messaging::event::EventBus` 的依赖，使用 `cmx_core::GlobalEventBus`。

### 3.4 cmx-service 模块修改

#### 新增文件：`cmx-service/src/lifecycle_listener.rs`

```rust
//! 服务生命周期监听器
//!
//! 监听插件生命周期事件，同步服务缓存。

use std::sync::Arc;
use cmx_core::{GlobalEventBus, EventHandler};
use cmx_traits::{plugin_events, PluginLifecyclePayload, ServiceStorage};
use crate::registry::ServiceRegistry;
use tracing::{info, warn, error};

/// 服务生命周期监听器
///
/// 监听插件生命周期事件，自动同步服务定义缓存。
pub struct ServiceLifecycleListener {
    service_storage: Arc<dyn ServiceStorage>,
    service_registry: Arc<ServiceRegistry>,
}

impl ServiceLifecycleListener {
    /// 创建监听器
    pub fn new(
        service_storage: Arc<dyn ServiceStorage>,
        service_registry: Arc<ServiceRegistry>,
    ) -> Self {
        Self {
            service_storage,
            service_registry,
        }
    }

    /// 注册到全局事件总线
    pub async fn register(&self) {
        // 订阅安装事件
        let storage = self.service_storage.clone();
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let storage = storage.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_installed(storage, registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::INSTALLED, handler).await;

        // 订阅升级事件
        let storage = self.service_storage.clone();
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let storage = storage.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_upgraded(storage, registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UPGRADED, handler).await;

        // 订阅卸载事件
        let registry = self.service_registry.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let registry = registry.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_uninstalled(registry, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNINSTALLED, handler).await;

        info!("服务生命周期监听器已注册");
    }

    /// 处理安装事件：从数据库加载服务定义到缓存
    async fn handle_installed(
        storage: Arc<dyn ServiceStorage>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!("处理插件安装事件: {} v{}", event.plugin_id, event.version);
        
        match storage.get_services_by_plugin(&event.plugin_id).await {
            Ok(services) => {
                let mut orchestrations = std::collections::HashMap::new();
                for service in &services {
                    if let Some(config) = &service.config {
                        if let Ok(orch) = serde_json::from_str::<serde_json::Value>(config) {
                            orchestrations.insert(service.service_key.clone(), orch);
                        }
                    }
                }
                
                let service_infos: Vec<cmx_core::model::service::ServiceInfo> = 
                    services.into_iter().map(|s| s.into()).collect();
                
                registry.sync_plugin_services(&event.plugin_id, service_infos, orchestrations).await;
                info!("插件 {} 服务定义已加载到缓存", event.plugin_id);
            }
            Err(e) => {
                error!("加载插件 {} 服务定义失败: {}", event.plugin_id, e);
            }
        }
    }

    /// 处理升级事件：更新服务定义缓存
    async fn handle_upgraded(
        storage: Arc<dyn ServiceStorage>,
        registry: Arc<ServiceRegistry>,
        event: PluginLifecyclePayload,
    ) {
        info!("处理插件升级事件: {} {} -> {}", event.plugin_id, 
            event.old_version.as_deref().unwrap_or("?"), event.version);
        
        // 升级时重新加载服务定义（逻辑与安装相同）
        Self::handle_installed(storage, registry, event).await;
    }

    /// 处理卸载事件：清理服务定义缓存
    async fn handle_uninstalled(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
        info!("处理插件卸载事件: {} v{}", event.plugin_id, event.version);
        
        // 获取该插件的所有服务键
        let services = registry.get_by_plugin(&event.plugin_id).await;
        
        // 从缓存中移除
        for service in services {
            registry.unregister(&service.service_key, &event.plugin_id).await;
        }
        
        info!("插件 {} 服务定义已从缓存清理", event.plugin_id);
    }
}
```

#### 修改文件：`cmx-service/src/lib.rs`

```rust
pub mod lifecycle_listener;
pub use lifecycle_listener::ServiceLifecycleListener;
```

### 3.5 cmx-runtime 模块修改

#### 新增文件：`cmx-runtime/src/lifecycle_listener.rs`

```rust
//! 运行时生命周期监听器
//!
//! 监听插件生命周期事件，清除 WASM 实例缓存。

use std::sync::Arc;
use cmx_core::{GlobalEventBus, EventHandler};
use cmx_traits::{plugin_events, PluginLifecyclePayload, RuntimeInvoker};
use tracing::{info, warn};

/// 运行时生命周期监听器
///
/// 监听插件生命周期事件，在插件升级/卸载时清除 WASM 实例缓存。
pub struct RuntimeLifecycleListener {
    runtime_invoker: Arc<dyn RuntimeInvoker>,
}

impl RuntimeLifecycleListener {
    /// 创建监听器
    pub fn new(runtime_invoker: Arc<dyn RuntimeInvoker>) -> Self {
        Self { runtime_invoker }
    }

    /// 注册到全局事件总线
    pub async fn register(&self) {
        // 订阅升级事件
        let invoker = self.runtime_invoker.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_upgraded(invoker, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UPGRADED, handler).await;

        // 订阅卸载事件
        let invoker = self.runtime_invoker.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let invoker = invoker.clone();
            tokio::spawn(async move {
                if let Ok(event) = serde_json::from_value::<PluginLifecyclePayload>(payload) {
                    Self::handle_uninstalled(invoker, event).await;
                }
            });
        });
        GlobalEventBus::get().subscribe(plugin_events::UNINSTALLED, handler).await;

        info!("运行时生命周期监听器已注册");
    }

    /// 处理升级事件：清除 WASM 实例缓存
    async fn handle_upgraded(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!("处理插件升级事件，清除 WASM 缓存: {} {} -> {}", 
            event.plugin_id, event.old_version.as_deref().unwrap_or("?"), event.version);
        
        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存", event.plugin_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {}", event.plugin_id, e),
        }
    }

    /// 处理卸载事件：清除 WASM 实例缓存
    async fn handle_uninstalled(invoker: Arc<dyn RuntimeInvoker>, event: PluginLifecyclePayload) {
        info!("处理插件卸载事件，清除 WASM 缓存: {} v{}", event.plugin_id, event.version);
        
        match invoker.unload_module(&event.plugin_id).await {
            Ok(()) => info!("已清除插件 {} WASM 实例缓存", event.plugin_id),
            Err(e) => warn!("清除插件 {} WASM 缓存失败: {}", event.plugin_id, e),
        }
    }
}
```

#### 修改文件：`cmx-runtime/src/lib.rs`

```rust
pub mod lifecycle_listener;
pub use lifecycle_listener::RuntimeLifecycleListener;
```

### 3.6 web-server 模块修改

#### 修改文件：`web-server/src/config.rs`

```rust
pub async fn init_services() {
    use cmx_service::{GlobalServiceQuery, GlobalServiceStorage, GlobalServiceRegistry, 
                      ServiceRepository, ServiceRegistry, ServiceQueryImpl, ServiceStorageImpl,
                      ServiceLifecycleListener};
    use cmx_runtime::RuntimeLifecycleListener;
    use cmx_traits::{ServiceQuery, ServiceStorage};

    info!("初始化服务管理器...");

    // ... 现有初始化代码 ...

    // 注册服务生命周期监听器
    let service_listener = ServiceLifecycleListener::new(
        GlobalServiceStorage::get().clone(),
        GlobalServiceRegistry::get().clone(),
    );
    service_listener.register().await;

    // 注册运行时生命周期监听器
    let runtime_listener = RuntimeLifecycleListener::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker()
    );
    runtime_listener.register().await;

    info!("生命周期监听器已注册");
    info!("服务管理器初始化完成");
}
```

#### 修改文件：`web-server/src/main.rs`

```rust
// 在 init_plugins() 之前初始化全局 EventBus
info!("初始化全局事件总线...");
cmx_core::GlobalEventBus::initialize().expect("初始化全局事件总线失败");

// 初始化 WASM 运行时（必须在 init_plugins 之前）
init_runtime().await;

// 初始化插件管理器
init_plugins().await;

// 初始化服务管理器（会注册监听器）
init_services().await;
```

## 四、需要修改的文件清单

| 模块 | 文件 | 操作 |
|------|------|------|
| cmx-core | `src/event_bus/mod.rs` | 新增 |
| cmx-core | `src/event_bus/bus.rs` | 新增 |
| cmx-core | `src/event_bus/global.rs` | 新增 |
| cmx-core | `src/event_bus/types.rs` | 新增 |
| cmx-core | `src/error.rs` | 新增 |
| cmx-core | `src/lib.rs` | 修改 |
| cmx-traits | `src/lifecycle.rs` | 修改 |
| cmx-plugin | `src/infrastructure/messaging/event.rs` | 删除 |
| cmx-plugin | `src/service/install.rs` | 修改 |
| cmx-plugin | `src/service/upgrade.rs` | 修改 |
| cmx-plugin | `src/service/uninstall.rs` | 修改 |
| cmx-plugin | `src/core/manager.rs` | 修改 |
| cmx-service | `src/lifecycle_listener.rs` | 新增 |
| cmx-service | `src/lib.rs` | 修改 |
| cmx-runtime | `src/lifecycle_listener.rs` | 新增 |
| cmx-runtime | `src/lib.rs` | 修改 |
| web-server | `src/config.rs` | 修改 |
| web-server | `src/main.rs` | 修改 |

## 五、初始化顺序

```
1. init_global_config()
2. init_datasources()
3. init_cache()
4. GlobalEventBus::initialize()  ← 新增
5. init_runtime()
6. init_plugins()
7. init_services()  ← 内部注册 ServiceLifecycleListener 和 RuntimeLifecycleListener
```

## 六、通用 EventBus 使用示例

```rust
use cmx_core::{GlobalEventBus, EventHandler};
use serde_json::json;

// 定义新的事件类型
const CACHE_INVALIDATED: &str = "cache.invalidated";
const DATABASE_CONNECTED: &str = "database.connected";

// 订阅事件
let handler: EventHandler = Arc::new(|topic, payload| {
    println!("收到事件: {} -> {:?}", topic, payload);
});
GlobalEventBus::get().subscribe(CACHE_INVALIDATED, handler).await;

// 发布事件
GlobalEventBus::get().publish(CACHE_INVALIDATED, json!({
    "cache_key": "user:123",
    "reason": "data_updated",
})).await;
```

## 七、注意事项

1. **线程安全**：EventBus 使用 `Arc<RwLock>` 保证线程安全
2. **异步处理**：事件处理器使用 `tokio::spawn` 异步执行，不阻塞发布者
3. **错误处理**：监听器内部错误记录日志，不影响插件生命周期流程
4. **幂等性**：服务缓存操作和 WASM 缓存清理需要保证幂等
5. **缓存清理时机**：
   - 升级时：先清除旧版本缓存，下次调用时加载新版本
   - 卸载时：直接清除缓存
6. **事件顺序**：事件在插件操作成功后发布，确保数据一致性
7. **通用性**：EventBus 设计为通用组件，支持任意事件类型
