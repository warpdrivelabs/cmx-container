//! cmx-model —— 模型中心。
//!
//! 承接「meta 相关」元数据建模服务（迁移自 CMXPortalManager 的 Node 后端），即模型中心的
//! 五项元数据建模能力（BASE-DCT / BASE-DOC / DCT / DOC / FC）：
//! - [`definitions`]      —— DCT/DOC/BASE 定义中心（数据存 `data/meta/definitions/**`）。
//! - [`flexible_combination`]  —— 弹性组合规则引擎（数据存 `data/meta/flexible-combination`）。
//! - [`dict`]             —— 字典检索引擎（模型中心的字典能力，被 `flexible_combination` 依赖）。
//!
//! 基础设施（config/error/fsutil/util）从 [`cmx_portal_base`] 再导出，使被移动代码里既有的
//! `crate::config` / `crate::error` / `crate::fsutil` / `crate::util` 路径无需改动即可解析。

// 32 行的递归 derive 嵌套较深（HashMap/BTreeSet/serde_json::Value 多层泛型 + #[serde(flatten)]）,
// 默认 128 上限在编译期会触发 `recursion limit reached` 错误,故放宽到 256。
#![recursion_limit = "256"]

// 基础设施再导出：保持被移动代码里的 crate::{config,error,fsutil,util} 路径有效。
pub use cmx_portal_base::{config, error, fsutil, util};

pub mod definitions;
pub mod dict;
pub mod flexible_combination;
