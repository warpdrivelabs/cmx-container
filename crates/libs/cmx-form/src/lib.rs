//! cmx-form —— 表单中心。
//!
//! 承接「html_pages 相关」页面资源服务（迁移自 CMXHTMLDesigner / CMXPortalManager 的 Node 后端）：
//! 表单页(form)、HTML 页(html，v2 分片 + v1 兼容)、原生页面(native)。
//! 数据为 JSON 文件存储（`data/{form,html,native}-pages/**`），经 [`cmx_jsonstore::config`] 解析数据根。
//!
//! 另含 [`serve`]：引擎微服务自投递 native/html 页的通用只读路由（收编五份引擎副本，
//! 目录布局 / html 开关 / 错误体语义全部配置化，与门户 F3 反代契约字节对齐）。
//!
//! 基础设施（config/error/fsutil/cache/util）从 [`cmx_jsonstore`] 再导出，使 `pages` 内既有的
//! `crate::config` / `crate::error` / `crate::fsutil` / `crate::util` 路径无需改动即可解析。

// 基础设施再导出：保持被移动代码里的 crate::{config,error,fsutil,cache,util} 路径有效。
pub use cmx_jsonstore::{cache, config, error, fsutil, util};

pub mod pages;
pub mod serve;
