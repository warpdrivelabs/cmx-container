//! cmx-iam-api —— IAM（用户/角色/角色组/权限/规则）+ 认证（登录/OAuth2/API Key）的 HTTP 层。
//!
//! 薄 axum handler 调 cmx-iam / cmx-auth 服务。AuthModule / IamModule 实现 cmx-api-core 的
//! ModuleRoutes，由 cmx-platform-app 合并进主路由。IamApiDoc 提供本域 OpenApi 切片。
//!
//! 注：IamState 仍由 cmx-api-core 持有（过渡期）——Strategy 2 下服务 crate 不依赖 cmx-api-core，
//! 故不成环；iam handler 经 `state.iam()` 访问，不受影响。

pub mod handlers;
pub mod openapi;

pub use openapi::IamApiDoc;
pub use handlers::{auth::AuthModule, iam::IamModule};
