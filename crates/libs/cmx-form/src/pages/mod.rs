//! 页面资源：表单(form) / 原生(native) / HTML。
//!
//! - [`form`]：列表 JSON + 版本化文档（`{ form: "..." }`）。
//! - [`native`]：索引 JSON + 源文件（js/html）。
//! - html：v2 分片 + v1 兼容（阶段 1 后续单独落地）。

pub mod form;
pub mod html;
pub mod native;
