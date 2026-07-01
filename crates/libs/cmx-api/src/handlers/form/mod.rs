//! Form 模块
//!
//! 提供表单实体的 CRUD 操作
//! Entity/BMC/Filter/Service 定义在 cmx-biz crate，此处通过 re-export 暴露

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::form::{Form, FormBmc, FormFilter, FormForCreate, FormForUpdate, FormService};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;

/// Form 模块路由
pub struct FormModule;

impl ModuleRoutes for FormModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Form 标准 CRUD 路由(create/create-many/get/update/update-many/delete/list/page)
        let router = crate::register_crud_handlers_module!(router, form_crud, "/form");
        // 自定义路由可在此追加，如按模块批量查询
        router
    }

    fn prefix() -> &'static str {
        "form"
    }

    fn module_name(&self) -> &'static str {
        "form"
    }
}
