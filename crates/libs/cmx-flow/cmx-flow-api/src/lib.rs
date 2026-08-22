//! cmx-flow-api —— 流程引擎的**平台反代薄壳**（对标 cmx-rule-api）。
//!
//! 流程引擎是**独立微服务**：引擎核 [`cmx_flow_app`] 在独立 workspace `../../cmx-flowengine`，由
//! 那边的 `cmx-flow-server` 承载。门户不再进程内嵌引擎，故本 crate **不依赖 `cmx-flow-app`**——
//! 只含反代：[`FlowProxyModule`]（impl cmx-api 的 `ModuleRoutes`）把平台 `/api/flow/*` 透明转发到
//! 远程 flow-server；[`with_flow_page_proxy`] 把流程拥有的 native/html 页取页请求反代过去。
//! 切换只看 `[center_client]` 的服务定位配置（mode 驱动：http_url 看 `urls.flow`、
//! http_discovery/grpc 看 `discovery.services.flow`）——配了才挂流程路由，前端零改。
//! 目标经 [`UpstreamResolver`] 按请求动态解析，无可用实例 → 503。

// S6：反代壳（引擎在远程独立 flow-server；见 proxy.rs）。
mod proxy;
pub use proxy::{FlowProxyModule, UpstreamResolver, with_flow_page_proxy};
