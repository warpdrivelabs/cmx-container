//! cmx-onto-api —— 本体平台的**平台反代薄壳**（对标 cmx-rule-api）。
//!
//! 本体平台是**独立微服务**（不像 flow/report 有进程内嵌壳，与 rules 同构），故本 crate 只有反代：
//! `OntoProxyModule`（impl [`ModuleRoutes`]）把平台 `/api/onto/*` 透明转发到远程 cmx-onto-server；
//! `with_onto_page_proxy` 把本体拥有的 native 页（`portal.onto.*`）取页请求反代过去。切换只看
//! `[center_client]` 的服务定位配置（per-key：`services.onto` 配 url 静态基址或 discovery Nacos 选例）
//! ——配了才挂本体路由，前端零改。目标经 [`UpstreamResolver`](proxy::UpstreamResolver) 按请求动态解析，
//! 无可用实例 → 503。

mod proxy;
pub use proxy::{with_onto_page_proxy, OntoProxyModule, UpstreamResolver};
