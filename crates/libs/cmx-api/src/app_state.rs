//! CMX State 模块
//!
//! 定义应用程序的共享状态。
//! 包含 PluginQuery 和 RuntimeInvoker trait 对象，支持跨模块解耦调用。

use cmx_iam::service_traits::{PermissionService, RoleGroupService, RoleService, UserService};
use cmx_storage::service::StorageService;
use cmx_traits::auth::{AuthService, UserAuthQuery};
use cmx_traits::iam::PermissionChecker;
use cmx_traits::plugin::{PluginDataImporter, PluginQuery};
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::{ServiceQuery, ServiceStorage};
use std::sync::Arc;

/// IAM 服务状态
///
/// 聚合 IAM 模块的所有服务实例，通过 CmxAppState.iam() 访问。
pub struct IamState {
    /// 用户服务
    pub user_service: Arc<dyn UserService>,
    /// 角色服务
    pub role_service: Arc<dyn RoleService>,
    /// 角色组服务
    pub role_group_service: Arc<dyn RoleGroupService>,
    /// 权限服务
    pub permission_service: Arc<dyn PermissionService>,
    /// 互斥规则服务
    pub rule_service: Option<Arc<dyn cmx_iam::rule::service::ExclusionRuleService>>,
    /// 权限校验器
    pub permission_checker: Arc<dyn PermissionChecker>,
    /// IAM 权限校验器实现引用（阶段3新增，用于缓存失效）
    pub iam_checker: Option<Arc<cmx_iam::IamChecker>>,
    /// 用户认证查询（供 cmx-auth 使用）
    pub user_auth_query: Arc<dyn UserAuthQuery>,
}

impl IamState {
    /// 获取 IamChecker 引用（用于 finalize_iam_state 注入到 UserServiceImpl）
    pub fn permission_checker_clone(&self) -> Arc<cmx_iam::IamChecker> {
        self.iam_checker
            .clone()
            .expect("IamChecker 未初始化：请在 finalize_iam_state 中完成注入后再调用本方法")
    }
}

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
    /// 应用隔离标识，用于多租户/多应用场景隔离（初始化后不可变）
    pub app_id: String,
    /// 插件查询器（trait 对象）
    plugin_query: Option<Arc<dyn PluginQuery>>,
    /// WASM 运行时调用器（trait 对象）
    runtime_invoker: Option<Arc<dyn RuntimeInvoker>>,
    /// 服务查询器（trait 对象）
    service_query: Option<Arc<dyn ServiceQuery>>,
    /// 服务存储（trait 对象）
    service_storage: Option<Arc<dyn ServiceStorage>>,
    /// 存储服务（trait 对象）
    storage_service: Option<Arc<dyn StorageService>>,
    /// 认证服务（trait 对象）
    auth_service: Option<Arc<dyn AuthService>>,
    /// IAM 服务状态
    iam: Option<Arc<IamState>>,
    /// 插件数据导入器（trait 对象，用于 HTTP/gRPC 接收端统一调用）
    plugin_data_importer: Option<Arc<dyn PluginDataImporter>>,
}

impl Default for CmxAppState {
    fn default() -> Self {
        Self::new()
    }
}

impl CmxAppState {
    /// 创建新的 CmxAppState
    pub fn new() -> Self {
        let app_id = cmx_utils::ConfigManager::global().get_app_id();

        Self {
            app_id,
            plugin_query: None,
            runtime_invoker: None,
            service_query: None,
            service_storage: None,
            storage_service: None,
            auth_service: None,
            iam: None,
            plugin_data_importer: None,
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

    /// 设置认证服务
    pub fn with_auth_service(mut self, service: Arc<dyn AuthService>) -> Self {
        self.auth_service = Some(service);
        self
    }

    /// 设置 IAM 服务状态
    pub fn with_iam(mut self, iam: Arc<IamState>) -> Self {
        self.iam = Some(iam);
        self
    }

    /// 设置插件数据导入器
    pub fn with_plugin_data_importer(mut self, importer: Arc<dyn PluginDataImporter>) -> Self {
        self.plugin_data_importer = Some(importer);
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

    /// 获取认证服务
    pub fn auth_service(&self) -> Option<&Arc<dyn AuthService>> {
        self.auth_service.as_ref()
    }

    /// 获取 IAM 服务状态
    pub fn iam(&self) -> Option<&Arc<IamState>> {
        self.iam.as_ref()
    }

    /// 获取插件数据导入器
    pub fn plugin_data_importer(&self) -> Option<&Arc<dyn PluginDataImporter>> {
        self.plugin_data_importer.as_ref()
    }

    /// 获取应用隔离标识
    ///
    /// 返回 app_id 字符串的副本。
    pub fn app_id(&self) -> String {
        self.app_id.clone()
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
            app_id: self.app_id.clone(),
            plugin_query: self.plugin_query.clone(),
            runtime_invoker: self.runtime_invoker.clone(),
            service_query: self.service_query.clone(),
            service_storage: self.service_storage.clone(),
            storage_service: self.storage_service.clone(),
            auth_service: self.auth_service.clone(),
            iam: self.iam.clone(),
            plugin_data_importer: self.plugin_data_importer.clone(),
        }
    }
}
