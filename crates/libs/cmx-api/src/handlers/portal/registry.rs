//! 注册表只读派生 / 服务目录 / 模块清单与资源 handler。

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// DAM 列举过滤（domain / app）。
#[derive(Debug, Deserialize)]
pub struct DamQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    /// 仅返回启用（status=1）的记录。除 DAM 维护页外，其他消费方应传 true。
    #[serde(default)]
    pub active_only: Option<bool>,
}

/// 服务目录过滤（domain / app / module）。
#[derive(Debug, Deserialize)]
pub struct SvcCatalogQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
}

/// 模块资源 query（?domain=&app=&module=&type=）。
#[derive(Debug, Deserialize)]
pub struct ModuleResourceQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, alias = "application")]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default, rename = "type")]
    pub res_type: Option<String>,
}

/// 模块三段路径。
#[derive(Debug, Deserialize)]
pub struct ModulePath {
    pub domain: String,
    pub application: String,
    pub module: String,
}

/// 模块资源四段路径（含 type）。
#[derive(Debug, Deserialize)]
pub struct ModuleResourcePath {
    pub domain: String,
    pub application: String,
    pub module: String,
    #[serde(rename = "type")]
    pub res_type: String,
}

// ─── 注册表只读派生（DAM）───

/// `GET /api/registry/domains?active_only=` —— 域列表（DAM 派生）。
///
/// `active_only=true` 只返回 status=1（启用）的域。
pub async fn registry_domains(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let domains = cmx_portal::dam::store::list_domains(q.active_only.unwrap_or(false)).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "domains": domains }))))
}

/// `GET /api/registry/apps?domain=&active_only=` —— 应用列表（DAM 派生）。
///
/// `active_only=true` 只返回 status=1（启用）的应用。
pub async fn registry_apps(
    State(_s): State<CmxAppState>,
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

/// `GET /api/registry/modules?domain=&app=&active_only=` —— 模块列表（DAM 派生）。
///
/// `active_only=true` 只返回 status=1（启用）的模块。
pub async fn registry_modules(
    State(_s): State<CmxAppState>,
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

/// `GET /api/registry/dam?active_only=` —— 一次返回 { domains, apps, applications, modules }。
///
/// `active_only=true` 只返回 status=1（启用）的记录。
/// DAM 维护页（registry-center.js）不传此参数以查看含禁用的全量数据；
/// 其他消费方（活动栏/菜单/帮助中心/定义管理器/弹性组合管理器等）应传 true。
pub async fn registry_dam(
    State(_s): State<CmxAppState>,
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

/// `GET /api/service-catalog?domain=&app=&module=` —— 服务列表。
pub async fn service_catalog_list(
    State(_s): State<CmxAppState>,
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

/// `GET /api/service-catalog/:id` —— 单个服务（不存在 404）。
pub async fn service_catalog_get(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    match cmx_portal::service_catalog::store::get_service_by_id(&id).await? {
        Some(svc) => Ok(Json(ApiResp::ok(svc))),
        None => Ok(Json(ApiResp::fail(404, "服务不存在"))),
    }
}

// ─── 模块清单与资源 ───

/// `GET /api/modules?domain=&app=` —— 模块清单列表。
pub async fn list_modules(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items =
        cmx_portal::meta::modules::list_module_manifests(q.domain.as_deref(), q.app.as_deref())
            .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/modules/:domain/:application/:module` —— 单模块 manifest。
pub async fn get_module_manifest(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModulePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let m = cmx_portal::meta::modules::load_module_manifest(&p.domain, &p.application, &p.module)
        .await?;
    Ok(Json(ApiResp::ok(m)))
}

/// `GET /api/modules/:domain/:application/:module/resources/:type` —— 解析模块资源。
///
/// 注意：`dictEntries` / `dictSeeds` / `dictRegistry` / `facts` 等废弃资源类型仍可被请求
///（向后兼容存量 module.json），但前端 DAM 资源态势已不再请求这些类型。
pub async fn get_module_resource(
    State(_s): State<CmxAppState>,
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

/// `GET /api/module-resources?domain=&app=&module=&type=` —— 按 query 解析资源。
///
/// 注意：`dictEntries` / `dictSeeds` / `dictRegistry` / `facts` 等废弃资源类型仍可被请求
///（向后兼容存量 module.json），但前端 DAM 资源态势已不再请求这些类型。
pub async fn module_resources(
    State(_s): State<CmxAppState>,
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
