//! cmx-biz-api 的 OpenApi 切片。
//!
//! 从 cmx-api/openapi.rs 迁入的 domain/application/menu/sys_datasource/form 相关
//! paths + schemas，由 platform-app 用 `OpenApi::merge()` 聚合到总文档。
//! 注：crud_handlers 宏生成的 CRUD handler 路径引用本地 `crate::crud_handlers::*`。

use utoipa::OpenApi;

/// 业务模型模块 OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        // Domain handlers（写委托 DomainService 带 DAM 钩子；读复用 rest::handler 泛型）
        crate::handlers::domain::handler::create_domain,
        crate::handlers::domain::handler::update_domain,
        crate::handlers::domain::handler::delete_domain,
        crate::handlers::domain::handler::get_tree,
        // Application handlers
        crate::handlers::application::handler::create_application,
        crate::handlers::application::handler::update_application,
        crate::handlers::application::handler::delete_application,
        crate::handlers::application::handler::application_custom_page,
        // SysDatasource CRUD（宏生成）
        crate::crud_handlers::sys_datasource_crud::get,
        crate::crud_handlers::sys_datasource_crud::list,
        crate::crud_handlers::sys_datasource_crud::page,
        // Form CRUD（宏生成）
        crate::crud_handlers::form_crud::create,
        crate::crud_handlers::form_crud::create_many,
        crate::crud_handlers::form_crud::get,
        crate::crud_handlers::form_crud::update,
        crate::crud_handlers::form_crud::update_many,
        crate::crud_handlers::form_crud::delete,
        crate::crud_handlers::form_crud::list,
        crate::crud_handlers::form_crud::page,
        // Menu handlers
        crate::handlers::menu::handler::create_menu,
        crate::handlers::menu::handler::get_menu,
        crate::handlers::menu::handler::update_menu,
        crate::handlers::menu::handler::delete_menu,
        crate::handlers::menu::handler::list_menus,
        crate::handlers::menu::handler::page_menus,
        crate::handlers::menu::handler::get_menu_tree,
        // SysDatasource 手写 handler
        crate::handlers::sys_datasource::handler::get_by_db_id,
        crate::handlers::sys_datasource::handler::create_datasource,
        crate::handlers::sys_datasource::handler::update_datasource,
        crate::handlers::sys_datasource::handler::delete_datasource,
        crate::handlers::sys_datasource::handler::test_connection,
    ),
    components(
        schemas(
            crate::handlers::domain::Domain,
            crate::handlers::domain::DomainForCreate,
            crate::handlers::domain::DomainForUpdate,
            crate::handlers::domain::DomainTreeNodeData,
            cmx_api_types::TreeNode<crate::handlers::domain::DomainTreeNodeData>,
            crate::handlers::application::Application,
            crate::handlers::application::ApplicationForCreate,
            crate::handlers::application::ApplicationForUpdate,
            crate::handlers::sys_datasource::SysDatasource,
            crate::handlers::sys_datasource::SysDatasourceForCreate,
            crate::handlers::sys_datasource::SysDatasourceForUpdate,
            crate::handlers::sys_datasource::handler::GetByDbIdParams,
            crate::handlers::sys_datasource::handler::DatasourceUpdatePayload,
            crate::handlers::sys_datasource::handler::DatasourceDeletePayload,
            // Form schemas
            crate::handlers::form::Form,
            crate::handlers::form::FormForCreate,
            crate::handlers::form::FormForUpdate,
            // Menu schemas
            crate::handlers::menu::Menu,
            crate::handlers::menu::MenuForCreate,
            crate::handlers::menu::MenuForUpdate,
            crate::handlers::menu::MenuTreeNodeData,
            cmx_api_types::TreeNode<crate::handlers::menu::MenuTreeNodeData>,
        )
    )
)]
pub struct BizApiDoc;
