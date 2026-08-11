//! 服务管理器 + 调用器初始化（feature `services`）。
//!
//! 从 web-server `config/services.rs` 原样提取：服务仓储/注册中心/生命周期监听器（延迟加载策略）。
//! `init_service_invoker` 组合 runtime + plugin + service query，**须在 wasm + plugins 之后调**。纯基础设施。

use std::sync::Arc;

use cmx_database::get_default_db_manager;
use cmx_runtime::RuntimeLifecycleListener;
use cmx_service::{
    GlobalServiceQuery, GlobalServiceRegistry, GlobalServiceStorage, ServiceInvokerImpl,
    ServiceLifecycleListener, ServiceQueryImpl, ServiceRegistry, ServiceRepository,
    ServiceStorageImpl,
};
use cmx_traits::service::{GlobalServiceInvoker, ServiceQuery, ServiceStorage};
use tracing::info;

use crate::{BaseError, Result};

/// 初始化服务管理器（服务数据延迟加载）。
pub async fn init_services() -> Result<()> {
    info!("初始化服务管理器...");

    let db_manager = get_default_db_manager();
    let default_db_id = get_default_db_manager().get_default_db_id().await;

    let app_id = cmx_utils::ConfigManager::global().get_app_id();

    let repository = Arc::new(ServiceRepository::new(
        db_manager.clone(),
        default_db_id.clone(),
    ));
    let registry = Arc::new(ServiceRegistry::new());

    GlobalServiceRegistry::set(registry.clone())
        .map_err(|e| BaseError::Setup(format!("初始化服务注册中心失败: {e}")))?;

    let service_query = Arc::new(ServiceQueryImpl::new(
        repository.clone(),
        registry.clone(),
        app_id.clone(),
    )) as Arc<dyn ServiceQuery>;
    let service_storage =
        Arc::new(ServiceStorageImpl::new(repository.clone())) as Arc<dyn ServiceStorage>;

    GlobalServiceQuery::set(service_query.clone())
        .map_err(|e| BaseError::Setup(format!("初始化服务查询器失败: {e}")))?;
    GlobalServiceStorage::set(service_storage.clone())
        .map_err(|e| BaseError::Setup(format!("初始化服务存储失败: {e}")))?;

    info!("服务仓储使用数据库ID: {}", repository.get_default_db_id());
    info!("服务数据采用延迟加载策略，首次访问时自动加载");

    let service_listener = ServiceLifecycleListener::new(
        GlobalServiceQuery::get().clone(),
        repository.clone(),
        GlobalServiceRegistry::get().clone(),
        app_id.clone(),
    );
    service_listener.register().await;

    let runtime_listener =
        RuntimeLifecycleListener::new(cmx_runtime::GlobalExtismEngine::get_as_invoker(), app_id);
    runtime_listener.register().await;

    info!("生命周期监听器已注册");
    info!("服务管理器初始化完成");

    Ok(())
}

/// 初始化全局服务调用器（组合 runtime + plugin + service query）。**须在 plugins 之后调**。
pub async fn init_service_invoker() -> Result<()> {
    info!("初始化全局服务调用器...");

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let service_invoker = Arc::new(ServiceInvokerImpl::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
        GlobalServiceQuery::get().clone(),
        default_db_id,
    ));
    GlobalServiceInvoker::set(service_invoker)
        .map_err(|e| BaseError::Setup(format!("初始化服务调用器失败: {e:?}")))?;

    info!("全局服务调用器初始化完成");
    Ok(())
}
