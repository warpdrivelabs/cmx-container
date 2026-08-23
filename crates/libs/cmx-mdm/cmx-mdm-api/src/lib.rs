//! cmx-mdm-api —— 主数据（MDM）模块的**平台反代薄壳**（对标 cmx-rpt-api / cmx-flow-api）。
//!
//! 主数据治理是**独立微服务**：中立核 [`cmx_mdm_app`] 在独立 workspace `../cmx-mdm`，由那边的
//! `cmx-mdm-server` 承载。门户不再进程内嵌主数据治理，故本 crate **不依赖 `cmx-mdm-app`**——
//! 只含反代：[`MdmProxyModule`]（impl cmx-api-core 的 `ModuleRoutes`）把平台主数据 API 透明转发
//! 到远程 cmx-mdm-server；[`with_mdm_page_proxy`] 把主数据拥有的 native 页取页请求反代过去。
//! 切换只看 `[center_client]` 的服务定位配置（per-key：`services.mdm` 配 url 静态基址或
//! discovery Nacos 选例）——配了才挂主数据路由，前端零改。
//! 目标经 [`UpstreamResolver`] 按请求动态解析，无可用实例 → 503。

mod proxy;

pub use proxy::{MdmProxyModule, UpstreamResolver, with_mdm_page_proxy};
