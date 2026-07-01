//! CRUD Handlers 模块
//!
//! 为各实体生成带 OpenAPI 文档的 CRUD handler 模块
//!
//! 通过 `declare_crud_handlers!` 宏生成，每个实体的 handlers 都可以独立注册到 OpenAPI

use crate::declare_crud_handlers;

declare_crud_handlers!(
    domain_crud,
    crate::handlers::domain::Domain,
    crate::handlers::domain::DomainBmc,
    crate::handlers::domain::DomainForCreate,
    crate::handlers::domain::DomainForUpdate,
    crate::handlers::domain::DomainFilter,
    "Domain",
    "/domains"
);

declare_crud_handlers!(
    application_crud,
    crate::handlers::application::Application,
    crate::handlers::application::ApplicationBmc,
    crate::handlers::application::ApplicationForCreate,
    crate::handlers::application::ApplicationForUpdate,
    crate::handlers::application::ApplicationFilter,
    "Application",
    "/applications"
);

declare_crud_handlers!(
    module_crud,
    crate::handlers::module::Module,
    crate::handlers::module::ModuleBmc,
    crate::handlers::module::ModuleForCreate,
    crate::handlers::module::ModuleForUpdate,
    crate::handlers::module::ModuleFilter,
    "Module",
    "/module"
);

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

declare_crud_handlers!(
    menu_crud,
    crate::handlers::menu::Menu,
    crate::handlers::menu::MenuBmc,
    crate::handlers::menu::MenuForCreate,
    crate::handlers::menu::MenuForUpdate,
    crate::handlers::menu::MenuFilter,
    "Menu",
    "/menu"
);
