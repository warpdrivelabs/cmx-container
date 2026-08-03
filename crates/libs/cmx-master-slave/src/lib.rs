//! # cmx-master-slave —— 后端主从协调器
//!
//! 前端 [`CmxMasterSlave`](https://.../cmx-master-slave.js) 的 Rust 对等物：一个**业务无感知**、
//! **任意层级**的主从协调器。只认 schema 路径树 + relations + aggregations，不认任何
//! `cv_*`/`cf_*`/业务术语。
//!
//! ## 依赖方向（与前端同姿态）
//! 本 crate 是近叶子——只依赖 [`cmx_rowsource`]（ZmcDataSet）+ serde 系。**是服务依赖它，
//! 不是它依赖服务**：`cmx-doc-store-pg` / `cmx-dct-store-pg` 等去 `impl` 本 crate 的
//! [`HierService`]，把协调器接到它们现成的加载/保存上。前端换 source 是换 JS 适配器，
//! 后端换服务是换 `impl HierService`——协调器一字不改。
//!
//! ## 数据模型
//! 加载态数据统一用 [`ZmcDataSet`](cmx_rowsource::ZmcDataSet)（零拷贝，列式包与前端
//! `CmxDataSet.fromJSON` 同 wire）。写入态用 [`ChangeSet`]（JSON，与前端
//! `ChangeSetCollector.export()` 同结构）。
//!
//! ## 汇总（= 统一建模方案点名的 cmx-agg）
//! [`agg`] 模块是层间汇总引擎，语义与前端 `AGG_FUNCS` + `_cascade` 逐字对齐，作为
//! **服务端权威**：saver 落库前对变更集 [`rollup_changeset`](agg::rollup_changeset)，
//! 承接字段随子层一并 UPSERT。

pub mod agg;
pub mod changeset;
pub mod coordinator;
pub mod error;
pub mod schema;
pub mod service;
pub mod tree;
pub mod value;

pub use changeset::{ChangeSet, LayerChanges, SaveOutcome};
pub use coordinator::CmxMasterSlave;
pub use error::{MsError, Result};
pub use schema::{AggFn, AggRule, DerivedCols, HierSchema, LayerDef, RelationDef, Scope, Shape};
pub use service::{HierService, LoadQuery};
pub use tree::{MsTree, NodeId};

/// 版本号。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
