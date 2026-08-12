//! 路由注册宏
//!
//! 提供宏来简化通用 CRUD 路由的注册，支持 OpenAPI 文档生成。
//!
//! ## 设计原则
//!
//! 采用**双层结构体**模式：handler 函数签名使用 cmx-core 的运行时参数类型，
//! utoipa 宏的 request_body 使用 cmx-api 的文档类型（param_doc）。
//! 只有 get_by_id 使用 GET 请求，其他操作（包括 update、delete）均使用 POST + application/json。

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
            .route(
                concat!($prefix, "/create"),
                axum::routing::post($crate::rest::handler::create::<$bmc, $entity_create>),
            )
            .route(
                concat!($prefix, "/create-many"),
                axum::routing::post($crate::rest::handler::create_many::<$bmc, $entity_create>),
            )
            .route(
                concat!($prefix, "/get"),
                axum::routing::get($crate::rest::handler::get_by_id::<$bmc>),
            )
            .route(
                concat!($prefix, "/update"),
                axum::routing::post($crate::rest::handler::update::<$bmc, $entity_update>),
            )
            .route(
                concat!($prefix, "/update-many"),
                axum::routing::post($crate::rest::handler::update_many::<$bmc, $entity_update>),
            )
            .route(
                concat!($prefix, "/delete"),
                axum::routing::post($crate::rest::handler::delete::<$bmc>),
            )
            .route(
                concat!($prefix, "/list"),
                axum::routing::post($crate::rest::handler::list::<$bmc, $filter>),
            )
            .route(
                concat!($prefix, "/page"),
                axum::routing::post($crate::rest::handler::page::<$bmc, $filter>),
            )
    };
}

/// 声明 CRUD Handlers 模块
///
/// 为指定实体生成包含 8 个 CRUD handler 函数的模块，
/// 这些 handler 调用通用的 rest::handler 函数，并添加 OpenAPI 注解。
///
/// handler 函数签名使用 cmx-core 运行时参数类型（PageParams、UpdatePayload 等），
/// utoipa 宏的 request_body 使用 cmx-api 文档类型（PageParamsDoc、UpdatePayloadDoc 等）。
///
/// # 三种权限配置模式
///
/// ## 模式一：统一资源名（第 9 参数为权限资源名）
///
/// 所有 CRUD 操作使用同一资源名，自动拼接 `:create`/`:read`/`:update`/`:delete` 后缀。
///
/// ```ignore
/// declare_crud_handlers!(
///     user_crud, User, UserBmc, UserForCreate, UserForUpdate, UserFilter,
///     "User", "/users",
///     "user"  // 权限码: user:create, user:read, user:update, user:delete
/// );
/// ```
///
/// ## 模式二：无鉴权（仅 8 参数）
///
/// 不注入任何权限检查，保持向后兼容。
///
/// ```ignore
/// declare_crud_handlers!(
///     user_crud, User, UserBmc, UserForCreate, UserForUpdate, UserFilter,
///     "User", "/users"
/// );
/// ```
///
/// ## 模式三：精细化配置（perms(...) 语法）
///
/// 为 create/read/update/delete 分别指定权限资源名。
///
/// ```ignore
/// declare_crud_handlers!(
///     user_crud, User, UserBmc, UserForCreate, UserForUpdate, UserFilter,
///     "User", "/users",
///     perms(create="user", read="user", update="user_admin", delete="user_admin")
/// );
/// ```
///
/// # 参数
/// * `$module_name` - 生成的模块名
/// * `$entity` - 实体类型
/// * `$bmc` - BMC 类型
/// * `$entity_create` - 创建 DTO 类型（需实现 ToSchema）
/// * `$entity_update` - 更新 DTO 类型（需实现 ToSchema）
/// * `$filter` - 过滤器类型（实现 Into<FilterGroups>）
/// * `$tag` - OpenAPI tag
/// * `$prefix` - 路由前缀
#[macro_export]
macro_rules! declare_crud_handlers {
    // 模式三：精细化权限配置 perms(create=..., read=..., update=..., delete=...)
    (
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr,
        perms(create=$p_create:expr, read=$p_read:expr, update=$p_update:expr, delete=$p_delete:expr)
    ) => {
        $crate::__declare_crud_handlers_inner!(
            $module_name,
            $entity,
            $bmc,
            $entity_create,
            $entity_update,
            $filter,
            $tag,
            $prefix,
            $p_create,
            $p_read,
            $p_update,
            $p_delete
        )
    };

    // 模式一：统一资源名（第 9 参数为权限资源名）
    (
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr,
        $perm_resource:expr
    ) => {
        $crate::__declare_crud_handlers_inner!(
            $module_name,
            $entity,
            $bmc,
            $entity_create,
            $entity_update,
            $filter,
            $tag,
            $prefix,
            $perm_resource,
            $perm_resource,
            $perm_resource,
            $perm_resource
        )
    };

    // 模式二：无鉴权（仅 8 参数，向后兼容）
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
        $crate::__declare_crud_handlers_inner!(
            $module_name,
            $entity,
            $bmc,
            $entity_create,
            $entity_update,
            $filter,
            $tag,
            $prefix,
            "",
            "",
            "",
            ""
        );
    };
}

/// 内部代理宏：生成 CRUD handlers 模块并注入权限检查
///
/// 此宏为 `declare_crud_handlers!` 的内部实现，不应直接调用。
/// 通过 4 个权限资源参数控制各操作的鉴权行为：
/// 空字符串表示不鉴权，非空字符串则调用 `svr_ctx.require_permission`。
///
/// # 权限检查注入规则
/// * create/create_many → `{resource}:create`
/// * get/list/page → `{resource}:read`
/// * update/update_many → `{resource}:update`
/// * delete → `{resource}:delete`
#[doc(hidden)]
#[macro_export]
macro_rules! __declare_crud_handlers_inner {
    (
        $module_name:ident,
        $entity:ty,
        $bmc:ty,
        $entity_create:ty,
        $entity_update:ty,
        $filter:ty,
        $tag:expr,
        $prefix:expr,
        $p_create:expr,
        $p_read:expr,
        $p_update:expr,
        $p_delete:expr
    ) => {
        pub mod $module_name {
            use axum::Json;
            use axum::extract::{Query, State};
            use axum::http::HeaderMap;
            use cmx_core::model::data::dataset::DataSet;
            use cmx_core::{DeletePayload, GetParams, ListParams, PageParams, UpdatePayload};
            use $crate::ApiResp;
            use $crate::Result;
            use $crate::app_state::CmxAppState;
            use $crate::middleware::CmxSvrContext;

            /// 创建实体 Handler
            ///
            /// 创建单个实体记录，请求体为实体的创建 DTO。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/create"),
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
                Json(data): Json<$entity_create>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_create.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_create, ":create"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::create::<$bmc, $entity_create>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            /// 批量创建实体 Handler
            ///
            /// 批量创建多个实体记录，请求体为实体创建 DTO 的数组。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/create-many"),
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
                Json(data): Json<Vec<$entity_create>>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_create.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_create, ":create"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::create_many::<$bmc, $entity_create>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            /// 获取实体详情 Handler
            ///
            /// 根据主键 ID 查询单个实体的详细信息。
            #[utoipa::path(
                                get,
                                path = concat!("/api",$prefix, "/get"),
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
                if !$p_read.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_read, ":read"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::get_by_id::<$bmc>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Query(params),
                )
                .await
            }

            /// 更新实体 Handler
            ///
            /// 根据主键 ID 更新单个实体记录，请求体包含 ID 和更新字段。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/update"),
                                request_body = cmx_core::UpdatePayload<$entity_update>,
                                // request_body = serde_json::Value,
                                responses(
                                    (status = 200, description = "更新成功")
                                ),
                                tag = $tag
                            )]
            pub async fn update(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(payload): Json<UpdatePayload<$entity_update>>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_update.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_update, ":update"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::update::<$bmc, $entity_update>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(payload),
                )
                .await
            }

            /// 批量更新实体 Handler
            ///
            /// 批量更新多个实体记录，请求体为包含 ID 和更新字段的对象数组。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/update-many"),
                                // request_body = Vec<cmx_core::UpdatePayload<serde_json::Value>>,
                                request_body = inline(Vec<cmx_core::UpdatePayload<$entity_update>>),
                                responses(
                                    (status = 200, description = "批量更新成功")
                                ),
                                tag = $tag
                            )]
            pub async fn update_many(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(data): Json<Vec<UpdatePayload<$entity_update>>>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_update.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_update, ":update"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::update_many::<$bmc, $entity_update>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(data),
                )
                .await
            }

            /// 删除实体 Handler
            ///
            /// 根据主键 ID 删除单个或多个实体记录。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/delete"),
                                request_body = cmx_core::DeletePayload,
                                responses(
                                    (status = 200, description = "删除成功")
                                ),
                                tag = $tag
                            )]
            pub async fn delete(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(payload): Json<DeletePayload>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_delete.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_delete, ":delete"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::delete::<$bmc>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(payload),
                )
                .await
            }

            /// 实体列表查询 Handler
            ///
            /// 根据过滤条件查询实体列表，返回符合条件的所有记录。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/list"),
                                request_body = cmx_core::ListParams<serde_json::Value>,
                                responses(
                                    (status = 200, description = "列表查询成功")
                                ),
                                tag = $tag
                            )]
            pub async fn list(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(params): Json<ListParams<$filter>>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_read.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_read, ":read"))
                        .map_err($crate::Error::from)?;
                }
                $crate::rest::handler::list::<$bmc, $filter>(
                    State(cmx_state),
                    CmxSvrContext(svr_ctx),
                    headers,
                    Json(params),
                )
                .await
            }

            /// 实体分页查询 Handler
            ///
            /// 根据过滤条件和分页参数查询实体数据，返回分页结果。
            #[utoipa::path(
                                post,
                                path = concat!("/api",$prefix, "/page"),
                                request_body = cmx_core::PageParams<serde_json::Value>,
                                responses(
                                    (status = 200, description = "分页查询成功")
                                ),
                                tag = $tag
                            )]
            pub async fn page(
                State(cmx_state): State<CmxAppState>,
                CmxSvrContext(svr_ctx): CmxSvrContext,
                headers: HeaderMap,
                Json(params): Json<PageParams<$filter>>,
            ) -> Result<Json<ApiResp<DataSet>>> {
                if !$p_read.is_empty() {
                    svr_ctx
                        .require_permission(concat!($p_read, ":read"))
                        .map_err($crate::Error::from)?;
                }
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
                axum::routing::post($handlers_mod::create),
            )
            .route(
                concat!($prefix, "/create-many"),
                axum::routing::post($handlers_mod::create_many),
            )
            .route(
                concat!($prefix, "/get"),
                axum::routing::get($handlers_mod::get),
            )
            .route(
                concat!($prefix, "/update"),
                axum::routing::post($handlers_mod::update),
            )
            .route(
                concat!($prefix, "/update-many"),
                axum::routing::post($handlers_mod::update_many),
            )
            .route(
                concat!($prefix, "/delete"),
                axum::routing::post($handlers_mod::delete),
            )
            .route(
                concat!($prefix, "/list"),
                axum::routing::post($handlers_mod::list),
            )
            .route(
                concat!($prefix, "/page"),
                axum::routing::post($handlers_mod::page),
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
