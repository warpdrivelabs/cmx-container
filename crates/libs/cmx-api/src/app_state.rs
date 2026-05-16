//! CMX State 模块
//!
//! 定义应用程序的共享状态，支持运行时动态修改。
//! 包含 PluginQuery 和 RuntimeInvoker trait 对象，支持跨模块解耦调用。

use std::sync::Arc;
use tokio::sync::RwLock;
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery, ServiceStorage};
use cmx_storage::service::StorageService;

/// CMX 应用程序状态
///
/// 包含应用程序运行时的共享状态。
/// DatabaseManager 通过 get_default_db_manager() 全局获取，不需要通过 state 传递。
///
/// # 使用示例
/// ```rust
/// use cmx_api::CmxAppState;
/// use std::sync::Arc;
///
/// let state = CmxAppState::new()
///     .with_plugin_query(plugin_manager)
///     .with_runtime_invoker(wasm_engine);
/// ```
pub struct CmxAppState {
    /// 内部可修改的状态
    pub app_state: Arc<RwLock<AppStateInner>>,
    /// 插件查询器（trait 对象）
    plugin_query: Option<Arc<dyn PluginQuery>>,
    /// WASM 运行时调用器（trait 对象）
    runtime_invoker: Option<Arc<dyn RuntimeInvoker>>,
    /// 服务查询器（trait 对象）
    service_query: Option<Arc<dyn ServiceQuery>>,
    /// 服务存储（trait 对象）
    service_storage: Option<Arc<dyn ServiceStorage>>,
    storage_service: Option<Arc<dyn StorageService>>,
}

/// 内部状态结构
#[derive(Debug, Clone)]
pub struct AppStateInner {
    // /// 默认数据库 ID
    // pub default_db_id: String,
}

impl Default for CmxAppState {
    fn default() -> Self {
        Self::new()
    }
}

impl CmxAppState {
    /// 创建新的 CmxAppState
    pub fn new() -> Self {
        Self {
            app_state: Arc::new(RwLock::new(AppStateInner {})),
            plugin_query: None,
            runtime_invoker: None,
            service_query: None,
            service_storage: None,
            storage_service: None,
        }
    }

    /// 设置插件查询器
    ///
    /// # 参数
    ///
    /// * `query` - 实现 PluginQuery trait 的实例
    pub fn with_plugin_query(mut self, query: Arc<dyn PluginQuery>) -> Self {
        self.plugin_query = Some(query);
        self
    }

    /// 设置运行时调用器
    ///
    /// # 参数
    ///
    /// * `invoker` - 实现 RuntimeInvoker trait 的实例
    pub fn with_runtime_invoker(mut self, invoker: Arc<dyn RuntimeInvoker>) -> Self {
        self.runtime_invoker = Some(invoker);
        self
    }

    /// 设置服务查询器
    ///
    /// # 参数
    ///
    /// * `query` - 实现 ServiceQuery trait 的实例
    pub fn with_service_query(mut self, query: Arc<dyn ServiceQuery>) -> Self {
        self.service_query = Some(query);
        self
    }

    /// 设置服务存储
    ///
    /// # 参数
    ///
    /// * `storage` - 实现 ServiceStorage trait 的实例
    pub fn with_service_storage(mut self, storage: Arc<dyn ServiceStorage>) -> Self {
        self.service_storage = Some(storage);
        self
    }

    pub fn with_storage_service(mut self, service: Arc<dyn StorageService>) -> Self {
        self.storage_service = Some(service);
        self
    }

    /// 获取插件查询器
    ///
    /// # 返回值
    ///
    /// 返回 PluginQuery trait 对象引用，如果未设置返回 None。
    pub fn plugin_query(&self) -> Option<&Arc<dyn PluginQuery>> {
        self.plugin_query.as_ref()
    }

    /// 获取运行时调用器
    ///
    /// # 返回值
    ///
    /// 返回 RuntimeInvoker trait 对象引用，如果未设置返回 None。
    pub fn runtime_invoker(&self) -> Option<&Arc<dyn RuntimeInvoker>> {
        self.runtime_invoker.as_ref()
    }

    /// 获取服务查询器
    ///
    /// # 返回值
    ///
    /// 返回 ServiceQuery trait 对象引用，如果未设置返回 None。
    pub fn service_query(&self) -> Option<&Arc<dyn ServiceQuery>> {
        self.service_query.as_ref()
    }

    /// 获取服务存储
    ///
    /// # 返回值
    ///
    /// 返回 ServiceStorage trait 对象引用，如果未设置返回 None。
    pub fn service_storage(&self) -> Option<&Arc<dyn ServiceStorage>> {
        self.service_storage.as_ref()
    }

    pub fn storage_service(&self) -> Option<&Arc<dyn StorageService>> {
        self.storage_service.as_ref()
    }

    /// 检查是否已完全初始化
    ///
    /// 返回 true 表示 plugin_query 和 runtime_invoker 都已设置。
    pub fn is_fully_initialized(&self) -> bool {
        self.plugin_query.is_some() && self.runtime_invoker.is_some()
    }
}

impl Clone for CmxAppState {
    fn clone(&self) -> Self {
        Self {
            app_state: self.app_state.clone(),
            plugin_query: self.plugin_query.clone(),
            runtime_invoker: self.runtime_invoker.clone(),
            service_query: self.service_query.clone(),
            service_storage: self.service_storage.clone(),
            storage_service: self.storage_service.clone(),
        }
    }
}
