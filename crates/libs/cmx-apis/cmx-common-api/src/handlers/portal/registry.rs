//! 注册表只读派生 / 服务目录 / 模块清单与资源 handler。

use axum::Json;
use axum::extract::{Path, Query};
use serde::Deserialize;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// DAM 列举过滤（domain / app）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DamQuery {
    /// 域过滤（可选）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用过滤（可选；query key `app`，兼容 `application`）。
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    /// 仅返回启用（status=1）的记录。除 DAM 维护页外，其他消费方应传 true。
    #[serde(default)]
    pub active_only: Option<bool>,
}

/// 服务目录过滤（domain / app / module）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SvcCatalogQuery {
    /// 域过滤（可选）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用过滤（可选；query key `app`，兼容 `application`）。
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    /// 模块过滤（可选）。
    #[serde(default)]
    pub module: Option<String>,
}

/// 模块资源 query（?domain=&app=&module=&type=）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ModuleResourceQuery {
    /// 域（可选）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用（可选；query key `app`，兼容 `application`）。
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    /// 模块（可选）。
    #[serde(default)]
    pub module: Option<String>,
    /// 资源类型（可选；query key `type`）。
    #[serde(default, rename = "type")]
    pub res_type: Option<String>,
}

/// 模块三段路径。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ModulePath {
    /// 域。
    pub domain: String,
    /// 应用。
    pub application: String,
    /// 模块。
    pub module: String,
}

/// 模块资源四段路径（含 type）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct ModuleResourcePath {
    /// 域。
    pub domain: String,
    /// 应用。
    pub application: String,
    /// 模块。
    pub module: String,
    /// 资源类型。
    #[serde(rename = "type")]
    pub res_type: String,
}

// ─── 注册表只读派生（DAM）───

/// 列出注册表域。
///
/// `GET /api/registry/domains?active_only=` —— 域列表（DAM 注册表只读派生）。
/// `active_only=true` 只返回 status=1（启用）的域。
#[utoipa::path(
    get,
    path = "/api/registry/domains",
    params(DamQuery),
    responses(
        (status = 200, description = "域列表 {domains}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn registry_domains(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let domains = cmx_portal::dam::store::list_domains(q.active_only.unwrap_or(false)).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "domains": domains }))))
}

/// 列出注册表应用。
///
/// `GET /api/registry/apps?domain=&active_only=` —— 应用列表（DAM 注册表只读派生）。
/// `active_only=true` 只返回 status=1（启用）的应用。
#[utoipa::path(
    get,
    path = "/api/registry/apps",
    params(DamQuery),
    responses(
        (status = 200, description = "应用列表 {apps}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn registry_apps(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let apps = cmx_portal::dam::store::list_applications(
        q.domain.as_deref(),
        q.active_only.unwrap_or(false),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "apps": apps }))))
}

/// 列出注册表模块。
///
/// `GET /api/registry/modules?domain=&app=&active_only=` —— 模块列表（DAM 注册表
/// 只读派生）。`active_only=true` 只返回 status=1（启用）的模块。
#[utoipa::path(
    get,
    path = "/api/registry/modules",
    params(DamQuery),
    responses(
        (status = 200, description = "模块列表 {modules}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn registry_modules(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let mods = cmx_portal::dam::store::list_modules(
        q.domain.as_deref(),
        q.app.as_deref(),
        q.active_only.unwrap_or(false),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "modules": mods }))))
}

/// DAM 注册表全量。
///
/// `GET /api/registry/dam?active_only=` —— 一次返回 { domains, apps, applications,
/// modules }。`active_only=true` 只返回 status=1（启用）的记录。DAM 维护页
/// （registry-center.js）不传此参数以查看含禁用的全量数据；其他消费方（活动栏 /
/// 菜单 / 帮助中心 / 定义管理器 / 弹性组合管理器等）应传 true。
#[utoipa::path(
    get,
    path = "/api/registry/dam",
    params(DamQuery),
    responses(
        (status = 200, description = "{domains, apps, applications, modules}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn registry_dam(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let reg = cmx_portal::dam::store::get_dam_registry(q.active_only.unwrap_or(false)).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({
        "domains": reg.domains,
        "apps": reg.applications,
        "applications": reg.applications,
        "modules": reg.modules,
    }))))
}

// ─── 服务目录（Bruno collection）───

/// 列出服务目录。
///
/// `GET /api/service-catalog?domain=&app=&module=` —— 服务列表（Bruno collection），
/// 三级过滤均可选。
#[utoipa::path(
    get,
    path = "/api/service-catalog",
    params(SvcCatalogQuery),
    responses(
        (status = 200, description = "服务列表 {services}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn service_catalog_list(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<SvcCatalogQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let services = cmx_portal::service_catalog::store::list_services(
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(
        serde_json::json!({ "services": services }),
    )))
}

/// 取单个服务。
///
/// `GET /api/service-catalog/{id}` —— 按 id 取服务目录单项；不存在返回 404 业务码。
#[utoipa::path(
    get,
    path = "/api/service-catalog/{id}",
    params(
        ("id" = String, Path, description = "服务 id")
    ),
    responses(
        (status = 200, description = "服务详情；不存在返回 404 业务码", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn service_catalog_get(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    match cmx_portal::service_catalog::store::get_service_by_id(&id).await? {
        Some(svc) => Ok(Json(ApiResp::ok(svc))),
        None => Ok(Json(ApiResp::fail(404, "服务不存在"))),
    }
}

// ─── 模块清单与资源 ───

/// 列出模块清单。
///
/// `GET /api/modules?domain=&app=` —— 模块 manifest 列表（module.json），两级过滤均可选。
#[utoipa::path(
    get,
    path = "/api/modules",
    params(DamQuery),
    responses(
        (status = 200, description = "模块清单列表 {items}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn list_modules(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items =
        cmx_portal::meta::modules::list_module_manifests(q.domain.as_deref(), q.app.as_deref())
            .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// 取模块清单。
///
/// `GET /api/modules/{domain}/{application}/{module}` —— 单模块 manifest（module.json）。
/// 既有接口，保留路径参数（新接口规范不再如此设计）。
#[utoipa::path(
    get,
    path = "/api/modules/{domain}/{application}/{module}",
    params(ModulePath),
    responses(
        (status = 200, description = "单模块 manifest", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn get_module_manifest(
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModulePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let m = cmx_portal::meta::modules::load_module_manifest(&p.domain, &p.application, &p.module)
        .await?;
    Ok(Json(ApiResp::ok(m)))
}

/// 解析模块资源。
///
/// `GET /api/modules/{domain}/{application}/{module}/resources/{type}` —— 按资源
/// 类型解析模块资源。既有接口，保留路径参数（新接口规范不再如此设计）。
///
/// 注意：`dictEntries` / `dictSeeds` / `dictRegistry` / `facts` 等废弃资源类型仍可被
/// 请求（向后兼容存量 module.json），但前端 DAM 资源态势已不再请求这些类型。
#[utoipa::path(
    get,
    path = "/api/modules/{domain}/{application}/{module}/resources/{type}",
    params(ModuleResourcePath),
    responses(
        (status = 200, description = "解析后的模块资源", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn get_module_resource(
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModuleResourcePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let out = cmx_portal::meta::modules::resolve_module_resource(
        &p.domain,
        &p.application,
        &p.module,
        &p.res_type,
    )
    .await?;
    Ok(Json(ApiResp::ok(out)))
}

/// 解析模块资源。
///
/// `GET /api/module-resources?domain=&app=&module=&type=` —— 按 query 解析模块
/// 资源（语义同路径版 `/api/modules/.../resources/{type}`）。
///
/// 注意：`dictEntries` / `dictSeeds` / `dictRegistry` / `facts` 等废弃资源类型仍可被
/// 请求（向后兼容存量 module.json），但前端 DAM 资源态势已不再请求这些类型。
#[utoipa::path(
    get,
    path = "/api/module-resources",
    params(ModuleResourceQuery),
    responses(
        (status = 200, description = "解析后的模块资源", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn module_resources(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<ModuleResourceQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let out = cmx_portal::meta::modules::resolve_module_resource(
        q.domain.as_deref().unwrap_or(""),
        q.app.as_deref().unwrap_or(""),
        q.module.as_deref().unwrap_or(""),
        q.res_type.as_deref().unwrap_or(""),
    )
    .await?;
    Ok(Json(ApiResp::ok(out)))
}
