//! 弹性组合（FlexibleCombination）—— 设计期元数据 + 运行时规则引擎。
//!
//! 复刻：
//! - Node `lib/flexibleCombinationStore.js`（[`store`]）：list/config/save/delete 文件 CRUD。
//! - `cmx-data-comp` 的 `flexible-combination-engine.js`（[`engine`]）：resolveMergedRule 评分合并、
//!   buildColumns（_fieldToColumn 全派生 → CmxColumn.toJSON）、buildMembers（分组）、buildColumnModelProps。
//! - `flexible-combination-validator.js`（[`validator`]）：domain-neutral schema 校验。
//! - 处理器侧 `enrichFlexibleCombinationDictMeta`（[`dict_meta`]）：维度 dict 元数据补全。

pub mod api;
pub mod dict_meta;
pub mod engine;
pub mod store;
pub mod validator;
