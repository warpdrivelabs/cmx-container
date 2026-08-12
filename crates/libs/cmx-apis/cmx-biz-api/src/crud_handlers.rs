//! CRUD Handlers 模块
//!
//! 为各实体生成带 OpenAPI 文档的 CRUD handler 模块
//!
//! 通过 `declare_crud_handlers!` 宏生成，每个实体的 handlers 都可以独立注册到 OpenAPI
//!
//! 注：domain/application/module 的 CRUD 已改为手写 handler（各自 handler.rs），
//! 不再用宏。原因：写操作需触发 DAM 资产文件副作用（目录搬移/引用校验），
//! 宏走 GenericCrudService 会绕过 Service 层钩子。

use cmx_api_core::declare_crud_handlers;

declare_crud_handlers!(
    sys_datasource_crud,
    crate::handlers::sys_datasource::SysDatasource,
    crate::handlers::sys_datasource::SysDatasourceBmc,
    crate::handlers::sys_datasource::SysDatasourceForCreate,
    crate::handlers::sys_datasource::SysDatasourceForUpdate,
    crate::handlers::sys_datasource::SysDatasourceFilter,
    "SysDatasource",
    "/sys-datasource"
);

declare_crud_handlers!(
    form_crud,
    crate::handlers::form::Form,
    crate::handlers::form::FormBmc,
    crate::handlers::form::FormForCreate,
    crate::handlers::form::FormForUpdate,
    crate::handlers::form::FormFilter,
    "Form",
    "/form"
);

// 注：menu 的 CRUD 已改为手写 handler（menu/handler.rs），不再用宏。
// 原因：菜单增删改需维护树形字段(code_path/id_path/depth/leaf)，宏走 GenericCrudService
// 会绕过 MenuService，无法组装分级字段。
