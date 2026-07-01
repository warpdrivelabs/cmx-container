//! 上下文档案（ContextProfile）—— 设计期元数据 + 运行时规则引擎。
//!
//! 复刻：
//! - Node `lib/contextProfileStore.js`（[`store`]）：list/config/save/delete 文件 CRUD。
//! - `cmx-data-comp` 的 `context-profile-engine.js`（[`engine`]）：resolveMergedRule 评分合并、
//!   buildColumns（_fieldToColumn 全派生 → CmxColumn.toJSON）、buildMembers（分组）、buildColumnModelProps。
//! - `context-profile-validator.js`（[`validator`]）：domain-neutral schema 校验。
//! - 处理器侧 `enrichContextProfileDictMeta`（[`dict_meta`]）：维度 dict 元数据补全。

pub mod api;
pub mod dict_meta;
pub mod engine;
pub mod store;
pub mod validator;
