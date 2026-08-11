//! RPC 组装层（portal 专属注入）。
//!
//! 通用 RPC 子系统（`init_rpc` / `load_rpc_config` / `load_service_auth_config` / `ServiceAuthConfig`）
//! 已下沉公用包 [`cmx_service_base::rpc`]。本模块只保留**绑业务层、不能进公用包**的两个组装函数：
//! - [`build_function_invoker`]：构造 cmx-biz 的 `BizFunctionInvoker`（绑 cmx-biz），注入 `init_rpc`。
//! - [`load_outgoing_credential`]：构造 cmx-plugin 的 `Credential`（绑 cmx-plugin），供 HTTP 出站注入。
//!
//! 二者都复用公用包的 [`cmx_service_base::load_service_auth_config`] 读 `[service_auth]` 段，
//! 使公用包无需依赖 cmx-biz/cmx-plugin。

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

/// 读取服务对外凭证（供 web-server 其他装配点使用，如 HTTP 出站注入）。
///
/// 复用公用包 [`cmx_service_base::load_service_auth_config`] 读 `[service_auth]` 段，
/// 包成 cmx-plugin 的 `Credential`（服务级 API Key，统一走 `X-API-Key`）；
/// 未配置时返回 `None`。
pub(crate) fn load_outgoing_credential() -> Option<cmx_plugin::service::remote_importers::Credential>
{
    let cfg = cmx_service_base::load_service_auth_config();
    if cfg.outgoing_api_key.is_empty() {
        None
    } else {
        Some(cmx_plugin::service::remote_importers::Credential {
            value: cfg.outgoing_api_key,
        })
    }
}
