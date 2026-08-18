//! cmx-rpt-api —— 报表模块的**平台反代薄壳**（对标 cmx-rule-api / cmx-flow-api）。
//!
//! 报表引擎是**独立微服务**：中立核 [`cmx_rpt_app`] 在独立 workspace `../cmx-report`，由那边的
//! `cmx-rpt-server` 承载。门户不再进程内嵌报表，故本 crate **不依赖 `cmx-rpt-app`**——只含反代：
//! [`ReportProxyModule`]（impl cmx-api-core 的 `ModuleRoutes`）把平台报表 API 透明转发到远程
//! cmx-rpt-server；[`with_report_page_proxy`] 把报表拥有的 native/html 页取页请求反代过去。
//! 切换只看 `[center_client.urls].report` 配没配——配了才挂报表路由，前端零改。

mod proxy;
pub use proxy::{ReportProxyModule, with_report_page_proxy};
