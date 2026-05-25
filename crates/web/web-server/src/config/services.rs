//! 服务管理器模块
//!
//! 提供服务仓储、注册中心和生命周期监听器的初始化功能。

use cmx_runtime::RuntimeLifecycleListener;
use cmx_service::{
    GlobalServiceQuery, GlobalServiceRegistry, GlobalServiceStorage, ServiceLifecycleListener,
    ServiceQueryImpl, ServiceRegistry, ServiceRepository, ServiceStorageImpl,
};
use cmx_traits::{ServiceQuery, ServiceStorage};
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

    // 从配置读取 app_id，默认值为 "default"
    // let app_id = ConfigManager::global()
    //     .get_string("plugin.app_id")
    //     .unwrap_or("default".to_string());
    let app_id = std::env::var("NACOS_NAMING_SERVICE_NAME").unwrap_or("default".to_string());

    let repository = Arc::new(ServiceRepository::new(db_manager.clone(), default_db_id));
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
