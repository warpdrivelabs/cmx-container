//! cmx-rule-api —— 决策规则引擎的**平台反代薄壳**（对标 cmx-rpt-api）。
//!
//! 规则引擎是**独立微服务**（不像 flow/report 有进程内嵌壳），故本 crate 只有反代：
//! `RulesProxyModule`（impl [`ModuleRoutes`]）把平台 `/api/rules/*` 透明转发到远程
//! cmx-rule-server；`with_rules_page_proxy` 把规则拥有的 native 页（`portal.rules.*`）取页请求
//! 反代过去。切换只看 `[center_client]` 的服务定位配置（mode 驱动：http_url 看 `urls.rules`、
//! http_discovery/grpc 看 `discovery.services.rules`）——配了才挂规则路由，前端零改。
//! 目标经 [`UpstreamResolver`](proxy::UpstreamResolver) 按请求动态解析，无可用实例 → 503。

mod proxy;
pub use proxy::{UpstreamResolver, RulesProxyModule, with_rules_page_proxy};
