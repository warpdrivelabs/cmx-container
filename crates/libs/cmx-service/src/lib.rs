//! cmx-service — 企业级通用服务层
//!
//! 作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker 完成请求处理。
//!
//! # 核心功能
//!
//! - **CmxService** - 核心服务结构，实现 PluginLifecycleListener 响应插件生命周期
//! - **OrchestratorV2** - 编排执行器 V2，支持服务编排 JSON 格式、事务框、多分支节点
//! - **ServiceHandler** - HTTP 处理器，封装服务层逻辑供 cmx-api 调用
//! - **ServiceRegistry** - 服务注册中心，提供服务信息的内存缓存
//! - **ServiceRepository** - 服务仓储层，提供服务定义的数据库访问
//!
//! # 服务编排特性
//!
//! - **线性流程执行**：start -> func -> func -> end
//! - **事务框支持**：多个函数在同一个数据库事务中执行（通过 parent 字段识别）
//! - **多分支路由**：switch 节点根据返回值选择执行路径
//! - **SVRContext 上下文传递**：初始入参、请求头、各步骤输出在函数间传递
//!
//! # 依赖关系
//!
//! - 依赖 cmx-traits（trait 定义）
//! - 依赖 cmx-database（直接执行 SQL）
//! - 依赖 cmx-core（模型定义）
//! - **不依赖** cmx-plugin（通过 PluginQuery trait 交互）
//! - **不依赖** cmx-runtime（通过 RuntimeInvoker trait 交互）

pub mod error;
pub mod handler;
pub mod orchestrator_v2;
pub mod request;
pub mod service;
pub mod repository;
pub mod registry;
pub mod service_query_impl;
pub mod service_storage_impl;

pub use error::ServiceError;
pub use handler::ServiceHandler;
pub use orchestrator_v2::{OrchestratorV2, OrchestrationResultV2, ExecutionStep, ExecutionContext};
pub use request::{InvokeRequest, InvokeResponse};
pub use service::{CmxService, ServiceConfig};
pub use registry::ServiceRegistry;
pub use repository::ServiceRepository;
pub use service_query_impl::ServiceQueryImpl;
pub use service_storage_impl::ServiceStorageImpl;

// ==================== 全局单例 ====================

use std::sync::{Arc, OnceLock};
use cmx_traits::{ServiceQuery, ServiceStorage};

/// 全局服务查询器单例
static GLOBAL_SERVICE_QUERY: OnceLock<Arc<dyn ServiceQuery>> = OnceLock::new();

/// 全局服务存储单例
static GLOBAL_SERVICE_STORAGE: OnceLock<Arc<dyn ServiceStorage>> = OnceLock::new();

/// 全局服务注册中心单例
static GLOBAL_SERVICE_REGISTRY: OnceLock<Arc<ServiceRegistry>> = OnceLock::new();

/// 全局服务查询器访问器
pub struct GlobalServiceQuery;

impl GlobalServiceQuery {
    /// 设置全局服务查询器
    pub fn set(query: Arc<dyn ServiceQuery>) -> Result<(), String> {
        GLOBAL_SERVICE_QUERY
            .set(query)
            .map_err(|_| "全局服务查询器已初始化".to_string())
    }

    /// 获取全局服务查询器引用
    pub fn get() -> &'static Arc<dyn ServiceQuery> {
        GLOBAL_SERVICE_QUERY.get().expect(
            "服务查询器未初始化，请先调用 GlobalServiceQuery::set()"
        )
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_SERVICE_QUERY.get().is_some()
    }
}

/// 全局服务存储访问器
pub struct GlobalServiceStorage;

impl GlobalServiceStorage {
    /// 设置全局服务存储
    pub fn set(storage: Arc<dyn ServiceStorage>) -> Result<(), String> {
        GLOBAL_SERVICE_STORAGE
            .set(storage)
            .map_err(|_| "全局服务存储已初始化".to_string())
    }

    /// 获取全局服务存储引用
    pub fn get() -> &'static Arc<dyn ServiceStorage> {
        GLOBAL_SERVICE_STORAGE.get().expect(
            "服务存储未初始化，请先调用 GlobalServiceStorage::set()"
        )
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_SERVICE_STORAGE.get().is_some()
    }
}

/// 全局服务注册中心访问器
pub struct GlobalServiceRegistry;

impl GlobalServiceRegistry {
    /// 设置全局服务注册中心
    pub fn set(registry: Arc<ServiceRegistry>) -> Result<(), String> {
        GLOBAL_SERVICE_REGISTRY
            .set(registry)
            .map_err(|_| "全局服务注册中心已初始化".to_string())
    }

    /// 获取全局服务注册中心引用
    pub fn get() -> &'static Arc<ServiceRegistry> {
        GLOBAL_SERVICE_REGISTRY.get().expect(
            "服务注册中心未初始化，请先调用 GlobalServiceRegistry::set()"
        )
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_SERVICE_REGISTRY.get().is_some()
    }
}
