//! cmx-biz-api —— 业务模型（domain/application/menu/sys_datasource/form）的 HTTP 层。
//!
//! 薄 axum handler 调 cmx-biz 服务；写操作手写委托 Service（带 DAM 资产钩子），
//! 读操作复用 cmx-api-core 的通用 CRUD（rest::handler / declare_crud_handlers 宏）。
//!
//! 各 Module 实现 cmx-api-core 的 ModuleRoutes，由 cmx-platform-app 合并进主路由。
//! BizApiDoc 提供本域 OpenApi 切片，由 platform-app 用 OpenApi::merge() 聚合。
//!
//! 注：module handler（CRUD + 包导入导出）暂留 cmx-api，将在 cmx-plugin-api 阶段拆分。

pub mod crud_handlers;
pub mod handlers;
pub mod openapi;

pub use openapi::BizApiDoc;
pub use handlers::{
    application::ApplicationModule, domain::DomainModule, form::FormModule, menu::MenuModule,
    module::ModuleCrudModule, sys_datasource::SysDatasourceModule,
};
