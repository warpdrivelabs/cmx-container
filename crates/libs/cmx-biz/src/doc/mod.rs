//! 业务单据数据服务（方案落地 Phase 2–5）
//!
//! - `meta`：DocMetaView 强类型定义投影（§3.8）
//! - `cache`：DocMetaView 进程内缓存（§3.7）
//! - `loader`：DocLoader 装载（§5.1，Phase 3）
//! - `saver`：DocSaver 回存双模式（§6.4，Phase 5）

pub mod cache;
pub mod formula;
pub mod loader;
pub mod meta;
pub mod revision;
pub mod rule;
pub mod saver;

pub use formula::{eval_bool, eval_formula, scope_from_json, FValue, Scope};
pub use loader::{DocLoader, LoadOptions};
pub use meta::{ColumnView, DocMetaView, LayerView, RelationView};
pub use revision::DocRevision;
pub use rule::{validate, ValidateResult, Violation};
pub use saver::{DocSaver, SaveMode, SaveResult};
