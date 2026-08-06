//! 编码引擎 DB 访问层。
//!
//! - [`rule_store`]：`cmx_code_rule` 表 CRUD + 规则选优。
//! - [`serial_pg`]：反查 max SQL 实现（impl Advance trait）+ minted_buffer union。
//! - [`gap_store`]：`cmx_code_gap` 断号表读写（连号域 enable_gap=true 启用）。

pub mod gap_store;
pub mod rule_store;
pub mod serial_pg;

pub use serial_pg::PgAdvance;
