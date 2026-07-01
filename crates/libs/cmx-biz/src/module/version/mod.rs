//! 模块版本管理子模块
//!
//! 管理 cmx_module_current_version(当前态,每模块一行)
//! 与 cmx_module_version_history(完整导入历史)两张表。

pub mod bmc;
pub mod service;

pub use bmc::{ModuleCurrentVersionBmc, ModuleVersionHistoryBmc};
pub use service::{ModuleVersionRecord, ModuleVersionService};
