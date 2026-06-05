//! 服务管理器模块
//!
//! 提供服务仓储、注册中心和生命周期监听器的初始化功能。

use cmx_runtime::RuntimeLifecycleListener;
use cmx_service::{
    GlobalServiceQuery, GlobalServiceRegistry, GlobalServiceStorage, ServiceInvokerImpl,
    ServiceLifecycleListener, ServiceQueryImpl, ServiceRegistry, ServiceRepository,
    ServiceStorageImpl,
};
use cmx_traits::{GlobalServiceInvoker, ServiceQuery, ServiceStorage};
use cmx_database::get_default_db_manager;
use std::sync::Arc;
use tracing::info;
pub use crate::Error;

/// 初始化服务管理器。
///
/// 初始化服务相关的组件，服务数据采用延迟加载策略：
/// - 不在启动时全量加载所有服务
/// - 首次访问服务时才从数据库加载并缓存
///
/// 这样做的好处：
/// - 加快服务启动速度，特别是服务数量较多时
/// - 避免启动时执行大量数据库查询（1+2N 次）
///
/// # Returns
///
/// * `Ok(())` - 服务管理器初始化成功
/// * `Err(Error::ServiceInit)` - 服务管理器初始化失败
pub async fn init_services() -> crate::Result<()> {
    info!("初始化服务管理器...");

    let db_manager = get_default_db_manager();
    let default_db_id = get_default_db_manager().get_default_db_id().await;

    //fixme yqs 不使用nacos的时候要修改
    let app_id = cmx_utils::ConfigManager::global()
        .get_string("app.id")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("APP_ID").ok())
        .or_else(|| std::env::var("SERVICE_REGISTRY_NAME").ok())
        .or_else(|| std::env::var("NACOS_NAMING_SERVICE_NAME").ok())
        .unwrap_or_else(|| "default".to_string());

    let repository = Arc::new(ServiceRepository::new(db_manager.clone(), default_db_id.clone()));
    let registry = Arc::new(ServiceRegistry::new());

    GlobalServiceRegistry::set(registry.clone())
        .map_err(|e| Error::ServiceInit(format!("初始化服务注册中心失败: {}", e)))?;

    let service_query = Arc::new(ServiceQueryImpl::new(repository.clone(), registry.clone(), app_id.clone())) as Arc<dyn ServiceQuery>;
    let service_storage = Arc::new(ServiceStorageImpl::new(repository.clone())) as Arc<dyn ServiceStorage>;

    GlobalServiceQuery::set(service_query.clone())
        .map_err(|e| Error::ServiceInit(format!("初始化服务查询器失败: {}", e)))?;
    GlobalServiceStorage::set(service_storage.clone())
        .map_err(|e| Error::ServiceInit(format!("初始化服务存储失败: {}", e)))?;

    info!("服务仓储使用数据库ID: {}", repository.get_default_db_id());
    info!("服务数据采用延迟加载策略，首次访问时自动加载");

    let service_listener = ServiceLifecycleListener::new(
        GlobalServiceQuery::get().clone(),
        repository.clone(),
        GlobalServiceRegistry::get().clone(),
        app_id.clone(),
    );
    service_listener.register().await;

    let runtime_listener = RuntimeLifecycleListener::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        app_id,
    );
    runtime_listener.register().await;

    info!("生命周期监听器已注册");
    info!("服务管理器初始化完成");

    Ok(())
}

/// 初始化全局服务调用器。
///
/// 在 init_plugins() 之后调用，因为依赖 GlobalPluginManager。
/// 组合 GlobalRuntime + GlobalPluginManager + GlobalServiceQuery，
/// 为宿主函数提供调用服务编排的能力。
///
/// # Returns
///
/// * `Ok(())` - 服务调用器初始化成功
/// * `Err(Error::ServiceInit)` - 服务调用器初始化失败
pub async fn init_service_invoker() -> crate::Result<()> {
    info!("初始化全局服务调用器...");

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let service_invoker = Arc::new(ServiceInvokerImpl::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
        GlobalServiceQuery::get().clone(),
        default_db_id,
    ));
    GlobalServiceInvoker::set(service_invoker)
        .map_err(|e| Error::ServiceInit(format!("初始化服务调用器失败: {:?}", e)))?;

    info!("全局服务调用器初始化完成");
    Ok(())
}
