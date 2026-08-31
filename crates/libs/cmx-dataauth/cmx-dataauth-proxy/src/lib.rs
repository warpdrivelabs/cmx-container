//! cmx-dataauth-proxy —— 数据权限引擎的**平台反代薄壳**（对标 cmx-meta-proxy）。
//!
//! 数据权限是独立微服务：中立核 `cmx_dataauth_app` 在独立 workspace `../cmx-data-auth`，由
//! `cmx-dataauth-server` 承载。门户经 `[center_client.services].dataauth` 决定：配了 = 反代到独立
//! 微服务（本壳）；没配 = 门户不挂数据权限路由（本服务无进程内嵌兜底）。本 crate 只含反代：
//! [`DataAuthProxyModule`]（impl cmx-api-core 的 `ModuleRoutes`）把 `/api/dataauth/*` 透明转发到远程
//! cmx-dataauth-server；[`console_routes`] 把顶层 `/console` 管理工作台页反代过去。前端零改。

mod proxy;
pub use proxy::{console_routes, DataAuthProxyModule, UpstreamResolver};
