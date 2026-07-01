//! 门户业务 Handler 实现。
//!
//! Handler 是薄层：解析请求 → 委托 `cmx_portal` 业务函数 → 包 [`ApiResp`] 返回。
//! 业务错误经 `From<PortalError> for cmx_api_types::Error` 自动 `?` 传播为 HTTP 错误。
//! 路径/查询/请求体与 Node 后端保持一致，响应统一 ApiResp 信封（前端 apiFetch 拆 data）。

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

// ───────────────────────── 查询/路径参数结构 ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct MenuQuery {
    #[serde(default)]
    pub menu: String,
}

#[derive(Debug, Deserialize)]
pub struct ActivitiesQuery {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FactPath {
    pub domain: String,
    pub app: String,
    pub module: String,
    pub file: String,
}

/// html-pages 列表查询：分页 + domain/app/module 过滤。
#[derive(Debug, Deserialize)]
pub struct HtmlListQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
}

/// DAM 列举过滤（domain / app）。
#[derive(Debug, Deserialize)]
pub struct DamQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, alias = "application")]
    pub app: Option<String>,
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

/// 删除应用路径。
#[derive(Debug, Deserialize)]
pub struct AppDelPath {
    pub domain: String,
    pub application: String,
}

/// definitions 查询（list / config / delete 用）。
#[derive(Debug, Deserialize)]
pub struct DefQuery {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, alias = "app")]
    pub application: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
}

impl DefQuery {
    fn to_ref(&self) -> cmx_portal::definitions::store::DefRef {
        cmx_portal::definitions::store::DefRef {
            domain: self.domain.clone(),
            application: self.application.clone(),
            app: None,
            module: self.module.clone(),
            file: self.file.clone(),
            id: None,
        }
    }
}

// ───────────────────────── 域 / 菜单 / 活动 ─────────────────────────

/// `GET /api/domains` —— 域清单（DAM 优先派生，回退 activities/domains.json）。
pub async fn get_domains(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    debug!("{:<12} - handler::get_domains", "HANDLER");
    Ok(Json(ApiResp::ok(cmx_portal::meta::domains::get_domains_doc().await?)))
}

/// `GET /api/menu-pages?menu=…` —— 菜单 JSON。
pub async fn get_menu_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<MenuQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::meta::menu_pages::get_menu_page_json(&q.menu).await?)))
}

/// `GET /api/activities?name=…` —— 域应用清单。
pub async fn get_activities(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<ActivitiesQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::meta::activities::get_activities_doc(&q.name).await?)))
}

// ───────────────────────── 工作区节点 ─────────────────────────

/// `GET /api/workspace-nodes` —— 列表摘要。
pub async fn list_workspace_nodes(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::meta::workspace_nodes::list_workspace_nodes().await?)))
}

/// `GET /api/workspace-nodes/:id` —— 完整定义。
pub async fn get_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::get_workspace_node_by_id(&id).await?;
    Ok(Json(ApiResp::ok(serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?)))
}

/// `POST /api/workspace-nodes` —— upsert。
pub async fn save_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::meta::workspace_nodes::WorkspaceNodeInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::save_workspace_node(input).await?;
    Ok(Json(ApiResp::ok(serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?)))
}

/// `DELETE /api/workspace-nodes/:id` —— 删除。
pub async fn delete_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::meta::workspace_nodes::delete_workspace_node(&id).await?)))
}

// ───────────────────────── 表单页 ─────────────────────────

/// `GET /api/form-pages?page=&pageSize=` —— 分页列表。
pub async fn list_form_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::form::list_form_pages_paged(q.page, q.page_size).await?)))
}

/// `POST /api/form-pages` —— 保存。
pub async fn save_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::form::FormPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::form::save_form_page(input).await?)))
}

/// `GET /api/form-pages/:id` —— 单条。
pub async fn get_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::form::get_form_page_by_id(&id).await?)))
}

// ───────────────────────── 原生页面 ─────────────────────────

/// `GET /api/native-pages?page=&pageSize=` —— 分页列表。
pub async fn list_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::native::list_native_pages_paged(q.page, q.page_size).await?)))
}

/// `POST /api/native-pages` —— 保存。
pub async fn save_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::native::NativePageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::native::save_native_page(input).await?)))
}

/// `POST /api/native-pages/batch` —— 批量取源码。
pub async fn batch_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::native::get_native_pages_by_ids(&body).await?)))
}

/// `GET /api/native-pages/:id` —— 单条（含源码）。
pub async fn get_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let full = cmx_portal::pages::native::get_native_page_by_id(&id).await?;
    Ok(Json(ApiResp::ok(serde_json::to_value(full).map_err(cmx_portal::PortalError::from)?)))
}

// ───────────────────────── 事实数据 ─────────────────────────
/// `GET /api/fact/list?domain=&app=&module=` —— 列出事实文件。
pub async fn list_facts(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::fact::store::FactQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::fact::store::list_facts(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/fact/get` —— 读取事实文件（请求体 { domain, app, module, file }）。
pub async fn get_fact_post(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(r): Json<cmx_portal::fact::store::FactRef>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::fact::store::get_fact(&r).await?)))
}

/// `GET /api/fact/:domain/:app/:module/:file` —— 读取事实文件（路径参数）。
pub async fn get_fact_path(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<FactPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let r = cmx_portal::fact::store::FactRef {
        domain: p.domain,
        app: p.app,
        module: p.module,
        file: p.file,
    };
    Ok(Json(ApiResp::ok(cmx_portal::fact::store::get_fact(&r).await?)))
}

// ───────────────────────── HTML 页面 ─────────────────────────

/// `GET /api/html-pages?page=&pageSize=&domain=&app=&module=` —— 分页列表。
pub async fn list_html_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<HtmlListQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let doc = cmx_portal::pages::html::list_html_pages_paged(
        q.page,
        q.page_size,
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(doc)))
}

/// `POST /api/html-pages` —— 保存。
pub async fn save_html_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::html::HtmlPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::html::save_html_page(input).await?)))
}

/// `POST /api/html-pages/batch` —— 批量取完整页面。
pub async fn batch_html_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::html::get_html_pages_by_ids(&body).await?)))
}

/// `GET /api/html-pages/:id` —— 单页（含 html）。
pub async fn get_html_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::pages::html::get_html_page_by_id(&id).await?)))
}

// ───────────────────────── DAM 注册表（读写 CRUD）─────────────────────────

/// `GET /api/dam-registry` —— 完整注册表。
pub async fn dam_registry(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let reg = cmx_portal::dam::store::get_dam_registry().await?;
    Ok(Json(ApiResp::ok(serde_json::to_value(reg).map_err(cmx_portal::PortalError::from)?)))
}

/// `GET /api/dam-registry/domains` —— 域列表。
pub async fn dam_list_domains(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let domains = cmx_portal::dam::store::list_domains().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "domains": domains }))))
}

/// `POST /api/dam-registry/domains` —— upsert 域。
pub async fn dam_upsert_domain(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::dam::store::upsert_domain(&body).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// `DELETE /api/dam-registry/domains/:domain` —— 删除域。
pub async fn dam_delete_domain(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(domain): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dam::store::delete_domain(&domain).await?)))
}

/// `GET /api/dam-registry/applications?domain=` —— 应用列表。
pub async fn dam_list_applications(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let apps = cmx_portal::dam::store::list_applications(q.domain.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "applications": apps }))))
}

/// `POST /api/dam-registry/applications` —— upsert 应用。
pub async fn dam_upsert_application(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::dam::store::upsert_application(&body).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// `DELETE /api/dam-registry/applications/:domain/:application` —— 删除应用。
pub async fn dam_delete_application(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<AppDelPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dam::store::delete_application(&p.domain, &p.application).await?)))
}

/// `GET /api/dam-registry/modules?domain=&app=` —— 模块列表。
pub async fn dam_list_modules(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let mods = cmx_portal::dam::store::list_modules(q.domain.as_deref(), q.app.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "modules": mods }))))
}

/// `POST /api/dam-registry/modules` —— upsert 模块。
pub async fn dam_upsert_module(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::dam::store::upsert_module(&body).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// `DELETE /api/dam-registry/modules/:domain/:application/:module` —— 删除模块。
pub async fn dam_delete_module(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModulePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dam::store::delete_module(&p.domain, &p.application, &p.module).await?)))
}

// ───────────────────────── 注册表只读派生（/registry/*）─────────────────────────

/// `GET /api/registry/domains` —— 域列表（DAM 派生）。
pub async fn registry_domains(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let domains = cmx_portal::dam::store::list_domains().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "domains": domains }))))
}

/// `GET /api/registry/apps?domain=` —— 应用列表（DAM 派生）。
pub async fn registry_apps(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let apps = cmx_portal::dam::store::list_applications(q.domain.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "apps": apps }))))
}

/// `GET /api/registry/modules?domain=&app=` —— 模块列表（DAM 派生）。
pub async fn registry_modules(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let mods = cmx_portal::dam::store::list_modules(q.domain.as_deref(), q.app.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "modules": mods }))))
}

/// `GET /api/registry/dam` —— 一次返回 { domains, apps, applications, modules }。
pub async fn registry_dam(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let reg = cmx_portal::dam::store::get_dam_registry().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({
        "domains": reg.domains,
        "apps": reg.applications,
        "applications": reg.applications,
        "modules": reg.modules,
    }))))
}

// ───────────────────────── 模块清单与资源 ─────────────────────────

/// `GET /api/modules?domain=&app=` —— 模块清单列表。
pub async fn list_modules(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DamQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::meta::modules::list_module_manifests(q.domain.as_deref(), q.app.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/modules/:domain/:application/:module` —— 单模块 manifest。
pub async fn get_module_manifest(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModulePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let m = cmx_portal::meta::modules::load_module_manifest(&p.domain, &p.application, &p.module).await?;
    Ok(Json(ApiResp::ok(m)))
}

/// `GET /api/modules/:domain/:application/:module/resources/:type` —— 解析模块资源。
pub async fn get_module_resource(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<ModuleResourcePath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let out = cmx_portal::meta::modules::resolve_module_resource(&p.domain, &p.application, &p.module, &p.res_type).await?;
    Ok(Json(ApiResp::ok(out)))
}

/// `GET /api/module-resources?domain=&app=&module=&type=` —— 按 query 解析资源。
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

// ───────────────────────── 定义中心 ─────────────────────────

/// `GET /api/definitions/list?kind=&domain=&application=&module=` —— 列表。
pub async fn definitions_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::definitions::store::list_definitions(
        q.kind.as_deref(),
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/definitions/config?domain=&application=&module=&file=` —— 读单个定义。
pub async fn definitions_get(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::definitions::store::get_definition(&q.to_ref()).await?)))
}

/// `POST /api/definitions/config?domain=&...&file=` —— 保存定义（body 为文档）。
pub async fn definitions_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::definitions::store::save_definition(&q.to_ref(), &body).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "ok": true, "saved": saved }))))
}

/// `POST /api/definitions/batch` —— 批量读 + base 字段集。
pub async fn definitions_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::definitions::store::get_definitions_batch(&body).await?)))
}

/// `DELETE /api/definitions/config?domain=&...&file=` —— 删除定义。
pub async fn definitions_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::definitions::store::delete_definition(&q.to_ref()).await?)))
}

// ───────────────────────── 字典检索引擎 ─────────────────────────

/// suggest / entries 写入的 query 参数。
#[derive(Debug, Deserialize)]
pub struct DictQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub rebuild: Option<String>,
}

/// 字典 id 路径。
#[derive(Debug, Deserialize)]
pub struct DictIdPath {
    #[serde(rename = "dictId")]
    pub dict_id: String,
}

/// 字典 id + 条目 id 路径。
#[derive(Debug, Deserialize)]
pub struct DictEntryPath {
    #[serde(rename = "dictId")]
    pub dict_id: String,
    pub id: String,
}

/// `GET /api/dict/_schemas` —— schema 列表。
pub async fn dict_schemas(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let schemas = cmx_portal::dict::schema::list_schemas_json().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "schemas": schemas }))))
}

/// `POST /api/dict/_schema` —— 注册/更新 schema。
pub async fn dict_register_schema(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::schema::register_schema(&body).await?)))
}

/// `POST /api/dict/multi-search` —— 多字典联查。
pub async fn dict_multi_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::multi::execute(&body).await?)))
}

/// `POST /api/dict/batch-data` —— 多字典内容批量加载。
pub async fn dict_batch_data(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::api::batch_data_endpoint(&body).await?)))
}

/// `POST /api/dict/:dictId/search` —— 单字典检索。
pub async fn dict_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::api::search_endpoint(&p.dict_id, &body).await?)))
}

/// `GET /api/dict/:dictId/suggest?q=` —— 自动补全。
pub async fn dict_suggest(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Query(q): Query<DictQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::api::suggest_endpoint(&p.dict_id, q.q.as_deref().unwrap_or("")).await?)))
}

/// `POST /api/dict/:dictId/entries?rebuild=` —— 写入条目。
pub async fn dict_upsert_entries(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Query(q): Query<DictQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rebuild = q.rebuild.as_deref() == Some("true");
    Ok(Json(ApiResp::ok(cmx_portal::dict::api::upsert_entries_endpoint(&p.dict_id, &body, rebuild).await?)))
}

/// `DELETE /api/dict/:dictId/entries/:id` —— 删除单条目。
pub async fn dict_delete_entry(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictEntryPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::repo::delete_entry(&p.dict_id, &p.id).await?)))
}

/// `DELETE /api/dict/:dictId/entries` —— 清空条目。
pub async fn dict_clear_entries(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::dict::repo::clear_entries(&p.dict_id).await?)))
}

/// `POST /api/dict/:dictId/deactivate` —— 停用一个码。
pub async fn dict_deactivate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let code = body.get("code").and_then(|v| v.as_str()).unwrap_or("");
    let valid_to = body.get("validTo").and_then(|v| v.as_str());
    let successor = body.get("successorCode").and_then(|v| v.as_str());
    Ok(Json(ApiResp::ok(cmx_portal::dict::write::deactivate(&p.dict_id, code, valid_to, successor).await?)))
}

/// `POST /api/dict/:dictId/supersede` —— 停旧启新。
pub async fn dict_supersede(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let old_code = body.get("oldCode").and_then(|v| v.as_str()).unwrap_or("");
    let new_code = body.get("newCode").and_then(|v| v.as_str()).unwrap_or("");
    let as_of = body.get("asOf").and_then(|v| v.as_str());
    let new_entry = body.get("newEntry");
    Ok(Json(ApiResp::ok(cmx_portal::dict::write::supersede(&p.dict_id, old_code, new_code, as_of, new_entry).await?)))
}

// ───────────────────────── 上下文档案 ─────────────────────────

/// context-profile DAM + scenario query（list 只用 domain/app/module；其余用全四段 + 任意锚点键）。
#[derive(Debug, Deserialize)]
pub struct CpQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub scenario: Option<String>,
    /// 其余锚点维度键（gl_account=... 等）。
    #[serde(flatten)]
    pub rest: std::collections::HashMap<String, String>,
}

impl CpQuery {
    fn to_ref(&self) -> cmx_portal::context_profile::store::CpRef {
        cmx_portal::context_profile::store::CpRef {
            domain: self.domain.clone(),
            app: self.app.clone(),
            module: self.module.clone(),
            scenario: self.scenario.clone(),
        }
    }
    /// 把锚点键收成 serde_json Map（仅 rest，不含 DAM 四段）。
    fn anchor_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.rest.iter().map(|(k, v)| (k.clone(), serde_json::json!(v))).collect()
    }
}

/// `GET /api/context-profile/list` —— 列表。
pub async fn cp_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::context_profile::store::list_context_profiles(
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/context-profile/config` —— 读单个档案。
pub async fn cp_get_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::context_profile::store::get_context_profile(&q.to_ref()).await?)))
}

/// `POST /api/context-profile/config` —— 保存档案（含 validate）。
pub async fn cp_save_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    // 校验：无效 422（与 Node 一致用 fail code 422）
    let diagnostics = cmx_portal::context_profile::validator::validate_context_profile(&body);
    if !diagnostics.get("valid").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Ok(Json(ApiResp::fail_with_data(422, "校验未通过", serde_json::json!({ "ok": false, "diagnostics": diagnostics }))));
    }
    let saved = cmx_portal::context_profile::store::save_context_profile(&q.to_ref(), &body).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "ok": true, "saved": saved }))))
}

/// `DELETE /api/context-profile/config` —— 删除档案。
pub async fn cp_delete_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::context_profile::store::delete_context_profile(&q.to_ref()).await?)))
}

/// `GET /api/context-profile/resolve` —— 按锚点解析合并规则 → fields/columnModel。
pub async fn cp_resolve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::context_profile::api::resolve(&q.to_ref(), &q.anchor_map()).await?)))
}

/// `GET /api/context-profile/rule` —— 按锚点取规则 + 相关维度。
pub async fn cp_rule(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::context_profile::api::rule(&q.to_ref(), &q.anchor_map()).await?)))
}

/// `POST /api/context-profile/validate` —— 校验。
pub async fn cp_validate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let diagnostics = cmx_portal::context_profile::api::validate(&body, &q.to_ref()).await?;
    let valid = diagnostics.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
    if valid {
        Ok(Json(ApiResp::ok(diagnostics)))
    } else {
        Ok(Json(ApiResp::fail_with_data(422, "校验未通过", diagnostics)))
    }
}

/// `POST /api/context-profile/preview` —— 校验 + 解析预览。
pub async fn cp_preview(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<CpQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::context_profile::api::preview(&body, &q.to_ref()).await?)))
}

// ───────────────────────── AI 对话中继 ─────────────────────────

/// `POST /api/ai/chat` —— 转发到 DeepSeek/OpenAI 兼容服务。未配置返回 501 业务码。
pub async fn ai_chat(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    if !cmx_portal::ai::is_configured() {
        return Ok(Json(ApiResp::fail(501, "AI 服务未配置：请设置 CMX_AI_API_KEY 或 DEEPSEEK_API_KEY")));
    }
    Ok(Json(ApiResp::ok(cmx_portal::ai::chat(&body).await?)))
}

// ───────────────────────── AI 本地编辑代理 ─────────────────────────

/// `GET /api/agent/capabilities` —— 代理能力 / 工具清单。
pub async fn agent_capabilities(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::agent::flow::capabilities())))
}

/// `POST /api/agent/message` —— 一次性返回事件序列。
pub async fn agent_message(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::agent::flow::message(&body).await?)))
}

/// `POST /api/agent/message/stream` —— SSE 流式事件。
pub async fn agent_message_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::Event;
    use axum::response::IntoResponse;
    let messages = cmx_portal::agent::flow::normalize_messages(body.get("messages").unwrap_or(&serde_json::Value::Null));
    let context = body.get("context").filter(|v| v.is_object()).cloned().unwrap_or(serde_json::json!({}));
    let conv_id = body
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("conv_{}", std::process::id()));

    // 本地 planner 非真流式：先把事件全跑出来，再逐条 SSE 推送（与 Node 协议一致：meta/agent_event*/done）。
    let mut sse_events: Vec<std::result::Result<Event, std::convert::Infallible>> = Vec::new();
    sse_events.push(Ok(Event::default().event("meta").json_data(serde_json::json!({ "conversationId": conv_id })).unwrap_or_default()));
    match cmx_portal::agent::flow::run_agent_flow(&messages, &context, |_| {}).await {
        Ok(events) => {
            for ev in events {
                sse_events.push(Ok(Event::default().event("agent_event").json_data(&ev).unwrap_or_default()));
            }
            sse_events.push(Ok(Event::default().event("done").json_data(serde_json::json!({ "conversationId": conv_id })).unwrap_or_default()));
        }
        Err(e) => {
            sse_events.push(Ok(Event::default().event("error").json_data(serde_json::json!({ "error": e.to_string() })).unwrap_or_default()));
        }
    }
    axum::response::Sse::new(futures::stream::iter(sse_events)).keep_alive(axum::response::sse::KeepAlive::default()).into_response()
}

/// `POST /api/agent/approvals/:id` —— 审批决定。
pub async fn agent_approval(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let decision = body.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    Ok(Json(ApiResp::ok(cmx_portal::agent::flow::handle_approval(&id, decision).await?)))
}

// ───────────────────────── 服务目录（Bruno collection）─────────────────────────

/// `GET /api/service-catalog?domain=&app=&module=` —— 服务列表。
pub async fn service_catalog_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<SvcCatalogQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let services = cmx_portal::service_catalog::store::list_services(q.domain.as_deref(), q.app.as_deref(), q.module.as_deref()).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "services": services }))))
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
