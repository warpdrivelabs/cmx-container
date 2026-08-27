//! 引擎应用层横切件单源（"中立核们的中立核"）。
//!
//! 五引擎（flow / rules / model / mdm / report）平台中立应用层的**请求期**通用件收编处，
//! 消除各仓 app 层手写副本（身份认证中间件 / 租户上下文 / db_id 路由 / 响应信封别名）。
//! 与 `cmx-service-base`（启动期起服原语）按生命周期分工、互不依赖：启动期 / 无 axum /
//! 配置·数据源·注册中心相关归 service-base，请求期 / HTTP 处理链相关归本 crate。
//!
//! 模块：
//! - [`tenant`]：请求级租户上下文（task_local，收编自 cmx-flow-app / cmx-rule-app 同源副本）
//! - [`dbid`]：db_id 请求头路由（收编自 cmx-model-app / cmx-mdm-app 逐字节相同副本）
//! - [`auth`]：引擎身份认证横切件（族 A 委托令牌 / 族 B JWT）
//! - [`resp`]：响应信封 re-export（cmx-api-types 唯一真源）

pub mod auth;
pub mod dbid;
pub mod resp;
pub mod tenant;
