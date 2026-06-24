//! 配置变更通知器。
//!
//! 该模块是配置中心变更事件在应用内部分发的核心组件。
//!
//! - **传输层**：`ConfigChangeCallback`（`Fn(&str)`），用于适配 nacos-sdk 等配置中心 SDK 的回调接口，
//!   由调用方（如 `web-server` 的 `setup_config_listener`）负责将原始字符串解析为结构化事件。
//! - **应用层**：`ConfigChangeListener` trait，提供类型安全的结构化配置变更事件。
//!
//! 全局单例 `GlobalChangeNotifier` 内部通过 `OnceLock<RwLock<Option<ChangeNotifier>>>` 实现线程安全。
//! 所有监听器执行均使用 `std::panic::catch_unwind` 包裹，单个监听器 panic 不会影响其他监听器。

use std::sync::{Arc, OnceLock, RwLock};

use tracing::{debug, info, warn};

use crate::utils::{read_lock, write_lock};

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
/// 内部维护结构化监听器列表（按注册顺序，type-safe），
/// 通过 `RwLock` 实现并发安全，监听器串行执行。
pub struct ChangeNotifier {
    /// 结构化监听器列表。
    listeners: RwLock<Vec<Arc<dyn ConfigChangeListener>>>,
}

impl ChangeNotifier {
    /// 创建新的配置变更通知器实例。
    ///
    /// # Returns
    ///
    /// 返回空 listeners 的 `ChangeNotifier` 实例。
    pub fn new() -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
        }
    }

    /// 注册结构化配置变更监听器。
    ///
    /// # Arguments
    ///
    /// * `listener` - 实现 `ConfigChangeListener` trait 的监听器实例。
    pub fn add_listener(&self, listener: Arc<dyn ConfigChangeListener>) {
        let mut listeners = write_lock(&self.listeners);
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
        let mut listeners = write_lock(&self.listeners);
        listeners.retain(|l| {
            if l.name() == name {
                info!("移除配置变更监听器: {}", name);
                false
            } else {
                true
            }
        });
    }

    /// 通知结构化监听器配置已变更。
    ///
    /// 监听器按 `interested_keys()` 过滤后依次调用，
    /// 单个 panic 被 `catch_unwind` 捕获不影响后续监听器。
    ///
    /// # Arguments
    ///
    /// * `event` - 配置变更事件。
    pub fn notify_listeners(&self, event: &ConfigChangeEvent) {
        // 先 clone 出监听器列表，释放锁后再调用，避免回调内部操作 listeners 导致死锁
        let listeners_snapshot: Vec<Arc<dyn ConfigChangeListener>> = {
            let listeners = read_lock(&self.listeners);
            if listeners.is_empty() {
                return;
            }
            listeners.iter().cloned().collect()
        };

        for listener in &listeners_snapshot {
            let interested = listener.interested_keys();
            let should_notify = interested.is_empty()
                || event
                    .changed_keys
                    .iter()
                    .any(|k| interested.iter().any(|prefix| k.starts_with(prefix.as_str())));

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
        let mut notifier = write_lock(guard);
        if notifier.is_some() {
            warn!("全局配置变更通知器已初始化，跳过重复初始化");
            return;
        }
        *notifier = Some(ChangeNotifier::new());
        info!("全局配置变更通知器初始化完成");
    }

    /// 注册结构化配置变更监听器。
    ///
    /// # Arguments
    ///
    /// * `listener` - 实现 `ConfigChangeListener` trait 的监听器。
    pub fn add_listener(listener: Arc<dyn ConfigChangeListener>) {
        let guard = get_notifier();
        let notifier = read_lock(guard);
        if let Some(ref n) = *notifier {
            n.add_listener(listener);
        } else {
            warn!(
                "全局配置变更通知器未初始化，无法注册监听器: {}",
                listener.name()
            );
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
        let notifier = read_lock(guard);
        if let Some(ref n) = *notifier {
            n.notify_listeners(event);
        } else {
            warn!("全局配置变更通知器未初始化，无法通知配置变更事件");
        }
    }

    /// 移除指定名称的结构化配置变更监听器。
    ///
    /// # Arguments
    ///
    /// * `name` - 要移除的监听器名称。
    pub fn remove_listener(name: &str) {
        let guard = get_notifier();
        let notifier = read_lock(guard);
        if let Some(ref n) = *notifier {
            n.remove_listener(name);
        } else {
            warn!("全局配置变更通知器未初始化，无法移除监听器: {}", name);
        }
    }
}
