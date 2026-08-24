//! cmx-mdm-proxy —— 主数据（MDM）的**平台反代薄壳**（对标 cmx-model-proxy / cmx-rpt-api）。
//!
//! MDM 已抽为独立微服务：中立核 [`cmx_mdm_app`] 在独立 workspace `../cmx-mdm`，由 `cmx-mdm-server`
//! （:8095）承载。门户经 `[center_client.services].mdm` 决定：配了 = 反代到独立微服务（本壳）；没配
//! = 门户进程内嵌（迁移期兜底）。本 crate 只含反代：[`MdmProxyModule`] 把 `/api/mdm/*` 透明转发；
//! [`with_mdm_page_proxy`] 把 `portal.mdm.*` native 页取页请求反代过去。目标经 [`UpstreamResolver`]
//! 动态解析，无可用实例 → 503。前端零改。

mod proxy;
pub use proxy::{MdmProxyModule, UpstreamResolver, with_mdm_page_proxy};
