//! cmx-flow-api —— 流程引擎的**平台适配薄壳**（抽核后仅剩装配）。
//!
//! 业务全部在平台中立核 [`cmx_flow_app`]（引擎单例 + handler + 路由 + 信封）。本 crate 只把
//! core 的 [`cmx_flow_app::flow_routes`] 包成 `FlowModule`（impl cmx-api 的 `ModuleRoutes`），
//! 由 web-server 合并进主路由；并 re-export [`spawn_timer_poller`] 供 main 启动。
//! cmx-api 不反向依赖本 crate（无环）。端点前缀 `/flow/*`（`/api` 前缀由 web-server nest 加）。
//!
//! 独立部署形态见 `../../cmx-flowengine` 的 `cmx-flow-server`——同一个 core，另一层壳。

use axum::Router;

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

// 引擎定时器 poller 由 core 提供；main.rs 直接调 cmx_flow_api::spawn_timer_poller()。
pub use cmx_flow_app::spawn_timer_poller;

// S6：反代壳（引擎在远程独立 flow-server 时用；见 proxy.rs）。
mod proxy;
pub use proxy::{FlowProxyModule, with_flow_page_proxy};

/// 流程模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
///
/// 路由表 + handler 全来自 core 的 [`cmx_flow_app::flow_routes`]，对 `CmxAppState` 泛型实例化。
pub struct FlowModule;

impl ModuleRoutes for FlowModule {
    fn routes(self) -> Router<CmxAppState> {
        cmx_flow_app::flow_routes::<CmxAppState>()
    }

    fn prefix() -> &'static str {
        "flow"
    }

    fn module_name(&self) -> &'static str {
        "flow"
    }
}
