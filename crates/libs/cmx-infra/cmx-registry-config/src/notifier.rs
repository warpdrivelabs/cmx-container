//! 配置变更通知器
//!
//! 当远程配置发生变更时，通知已注册的回调处理器。
//! 支持两种通知模式：
//! - 原始字符串回调（`ConfigChangeCallback`）：向后兼容
//! - 结构化监听器（`ConfigChangeListener`）：提供变更事件详情
//!
//! 详细机制说明参见 `docs/配置变更事件订阅发布机制.md`。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use tracing::{debug, info, warn};

/// 配置变更回调函数类型
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 配置变更事件
///
/// 提供结构化的配置变更信息，包含变更的键列表和原始内容。
pub struct ConfigChangeEvent {
    /// 变更的配置键列表
    pub changed_keys: Vec<String>,
    /// 新的配置内容（原始 TOML 字符串）
    pub raw_content: String,
}

/// 配置变更监听器
///
/// 业务模块实现此 trait 以接收结构化的配置变更通知。
pub trait ConfigChangeListener: Send + Sync {
    /// 监听器名称（用于日志和调试）
    fn name(&self) -> &str;

    /// 感兴趣的配置键前缀（空切片表示监听所有变更）
    fn interested_keys(&self) -> &[String] {
        &[]
    }

    /// 配置变更回调
    fn on_change(&self, event: &ConfigChangeEvent);
}

/// 配置变更通知器
///
/// 管理配置变更回调处理器和结构化监听器，当远程配置变更时通知所有注册的处理器。
pub struct ChangeNotifier {
    handlers: RwLock<HashMap<String, ConfigChangeCallback>>,
    listeners: RwLock<Vec<Arc<dyn ConfigChangeListener>>>,
}

impl ChangeNotifier {
    /// 创建新的配置变更通知器
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// 注册配置变更处理器
    pub fn register(&self, key: &str, callback: ConfigChangeCallback) {
        let mut handlers = self.handlers.write().unwrap();
        info!("注册配置变更处理器: {}", key);
        handlers.insert(key.to_string(), callback);
    }

    /// 移除配置变更处理器
    pub fn unregister(&self, key: &str) {
        let mut handlers = self.handlers.write().unwrap();
        info!("移除配置变更处理器: {}", key);
        handlers.remove(key);
    }

    /// 注册结构化配置变更监听器
    pub fn add_listener(&self, listener: Arc<dyn ConfigChangeListener>) {
        let mut listeners = self.listeners.write().unwrap();
        info!("注册配置变更监听器: {}", listener.name());
        listeners.push(listener);
    }

    /// 移除结构化配置变更监听器
    pub fn remove_listener(&self, name: &str) {
        let mut listeners = self.listeners.write().unwrap();
        listeners.retain(|l| {
            if l.name() == name {
                info!("移除配置变更监听器: {}", name);
                false
            } else {
                true
            }
        });
    }

    /// 通知所有处理器配置已变更（原始字符串模式）
    pub fn notify(&self, content: &str) {
        let handlers = self.handlers.read().unwrap();
        info!("通知 {} 个配置变更处理器", handlers.len());

        for (key, callback) in handlers.iter() {
            info!("调用配置变更处理器: {}", key);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(content))) {
                Ok(_) => {}
                Err(e) => {
                    warn!("配置变更处理器 [{}] 执行异常: {:?}", key, e);
                }
            }
        }
    }

    /// 通知结构化监听器（仅 typed listener，不触发原始回调）
    ///
    /// 由 `config_reloader` 在完成配置替换后调用，避免与原始回调形成循环。
    pub fn notify_listeners(&self, event: &ConfigChangeEvent) {
        let listeners = self.listeners.read().unwrap();
        if listeners.is_empty() {
            return;
        }

        for listener in listeners.iter() {
            let interested = listener.interested_keys();
            let should_notify = interested.is_empty()
                || event.changed_keys.iter().any(|k| {
                    interested.iter().any(|prefix| k.starts_with(prefix.as_str()))
                });

            if should_notify {
                debug!("调用配置变更监听器: {}", listener.name());
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    listener.on_change(event)
                })) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!("配置变更监听器 [{}] 执行异常: {:?}", listener.name(), e);
                    }
                }
            }
        }
    }
}

impl Default for ChangeNotifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局配置变更通知器
pub struct GlobalChangeNotifier;

static NOTIFIER: OnceLock<RwLock<Option<ChangeNotifier>>> = OnceLock::new();

fn get_notifier() -> &'static RwLock<Option<ChangeNotifier>> {
    NOTIFIER.get_or_init(|| RwLock::new(None))
}

impl GlobalChangeNotifier {
    /// 初始化全局配置变更通知器
    pub fn initialize() {
        let guard = get_notifier();
        let mut notifier = guard.write().unwrap();
        if notifier.is_some() {
            warn!("全局配置变更通知器已初始化，跳过重复初始化");
            return;
        }
        *notifier = Some(ChangeNotifier::new());
        info!("全局配置变更通知器初始化完成");
    }

    /// 注册配置变更处理器
    pub fn register(key: &str, callback: ConfigChangeCallback) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.register(key, callback);
        } else {
            warn!("全局配置变更通知器未初始化，无法注册处理器: {}", key);
        }
    }

    /// 注册结构化配置变更监听器
    pub fn add_listener(listener: Arc<dyn ConfigChangeListener>) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.add_listener(listener);
        } else {
            warn!(
                "全局配置变更通知器未初始化，无法注册监听器: {}",
                listener.name()
            );
        }
    }

    /// 通知所有处理器配置已变更（原始字符串模式）
    pub fn notify(content: &str) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.notify(content);
        } else {
            warn!("全局配置变更通知器未初始化，无法通知配置变更");
        }
    }

    /// 通知结构化监听器（仅 typed listener）
    pub fn notify_listeners(event: &ConfigChangeEvent) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.notify_listeners(event);
        } else {
            warn!("全局配置变更通知器未初始化，无法通知配置变更事件");
        }
    }
}
