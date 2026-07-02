//! WASM 运行时模块
//!
//! 提供 Extism 引擎和宿主函数的初始化功能。

use cmx_buffer::BufferHostFunctions;
use cmx_database::DatabaseHostFunctions;
use cmx_database::get_default_db_manager;
use cmx_iam::{IamChecker, IamHostFunctions, UserAuthQueryImpl};
use cmx_plugin::PluginHostFunctions;
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine, LoggingHostFunctions};
use cmx_traits::auth::UserAuthQuery;
use cmx_traits::iam::PermissionChecker;
use cmx_traits::runtime::{GlobalRuntime, HostFunctionProvider};
use std::sync::Arc;
use tracing::info;

pub use crate::Error;

/// 初始化 WASM 运行时。
///
/// 必须在 init_cache 之后调用。
/// 注册所有宿主函数提供者到 WASM 引擎，包括 `cmx:iam` 用户/权限查询宿主函数。
///
/// # Returns
///
/// * `Ok(())` - 运行时初始化成功
/// * `Err(Error::RuntimeInit)` - 引擎创建或宿主函数注册失败
pub async fn init_runtime() -> crate::Result<()> {
    info!("初始化 WASM 运行时...");

    let engine = Arc::new(
        ExtismEngine::new(ExtismEngineConfig::default())
            .map_err(|e| Error::RuntimeInit(format!("Extism 引擎初始化失败: {}", e)))?,
    );

    let logging_provider: Arc<dyn HostFunctionProvider> = Arc::new(LoggingHostFunctions::new());
    engine
        .register_provider(logging_provider)
        .map_err(|e| Error::RuntimeInit(format!("注册日志宿主函数失败: {}", e)))?;

    let db_manager = get_default_db_manager();
    let db_provider: Arc<dyn HostFunctionProvider> =
        Arc::new(DatabaseHostFunctions::new(db_manager.clone()));
    engine
        .register_provider(db_provider)
        .map_err(|e| Error::RuntimeInit(format!("注册数据库宿主函数失败: {}", e)))?;

    let buffer_provider: Arc<dyn HostFunctionProvider> = Arc::new(BufferHostFunctions::new());
    engine
        .register_provider(buffer_provider)
        .map_err(|e| Error::RuntimeInit(format!("注册缓存宿主函数失败: {}", e)))?;

    let plugin_provider: Arc<dyn HostFunctionProvider> = Arc::new(PluginHostFunctions::new());
    engine
        .register_provider(plugin_provider)
        .map_err(|e| Error::RuntimeInit(format!("注册插件宿主函数失败: {}", e)))?;

    // 注册 cmx:iam 宿主函数（用户详情/角色权限/权限校验/角色判断）。
    // 构造轻量级 IamChecker + UserAuthQueryImpl（仅持有 DB 连接池，无额外开销），
    // 与 init_iam_services 后续创建的实例共享同一连接池。
    // 必须在 init_plugins() 之前完成，确保插件加载时宿主函数已注入。
    let iam_config = crate::config::iam::load_iam_config();
    let iam_db_id = match iam_config.auth_db_id.clone() {
        Some(id) => id,
        None => db_manager.get_default_db_id().await,
    };
    let user_auth_query: Arc<dyn UserAuthQuery> = Arc::new(
        UserAuthQueryImpl::new(db_manager.clone(), &iam_config)
            .await
            .map_err(|e| Error::RuntimeInit(format!("UserAuthQueryImpl 初始化失败: {}", e)))?,
    );
    let iam_checker: Arc<dyn PermissionChecker> =
        Arc::new(IamChecker::new(db_manager.clone(), iam_config).await);
    let iam_provider: Arc<dyn HostFunctionProvider> = Arc::new(IamHostFunctions::new(
        iam_checker,
        user_auth_query,
        db_manager.clone(),
        iam_db_id,
    ));
    engine
        .register_provider(iam_provider)
        .map_err(|e| Error::RuntimeInit(format!("注册 IAM 宿主函数失败: {}", e)))?;

    GlobalRuntime::set(engine.clone())
        .map_err(|e| Error::RuntimeInit(format!("设置全局运行时失败: {:?}", e)))?;

    GlobalExtismEngine::initialize(engine)
        .map_err(|e| Error::RuntimeInit(format!("全局引擎初始化失败: {}", e)))?;

    info!("WASM 运行时初始化完成，已注册 5 个宿主函数提供者");

    Ok(())
}
