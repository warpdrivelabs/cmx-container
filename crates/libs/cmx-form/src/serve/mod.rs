//! 页面只读投递（serve）：native-pages / html-pages 六端点的通用 axum 路由。
//!
//! 从五个引擎微服务（flow / report / rule / mdm / model）的重复实现收编而来，行为与各副本
//! 逐字节对齐；目录解析遵循页面资产目录规范 v2（见 [`config`] 模块文档）：
//! **relPath 一律相对索引文件所在目录**，加载器不感知任何布局约定，合并单体时
//! 拷贝服务子目录 + 重新生成 index.json 即可。差异只剩 html 投递开关 [`HtmlLayout`]。
//! 错误体经泛型 `E: From<PageServeError> + IntoResponse` 渲染：成功体统一
//! `cmx_api_types::ApiResp`（wire 与 flow/rule 自持 ApiResp 等价），错误体由 E 决定，
//! 保住 rule/flow `{code:4}` 与 mdm/model/rpt `{code:404}` 两种历史语义。
//!
//! 契约（与门户 cmx-common-api/portal/pages.rs 前缀一致，供门户 F3 反代）：
//!   - `GET  /native-pages`          分页列表（不含源码）
//!   - `POST /native-pages/batch`    批量取源码 → `{items:[NativePageFull]}`
//!   - `GET  /native-pages/{id}`     单条含源码
//!   - `GET  /html-pages`            v2 分片索引分页列表（可按 domain/app/module/keyword 过滤）
//!   - `POST /html-pages/batch`      批量取页面 → `{pages,revs,errors}`
//!   - `GET  /html-pages/{id}`       单页含 html
//!
//! rev = xxhash64(bytes, seed 0) → 16 hex（复用 [`crate::cache::content_rev`]，与门户一致）。
//! 每请求同步读盘、无进程内状态（无状态服务约束）；索引缺失降级空集，解析失败补 warn 日志。

pub mod config;
pub mod error;
pub mod loader;
pub mod routes;

#[cfg(test)]
mod tests;

pub use config::{HtmlLayout, PageServeConfig};
pub use error::PageServeError;
pub use routes::frontend_pages_routes;
