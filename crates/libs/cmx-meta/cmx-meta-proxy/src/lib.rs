//! cmx-meta-proxy —— 元数据管理的**平台反代薄壳**（对标 cmx-model-proxy）。
//!
//! 元数据管理全新独立微服务：中立核 [`cmx_meta_app`] 在独立 workspace `../cmx-meta-data`，由
//! `cmx-meta-server`（:8096）承载。门户经 `[service_rpc.services].meta` 决定：配了 = 反代到独立
//! 微服务（本壳）；没配 = 门户不挂元数据路由（本服务无进程内嵌兜底）。本 crate 只含反代：
//! [`MetaProxyModule`]（impl cmx-api-core 的 `ModuleRoutes`）把 `/api/meta/*` 透明转发到远程
//! cmx-meta-server；[`with_meta_page_proxy`] 把 `meta.*` native/html 页取页请求反代过去。前端零改。

mod proxy;
pub use proxy::{MetaProxyModule, UpstreamResolver, with_meta_page_proxy};
