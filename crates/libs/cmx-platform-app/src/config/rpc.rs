//! RPC 组装层（portal 专属注入）。
//!
//! 通用 gRPC 服务端子系统（`init_rpc` / `load_rpc_config` / `load_service_auth_config` /
//! `ServiceAuthConfig`）已下沉公用包 [`cmx_service_base::rpc`]（设施本体在 cmx-service-rpc
//! 的 grpc-server feature）。本模块只保留**绑业务层、不能进公用包**的组装函数：
//! - [`build_function_invoker`]：构造 cmx-biz 的 `BizFunctionInvoker`（绑 cmx-biz），注入 `init_rpc`。
//! - [`load_outgoing_credential`]：读服务对外凭证（反代出站注入用）。
//!
//! 二者都复用公用包的 [`cmx_service_base::load_service_auth_config`] 读 `[service_auth]` 段。

use std::sync::Arc;

use cmx_traits::function_invoker::FunctionInvoker;

/// 构造 cmx-biz 的 `BizFunctionInvoker`（封装 `RuntimeInvoker` + `PluginQuery`）。
///
/// 组装层在此构造并注入公用包的 [`cmx_service_base::init_rpc`]，使基础设施层 cmx-rpc
/// 无需直接依赖业务层 cmx-biz。返回值透传给 `init_rpc` 的 `function_invoker` 参数。
pub fn build_function_invoker() -> Arc<dyn FunctionInvoker> {
    Arc::new(cmx_biz::function_invoker::BizFunctionInvoker::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
    ))
}

/// 读取服务对外凭证（供反代壳出站注入 `X-API-Key`；基座服务间调用自行读取，不经此）。
///
/// 复用公用包 [`cmx_service_base::load_service_auth_config`] 读 `[service_auth]` 段；
/// 未配置时返回 `None`。
pub(crate) fn load_outgoing_credential() -> Option<String> {
    let cfg = cmx_service_base::load_service_auth_config();
    if cfg.outgoing_api_key.is_empty() {
        None
    } else {
        Some(cfg.outgoing_api_key)
    }
}
