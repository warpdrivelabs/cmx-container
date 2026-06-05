//! 配置变更通知器
//!
//! 当远程配置发生变更时，通知已注册的回调处理器。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use tracing::{debug, info, warn};

/// 配置变更回调函数类型
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 配置变更通知器
///
/// 管理配置变更回调处理器，当远程配置变更时通知所有注册的处理器。
pub struct ChangeNotifier {
    handlers: RwLock<HashMap<String, ConfigChangeCallback>>,
}

impl ChangeNotifier {
    /// 创建新的配置变更通知器
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
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

    /// 通知所有处理器配置已变更
    pub fn notify(&self, content: &str) {
        let handlers = self.handlers.read().unwrap();
        debug!("通知 {} 个配置变更处理器", handlers.len());

        for (key, callback) in handlers.iter() {
            debug!("调用配置变更处理器: {}", key);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(content))) {
                Ok(_) => {}
                Err(e) => {
                    warn!("配置变更处理器 [{}] 执行异常: {:?}", key, e);
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

    /// 通知所有处理器配置已变更
    pub fn notify(content: &str) {
        let guard = get_notifier();
        let notifier = guard.read().unwrap();
        if let Some(ref n) = *notifier {
            n.notify(content);
        } else {
            warn!("全局配置变更通知器未初始化，无法通知配置变更");
        }
    }
}
