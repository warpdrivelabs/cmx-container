//! cmx-doc-model —— 业务单据（DOC）模块的语义中立层（DB-free）。
//!
//! 把 `definitions::store` 读入的弱类型单据定义 JSON 解析为强类型模型，供上层
//! cmx-doc-store-pg 执行装载/回存：
//! - `meta`：DocMetaView 强类型定义投影（层序/各层列 caption·类型/父子关系）。
//! - `query`：DocQuery 富查询模型（每层条件/排序/分页/游标）。
//! - `formula`：单据公式求值（Scope/FValue，rule 依赖）。
//! - `rule`：落库前业务规则校验（T1 公式校验）。
//! - `sql_builder`：层级 SELECT 生成（$N 占位，tokio-postgres 与 sqlx 通用）。
//!
//! 跨文件复用工具：
//! - `codec`：JSON ↔ DataValue 类型化转换(消除 saver/query/revision 三份重复)。
//! - `datetime_util`：日期/时间解析归一(RFC3339 + 无时区兼容)。
//! - `error`：本 crate 内部错误精度(ModelError / FormulaError),对外经 BizError 桥接。

pub mod codec;
pub mod datetime_util;
pub mod error;
pub mod formula;
pub mod meta;
pub mod query;
pub mod rule;
pub mod sql_builder;

pub use codec::{dv_to_json, json_to_dv_loose, json_to_dv_typed};
pub use datetime_util::{parse_datetime_utc, parse_naive_date};
pub use error::{FormulaError, ModelError};
pub use formula::{FValue, Scope, eval_bool, eval_formula, scope_from_json};
pub use meta::{ColumnView, DocMetaView, LayerView, LevelGroup, RelationView, SummaryView};
pub use query::{Cond, Cursor, DocQuery, Filter, LayerQuery, Op, OrderBy, json_to_datavalue};
pub use rule::{ValidateResult, Violation, validate};
pub use sql_builder::build_layer_select;
