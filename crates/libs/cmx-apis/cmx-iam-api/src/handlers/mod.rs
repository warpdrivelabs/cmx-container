//! IAM / 认证 handler 子模块（从 cmx-api 迁入）。
//!
//! - `iam`：用户/角色/角色组/权限/互斥规则（调 cmx-iam 服务）
//! - `auth`：登录/登出/OAuth2/API Key（调 cmx-auth + cmx-iam 用户查询）

pub mod auth;
pub mod iam;
