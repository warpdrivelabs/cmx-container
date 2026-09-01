//! cmx-code-api —— 编码引擎库（段定义/规则算法/铸号/规则库 CRUD）。
//!
//! HTTP 层已迁至 `cmx-model-app`（平台中立应用层，对任意 axum state `S` 泛型成立，
//! 不依赖 cmx-api-core）。本 crate 现为纯库：暴露 `engine`（铸号引擎 + `CodeMinter`
//! 实现）、`store`（规则库/断号/序列表 DB 访问）与 `handlers`（仅保留被 engine/store
//! 复用的 [`handlers::Dam`] 三维标识）。迁出的 handler 经 `cmx_code_api::{engine, store,
//! handlers::Dam}` 引用本库。

pub mod engine;
pub mod handlers;
pub mod store;
