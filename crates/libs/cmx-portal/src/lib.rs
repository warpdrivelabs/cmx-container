//! cmx-portal —— 门户/设计器业务层（门面 + 门户本体）。
//!
//! 承接 CMXPortalManager / CMXHTMLDesigner 两个 Node.js 后端迁移而来的门户业务。
//! 数据为 JSON 文件存储（`data/**.json`），通过 `tokio::fs` 读写，原子写用「临时文件 + rename」。
//!
//! 本 crate 已按业务边界拆分，物理代码分布在三个 crate，但对外 API 路径保持不变（re-export 门面）：
//! - 基础设施下沉至 [`cmx_portal_base`]（config / error / fsutil / util）。
//! - 表单中心拆至 [`cmx_form`]（`pages`：form / html / native）。
//! - 模型中心拆至 [`cmx_model`]（`definitions` / `context_profile` / `dict`）。
//!
//! 下列 `pub use` 把基础设施与两个子中心再导出回本 crate 命名空间，于是
//! `cmx_portal::pages::*` / `cmx_portal::definitions::*` / `cmx_portal::PortalError` 等旧路径
//! 以及 `agent` 内部的 `crate::pages` / `crate::context_profile` 引用均无需改动。
//!
//! 仍属门户本体的资源域（保留在本 crate）：
//! - [`meta`]   —— menu/activities/domains/registry/dam_registry/modules/workspace_nodes（门户导航元数据）。
//! - [`dam`]    —— DAM 注册表。
//! - [`fact`] / [`help`] / [`launcher`] / [`notify`] / [`service_catalog`]。
//! - [`agent`] / [`ai`] —— AI 本地编辑代理 / 对话中继。

#![recursion_limit = "256"]

// 基础设施从 base 再导出：保持 crate::config / crate::PortalError / cmx_portal::data_root 等旧路径。
pub use cmx_portal_base::{config, data_root, error, fsutil, util, PortalError, PortalResult};

// 拆出的子中心再导出：保持 cmx_portal::pages / ::definitions / ::context_profile / ::dict
// 以及 agent 内部 crate::pages 等引用有效。
pub use cmx_form::pages;
pub use cmx_model::{context_profile, definitions, dict};

// 仍属门户本体的资源域。
pub mod agent;
pub mod ai;
pub mod dam;
pub mod fact;
pub mod help;
pub mod launcher;
pub mod meta;
pub mod notify;
pub mod service_catalog;
