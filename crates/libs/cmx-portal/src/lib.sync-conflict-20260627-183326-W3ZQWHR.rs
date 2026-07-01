//! cmx-portal —— 门户/设计器业务层
//!
//! 承接 CMXPortalManager / CMXHTMLDesigner 两个 Node.js 后端迁移而来的全部业务。
//! 数据为 JSON 文件存储（`data/**.json`），通过 `tokio::fs` 读写，原子写用「临时文件 + rename」。
//!
//! 模块组织（按资源域）：
//! - [`meta`]   —— menu/activities/domains/registry/dam_registry/modules/service_catalog/workspace_nodes
//! - `pages`    —— html/form/native（后续阶段）
//! - `dict`     —— 字典检索引擎（后续阶段）
//! - `context_profile` —— 上下文档案规则引擎（后续阶段）
//! - `definitions` / `fact` / `agent` / `ai`（后续阶段）

pub mod agent;
pub mod ai;
pub mod config;
pub mod context_profile;
pub mod dam;
pub mod definitions;
pub mod dict;
pub mod error;
pub mod fact;
pub mod fsutil;
pub mod meta;
pub mod pages;
pub mod service_catalog;
pub mod util;

pub use config::data_root;
pub use error::{PortalError, PortalResult};
