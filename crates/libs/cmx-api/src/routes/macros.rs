//! 路由注册宏
//!
//! 提供宏来简化通用 CRUD 路由的注册，支持 OpenAPI 文档生成。

/// 注册通用 CRUD 路由（旧版，不生成 OpenAPI 文档）
///
/// # 参数
/// * `router` - Axum 路由器
/// * `bmc` - 模型控制器类型（实现 DbBmc trait）
/// * `filter` - 过滤器类型（实现 Into<FilterGroups>）
/// * `entity_create` - 创建 Entity 类型（实现 HasSeaFields）
/// * `entity_update` - 更新 Entity 类型（实现 HasSeaFields）
/// * `prefix` - 路由前缀
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $bmc:ty, $filter:ty, $entity_create:ty, $entity_update:ty, $prefix:expr) => {
        $router
            .route(concat!($prefix, "/create"), axum::routing::post($crate::rest::handler::create::<$bmc, $entity_create>))
            .route(concat!($prefix, "/create-many"), axum::routing::post($crate::rest::handler::create_many::<$bmc, $entity_create>))
            .route(concat!($prefix, "/get"), axum::routing::get($crate::rest::handler::get_by_id::<$bmc>))
            .route(concat!($prefix, "/update"), axum::routing::post($crate::rest::handler::update::<$bmc, $entity_update>))
            .route(concat!($prefix, "/update-many"), axum::routing::post($crate::rest::handler::update_many::<$bmc, $entity_update>))
            .route(concat!($prefix, "/delete"), axum::routing::post($crate::rest::handler::delete::<$bmc>))
            .route(concat!($prefix, "/list"), axum::routing::post($crate::rest::handler::list::<$bmc, $filter>))
            .route(concat!($prefix, "/page"), axum::routing::post($crate::rest::handler::page::<$bmc, $filter>))
    };
}

/// 声明 CRUD Handlers 模块
///
/// 为指定实体生成包含 8 个 CRUD handler 函数的模块，
/// 这些 handler 调用通用的 rest::handler 函数，并添加 OpenAPI 注解。
///
/// 使用 `serde_json::Value` 作为 OpenAPI 兼容的参数类型，
/// 内部通过反序列化转换为正确的类型。
///
/// # 参数
/// * `$module_name` - 生成的模块名
/// * `$entity` - 实体类型
/// * `$bmc` - BMC 类型
/// * `$entity_create` - 创建 DTO 类型
/// * `$entity_update` - 更新 DTO 类型
/// * `$filter` - 过滤器类型
/// * `$tag` - OpenAPI tag
/// * `$prefix` - 路由前缀
#[macro_export]
macro_rules! declare_crud_handlers {
    (
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr
    ) => {
        pub mod $module_name {
            use axum::extract::{Query, State};
            use axum::http::HeaderMap;
            use axum::Json;
            use cmx_core::model::data::dataset::DataSet;
            use cmx_core::{DeletePayload, GetParams, ListParams, PageParams, UpdatePayload};
            use serde::Deserialize;
            use serde_json::Value;
            use $crate::api_response::ApiResp;
            use $crate::app_state::CmxAppState;
            use $crate::error::Result;
            use $crate::middleware::CmxSvrContext;

            #[utoipa::path(
                post,
                path = concat!($prefix, "/create"),
                request_body = $entity_create,
                responses(
                    (status = 200, description = "创建成功")
                ),
                tag = $tag
            )]
            pub async fn create(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(data): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let data: $entity_create = serde_json::from_value(data)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::create::<$bmc, $entity_create>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/create-many"),
                request_body = Vec<$entity_create>,
                responses(
                    (status = 200, description = "批量创建成功")
                ),
                tag = $tag
            )]
            pub async fn create_many(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(data): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let data: Vec<$entity_create> = serde_json::from_value(data)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::create_many::<$bmc, $entity_create>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            #[utoipa::path(
                get,
                path = concat!($prefix, "/get"),
                params(
                    ("id" = String, Path, description = "实体主键ID")
                ),
                responses(
                    (status = 200, description = "获取成功")
                ),
                tag = $tag
            )]
            pub async fn get(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Query(params): Query<GetParams>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                $crate::rest::handler::get_by_id::<$bmc>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Query(params),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/update"),
                request_body = serde_json::Value,
                responses(
                    (status = 200, description = "更新成功")
                ),
                tag = $tag
            )]
            pub async fn update(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(payload): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let payload: UpdatePayload<$entity_update> = serde_json::from_value(payload)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::update::<$bmc, $entity_update>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(payload),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/update-many"),
                request_body = serde_json::Value,
                responses(
                    (status = 200, description = "批量更新成功")
                ),
                tag = $tag
            )]
            pub async fn update_many(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(data): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let data: Vec<UpdatePayload<$entity_update>> = serde_json::from_value(data)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::update_many::<$bmc, $entity_update>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/delete"),
                request_body = serde_json::Value,
                responses(
                    (status = 200, description = "删除成功")
                ),
                tag = $tag
            )]
            pub async fn delete(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(payload): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let payload: DeletePayload = serde_json::from_value(payload)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::delete::<$bmc>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(payload),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/list"),
                request_body = serde_json::Value,
                responses(
                    (status = 200, description = "列表查询成功")
                ),
                tag = $tag
            )]
            pub async fn list(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(params): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let params: ListParams<$filter> = serde_json::from_value(params)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::list::<$bmc, $filter>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(params),
                )
                .await
            }

            #[utoipa::path(
                post,
                path = concat!($prefix, "/page"),
                request_body = serde_json::Value,
                responses(
                    (status = 200, description = "分页查询成功")
                ),
                tag = $tag
            )]
            pub async fn page(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(params): Json<Value>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                let params: PageParams<$filter> = serde_json::from_value(params)
                    .map_err(|e| $crate::error::Error::ValidationError {
                        errors: vec![e.to_string()],
                    })?;
                $crate::rest::handler::page::<$bmc, $filter>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(params),
                )
                .await
            }
        }
    };
}

/// 注册已生成的 CRUD Handlers 模块到路由
///
/// # 参数
/// * `$router` - Axum 路由器
/// * `$handlers_mod` - 生成的 handlers 模块 (如 `domain_crud`)
/// * `$prefix` - 路由前缀
#[macro_export]
macro_rules! register_crud_handlers_module {
    ($router:expr, $handlers_mod:ident, $prefix:expr) => {
        $router
            .route(
                concat!($prefix, "/create"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::create),
            )
            .route(
                concat!($prefix, "/create-many"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::create_many),
            )
            .route(
                concat!($prefix, "/get"),
                axum::routing::get(crate::routes::crud_handlers::$handlers_mod::get),
            )
            .route(
                concat!($prefix, "/update"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::update),
            )
            .route(
                concat!($prefix, "/update-many"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::update_many),
            )
            .route(
                concat!($prefix, "/delete"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::delete),
            )
            .route(
                concat!($prefix, "/list"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::list),
            )
            .route(
                concat!($prefix, "/page"),
                axum::routing::post(crate::routes::crud_handlers::$handlers_mod::page),
            )
    };
}

/// 设置 CRUD API（组合宏）
///
/// 一次性完成 handler 声明和路由注册，减少重复代码。
///
/// # 参数
/// * `$router` - Axum 路由器
/// * `$module_name` - 生成的模块名
/// * `$entity` - 实体类型
/// * `$bmc` - BMC 类型
/// * `$entity_create` - 创建 DTO 类型
/// * `$entity_update` - 更新 DTO 类型
/// * `$filter` - 过滤器类型
/// * `$tag` - OpenAPI tag
/// * `$prefix` - 路由前缀
#[macro_export]
macro_rules! setup_crud_api {
    (
        $router:expr,
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr
    ) => {
        $crate::declare_crud_handlers!(
            $module_name,
            $entity,
            $bmc,
            $entity_create,
            $entity_update,
            $filter,
            $tag,
            $prefix
        );
        $crate::register_crud_handlers_module!($router, $module_name, $prefix)
    };
}