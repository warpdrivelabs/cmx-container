//! 业务 handler 子模块（从 cmx-api 迁入）。
//!
//! 写操作手写委托各 Service（带 DAM 资产钩子）；读操作复用通用 CRUD。
//! Entity/BMC/Filter/Service 定义在 cmx-biz，各 mod.rs re-export 之。

pub mod application;
pub mod domain;
pub mod form;
pub mod menu;
pub mod sys_datasource;
