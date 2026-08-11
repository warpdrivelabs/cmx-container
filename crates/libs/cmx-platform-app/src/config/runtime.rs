//! WASM 运行时模块（portal 侧）。
//!
//! 通用部分（建引擎 + logging/db/buffer 三通用 provider + 设全局）已下沉 cmx-service-base。
//! 本模块只保留 **portal 专属的两个 host-fn provider 构造**——`cmx:iam`（cmx-iam）+ plugin
//! （cmx-plugin）——经 `init_wasm(extra_providers)` 注入。这样公用包不碰 cmx-iam/cmx-plugin。

use std::sync::Arc;

use cmx_database::get_default_db_manager;
use cmx_iam::{IamChecker, IamHostFunctions, UserAuthQueryImpl};
use cmx_plugin::PluginHostFunctions;
use cmx_traits::auth::UserAuthQuery;
use cmx_traits::iam::PermissionChecker;
use cmx_traits::runtime::HostFunctionProvider;

pub use crate::Error;

/// 初始化 WASM 运行时：构造 portal 专属的 iam/plugin provider，注入公用包的 `init_wasm`。
///
/// 必须在 init_cache/init_datasources 之后（provider 需 DB manager）、init_plugins 之前
/// （插件加载时宿主函数须已注入）。
pub async fn init_runtime() -> crate::Result<()> {
    let db_manager = get_default_db_manager();

    // plugin provider（无额外依赖）
    let plugin_provider: Arc<dyn HostFunctionProvider> = Arc::new(PluginHostFunctions::new());

    // cmx:iam provider（用户详情/角色权限/权限校验）：构造轻量 IamChecker + UserAuthQueryImpl，
    // 与 init_iam_services 后续实例共享同一连接池。
    let iam_config = crate::config::iam::load_iam_config();
    let iam_db_id = match iam_config.auth_db_id.clone() {
        Some(id) => id,
        None => db_manager.get_default_db_id().await,
    };
    let user_auth_query: Arc<dyn UserAuthQuery> = Arc::new(
        UserAuthQueryImpl::new(db_manager.clone(), &iam_config)
            .await
            .map_err(|e| Error::RuntimeInit(format!("UserAuthQueryImpl 初始化失败: {e}")))?,
    );
    let iam_checker: Arc<dyn PermissionChecker> =
        Arc::new(IamChecker::new(db_manager.clone(), iam_config).await);
    let iam_provider: Arc<dyn HostFunctionProvider> = Arc::new(IamHostFunctions::new(
        iam_checker,
        user_auth_query,
        db_manager.clone(),
        iam_db_id,
    ));

    // 注入公用包：通用 3 provider（logging/db/buffer）+ portal 注入的 plugin/iam 2 个 = 5 个。
    cmx_service_base::init_wasm(vec![plugin_provider, iam_provider])
        .await
        .map_err(|e| Error::RuntimeInit(format!("WASM 运行时初始化失败: {e}")))?;

    Ok(())
}
