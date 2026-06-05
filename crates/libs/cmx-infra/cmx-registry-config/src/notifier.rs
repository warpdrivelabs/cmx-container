//! 配置变更通知器。
//!
//! 该模块是配置中心变更事件在应用内部分发的核心组件，提供了两层 Pub/Sub 机制：
//!
//! - **传输层**：`ConfigChangeCallback`（`Fn(&str)`），用于适配 nacos-sdk 等配置中心 SDK 的回调接口。
//! - **应用层**：`ConfigChangeListener` trait，提供类型安全的结构化配置变更事件。
//!
//! 全局单例 `GlobalChangeNotifier` 内部通过 `OnceLock<RwLock<Option<ChangeNotifier>>>` 实现线程安全。
//! 所有回调执行均使用 `std::panic::catch_unwind` 包裹，单个处理器 panic 不会影响其他处理器。
//!
//! 详细机制说明参见 `docs/配置变更事件订阅发布机制.md`。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use tracing::{debug, info, warn};

/// 配置变更回调函数类型。
///
/// 接收配置中心推送的原始 TOML 字符串内容，主要用于：
/// - 适配 `nacos-sdk` 的 `ConfigChangeListener` 接口
/// - 兼容已有业务代码
///
/// # Examples
///
/// ```ignore
/// let callback: ConfigChangeCallback = Arc::new(|content: &str| {
///     info!("收到配置变更: {} 字节", content.len());
/// });
/// ```
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 配置变更事件。
///
/// 提供结构化的配置变更信息，业务模块可基于 `changed_keys` 做增量更新，
/// 而非对全量配置进行重新加载。
pub struct ConfigChangeEvent {
    /// 变更的配置键列表。
    ///
    /// 由 `ConfigReloader` 在解析新配置后计算得出，包含新增、删除和值变化的 key。
    pub changed_keys: Vec<String>,

    /// 新的配置内容（原始 TOML 字符串）。
    pub raw_content: String,
}

/// 配置变更监听器 trait。
///
/// 业务模块通过实现该 trait 订阅配置变更。
/// `interested_keys` 方法可按 key 前缀过滤，避免无关变更触发不必要的处理。
///
/// # Examples
///
/// ```ignore
/// use cmx_registry_config::{ConfigChangeEvent, ConfigChangeListener};
///
/// struct DatabaseListener;
///
/// impl ConfigChangeListener for DatabaseListener {
///     fn name(&self) -> &str { "database" }
///
///     fn interested_keys(&self) -> &[String] {
///         static KEYS: &[String] = &["database"];
///         KEYS
///     }
///
///     fn on_change(&self, event: &ConfigChangeEvent) {
///         info!("数据库配置变更: {:?}", event.changed_keys);
///     }
/// }
/// ```
pub trait ConfigChangeListener: Send + Sync {
    /// 返回监听器名称，用于日志和调试。
    fn name(&self) -> &str;

    /// 返回感兴趣的配置键前缀列表。
    ///
    /// # Returns
    ///
    /// * 空切片 —— 监听所有变更。
    /// * 非空切片 —— 遍历 `event.changed_keys`，任一 key 以某个 prefix 开头即触发 `on_change`。
    fn interested_keys(&self) -> &[String] {
        &[]
    }

    /// 配置变更回调。
    ///
    /// # Arguments
    ///
    /// * `event` - 配置变更事件，包含变更的 key 列表和原始内容。
    fn on_change(&self, event: &ConfigChangeEvent);
}

/// 配置变更通知器实例。
///
/// 内部维护两个并发安全的容器：
/// - `handlers`：原始字符串回调（按 key 索引，用于向后兼容）
/// - `listeners`：结构化监听器（按注册顺序，type-safe）
///
/// 通过 `RwLock` 实现并发安全，处理器串行执行。
pub struct ChangeNotifier {
    /// 原始字符串回调，按 key 索引。
    handlers: RwLock<HashMap<String, ConfigChangeCallback>>,

    /// 结构化监听器列表。
    listeners: RwLock<Vec<Arc<dyn ConfigChangeListener>>>,
}

impl ChangeNotifier {
    /// 创建新的配置变更通知器实例。
    ///
    /// # Returns
    ///
    /// 返回空 handlers 和空 listeners 的 `ChangeNotifier` 实例。
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// 注册原始字符串回调处理器。
    ///
    /// # Arguments
    ///
    /// * `key` - 处理器唯一标识，重复注册会覆盖已存在的同名处理器。
    /// * `callback` - 配置变更回调函数。
    pub fn register(&self, key: &str, callback: ConfigChangeCallback) {
        let mut handlers = self.handlers.write().unwrap();
        info!("注册配置变更处理器: {}", key);
        handlers.insert(key.to_string(), callback);
    }

    /// 移除指定 key 的原始字符串回调处理器。
    ///
    /// # Arguments
    ///
    /// * `key` - 要移除的处理器标识。
    pub fn unregister(&self, key: &str) {
        let mut handlers = self.handlers.write().unwrap();
        info!("移除配置变更处理器: {}", key);
        handlers.remove(key);
    }

    /// 注册结构化配置变更监听器。
    ///
    /// # Arguments
    ///
    /// * `listener` - 实现 `ConfigChangeListener` trait 的监听器实例。
    pub fn add_listener(&self, listener: Arc<dyn ConfigChangeListener>) {
        let mut listeners = self.listeners.write().unwrap();
        info!("注册配置变更监听器: {}", listener.name());
        listeners.push(listener);
    }

    /// 移除指定名称的结构化配置变更监听器。
    ///
    /// 按 `ConfigChangeListener::name()` 匹配。
    ///
    /// # Arguments
    ///
    /// * `name` - 要移除的监听器名称。
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

    /// 通知所有原始字符串处理器配置已变更。
    ///
    /// 处理器按注册顺序依次调用，单个 panic 被 `catch_unwind` 捕获不影响后续处理器。
    ///
    /// # Arguments
    ///
    /// * `content` - 新的配置内容（TOML 字符串）。
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

    /// 通知结构化监听器配置已变更。
    ///
    /// 仅触发 typed listener（不触发原始字符串回调），避免与 `notify()` 形成循环。
    /// 监听器按 `interested_keys()` 过滤后依次调用。
    ///
    /// # Arguments
    ///
    /// * `event` - 配置变更事件。
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
    /// 返回默认（空）通知器实例。
    fn default() -> Self {
        Self::new()
    }
}

/// 全局配置变更通知器门面。
///
/// 通过 `OnceLock` 实现线程安全的全局单例访问。
/// 应用启动时调用 [`initialize`] 初始化一次，之后可在任意位置调用静态方法。
pub struct GlobalChangeNotifier;

/// 全局通知器存储。`OnceLock` 保证线程安全的延迟初始化。
static NOTIFIER: OnceLock<RwLock<Option<ChangeNotifier>>> = OnceLock::new();

/// 获取全局通知器存储的可变引用。
fn get_notifier() -> &'static RwLock<Option<ChangeNotifier>> {
    NOTIFIER.get_or_init(|| RwLock::new(None))
}

impl GlobalChangeNotifier {
    /// 初始化全局配置变更通知器。
    ///
    /// 应在应用启动时调用一次。重复调用会被忽略（幂等）并打印 warning。
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

    /// 注册原始字符串回调处理器。
    ///
    /// # Arguments
    ///
    /// * `key` - 处理器唯一标识。
    /// * `callback` - 配置变更回调函数。
    ///
    /// # Note
    ///
    /// 若未调用 [`initialize`]，此方法会打印 warning 而非 panic。
    pub fn register(key: &str, callback: ConfigChangeCallback) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.register(key, callback);
        } else {
            warn!("全局配置变更通知器未初始化，无法注册处理器: {}", key);
        }
    }

    /// 注册结构化配置变更监听器。
    ///
    /// # Arguments
    ///
    /// * `listener` - 实现 `ConfigChangeListener` trait 的监听器。
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

    /// 通知所有原始字符串处理器配置已变更。
    ///
    /// 通常由配置中心 SDK 适配器在收到推送时调用。
    ///
    /// # Arguments
    ///
    /// * `content` - 新的配置内容（TOML 字符串）。
    pub fn notify(content: &str) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.notify(content);
        } else {
            warn!("全局配置变更通知器未初始化，无法通知配置变更");
        }
    }

    /// 通知所有结构化监听器配置已变更事件。
    ///
    /// 通常由 `ConfigReloader` 在完成配置替换后调用。
    ///
    /// # Arguments
    ///
    /// * `event` - 配置变更事件。
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
