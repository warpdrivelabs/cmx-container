//! cmx-model-proxy —— 模型中心的**平台反代薄壳**（对标 cmx-rpt-api / cmx-rule-api / cmx-flow-api）。
//!
//! 模型中心已抽为**独立微服务**：中立核 [`cmx_model_app`] 在独立 workspace `../cmx-model`，由那边的
//! `cmx-model-server`（:8093）承载。门户经 `[center_client.services].model` 决定：配了 = 反代到独立
//! 微服务（本壳）；没配 = 门户回退**进程内嵌**（原 Dct/Doc/Model/Code 模块仍在 cmx-container，作平滑
//! 迁移期兜底）。本 crate 只含反代：[`ModelProxyModule`]（impl cmx-api-core 的 `ModuleRoutes`）把模型
//! 中心 API 透明转发到远程 cmx-model-server；[`with_model_page_proxy`] 把模型中心拥有的 native/html
//! 页取页请求反代过去。目标经 [`UpstreamResolver`] 按请求动态解析，无可用实例 → 503。前端零改。

mod proxy;
pub use proxy::{ModelProxyModule, UpstreamResolver, with_model_page_proxy};
