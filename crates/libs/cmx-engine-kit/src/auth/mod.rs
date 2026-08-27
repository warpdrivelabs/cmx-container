//! 引擎身份认证横切件。
//!
//! 两族中间件（抽取方案 P2/P3，见
//! `documents/plans/20260826_cmx-container_五引擎应用层通用代码去重抽取方案.md`）：
//!
//! - 族 A（[`delegated`]，P2）：`X-Delegated-User-Token` 委托令牌 → `cmx_core::AuthContext`
//!   + `scope_full`（model / mdm 形态；失败降级匿名不 401）
//! - 族 B（[`jwt`]，P3）：JWT / API-Key → [`crate::tenant::TenantCtx`]（flow / rule 形态，
//!   flow 超集语义——RS256/exp 校验/委托桥/SSE ticket）
//!
//! [`common`] 收录族间公共小件；[`config`] 收录两族各自的配置装载入口。

pub mod common;
pub mod config;
pub mod delegated;
pub mod jwt;
