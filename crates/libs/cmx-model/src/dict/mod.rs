//! 字典检索引擎（复刻 Node `lib/dict/*`）。
//!
//! - [`schema`]：字典 schema 注册表（registry.json）。
//! - [`repo`]：条目读写 + 内存检索引擎（全文/模糊 CJK/Levenshtein/树形/有效性过滤/分页）。
//! - [`tree`]：平铺 hits → 树形（toTreeResult）+ 分页结果（toPagedResult）。
//! - [`multi`]：多字典联查（父子 join + 分批并发）。
//! - [`write`]：写入时计算 level/path/pathStr/sortKey/fullText + SCD 停旧启新。
//!
//! 存储：`dict/registry.json`（schema 数组）+ `dict/entries/<dictId>.json` 或
//! `dict/entries/<domain>/<app>/<module>/<dictId>.json`（schema 带 DAM 时）。

pub mod api;
pub mod multi;
pub mod repo;
pub mod schema;
pub mod tree;
pub mod write;
