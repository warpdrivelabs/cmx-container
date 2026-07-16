//! 门户业务 Handler 实现。
//!
//! Handler 是薄层：解析请求 → 委托 `cmx_portal` 业务函数 → 包 [`ApiResp`] 返回。
//! 业务错误经 `From<PortalError> for cmx_api_types::Error` 自动 `?` 传播为 HTTP 错误。
//! 路径/查询/请求体与 Node 后端保持一致，响应统一 ApiResp 信封（前端 apiFetch 拆 data）。

use axum::Json;
use axum::extract::{Path, Query, State};
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

/// 帮助文档路径参数（domain/app/module/file）。
#[derive(Debug, Deserialize)]
pub struct HelpPath {
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
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::domains::get_domains_doc().await?,
    )))
}

/// `GET /api/menu-pages?menu=…` —— 菜单 JSON。
pub async fn get_menu_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<MenuQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::menu_pages::get_menu_page_json(&q.menu).await?,
    )))
}

/// `GET /api/activities?name=…` —— 域应用清单。
pub async fn get_activities(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<ActivitiesQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::activities::get_activities_doc(&q.name).await?,
    )))
}

// ───────────────────────── 工作区节点 ─────────────────────────

/// `GET /api/workspace-nodes` —— 列表摘要。
pub async fn list_workspace_nodes(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::workspace_nodes::list_workspace_nodes().await?,
    )))
}

/// `GET /api/workspace-nodes/:id` —— 完整定义。
pub async fn get_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::get_workspace_node_by_id(&id).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `POST /api/workspace-nodes` —— upsert。
pub async fn save_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::meta::workspace_nodes::WorkspaceNodeInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::save_workspace_node(input).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `DELETE /api/workspace-nodes/:id` —— 删除。
pub async fn delete_workspace_node(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::workspace_nodes::delete_workspace_node(&id).await?,
    )))
}

// ───────────────────────── 表单页 ─────────────────────────

/// `GET /api/form-pages?page=&pageSize=` —— 分页列表。
pub async fn list_form_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::list_form_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/form-pages` —— 保存。
pub async fn save_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::form::FormPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::save_form_page(input).await?,
    )))
}

/// `GET /api/form-pages/:id` —— 单条。
pub async fn get_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::get_form_page_by_id(&id).await?,
    )))
}

// ───────────────────────── 原生页面 ─────────────────────────

/// `GET /api/native-pages?page=&pageSize=` —— 分页列表。
pub async fn list_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::list_native_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/native-pages` —— 保存。
pub async fn save_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::native::NativePageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::save_native_page(input).await?,
    )))
}

/// `POST /api/native-pages/batch` —— 批量取源码。
pub async fn batch_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::get_native_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/native-pages/:id` —— 单条（含源码）。
pub async fn get_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let full = cmx_portal::pages::native::get_native_page_by_id(&id).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(full).map_err(cmx_portal::PortalError::from)?,
    )))
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
    Ok(Json(ApiResp::ok(
        cmx_portal::fact::store::get_fact(&r).await?,
    )))
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
    Ok(Json(ApiResp::ok(
        cmx_portal::fact::store::get_fact(&r).await?,
    )))
}

// ───────────────────────── 帮助中心 ─────────────────────────
/// `GET /api/help/catalog?domain=&app=&module=` —— 帮助目录（轻量项，供 explorer 搜索建树）。
pub async fn help_catalog(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::help::store::HelpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::help::store::list_catalog(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/help/get` —— 读取完整帮助文档（请求体 { domain, app, module, file }）。
pub async fn help_get_post(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(r): Json<cmx_portal::help::store::HelpRef>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let doc = cmx_portal::help::store::get_doc(&r).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(doc).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `GET /api/help/doc/:domain/:app/:module/:file` —— 读取完整帮助文档（路径参数）。
pub async fn help_get_path(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<HelpPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let r = cmx_portal::help::store::HelpRef {
        domain: p.domain,
        app: p.app,
        module: p.module,
        file: p.file,
    };
    let doc = cmx_portal::help::store::get_doc(&r).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(doc).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `POST /api/help/doc` —— 保存帮助文档（upsert）。
pub async fn help_save_doc(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::help::store::HelpDocInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::help::store::save_doc(input).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// `DELETE /api/help/doc/:domain/:app/:module/:file` —— 删除帮助文档。
pub async fn help_delete_doc(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<HelpPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let r = cmx_portal::help::store::HelpRef {
        domain: p.domain,
        app: p.app,
        module: p.module,
        file: p.file,
    };
    cmx_portal::help::store::delete_doc(&r).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "ok": true }))))
}

// ───────────────────────── 通知中心 ─────────────────────────
/// 从认证上下文取当前用户 id（通知按用户隔离）。
fn notify_user_id(c: &cmx_core::model::service::context::SVRContext) -> Result<String> {
    c.auth_context
        .as_ref()
        .map(|a| a.user_id.clone())
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| cmx_api_types::Error::unauthorized("未登录或无用户标识"))
}

#[derive(Debug, serde::Deserialize)]
pub struct NotifyListQuery {
    #[serde(default)]
    pub center: Option<String>,
}

/// `GET /api/notifications/centers` —— 三中心元信息（前端下拉用）。
pub async fn notify_centers(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::notify::store::centers_meta())))
}

/// `GET /api/notifications/counts` —— 当前用户各中心未读数 + 合计（红色角标）。
pub async fn notify_counts(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let counts = cmx_portal::notify::store::counts(&uid).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(counts).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `GET /api/notifications?center=task|message|log` —— 当前用户通知列表（缺 center 则全部）。
pub async fn notify_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Query(q): Query<NotifyListQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match q.center.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    let items = cmx_portal::notify::store::list(&uid, center).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/notifications/publish` —— 发布一条通知（也用于后端/服务端主动推送的入口）。
/// 默认发给当前用户；body 带 userId 时发给指定用户（服务端代发场景）。
pub async fn notify_publish(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(mut input): Json<cmx_portal::notify::store::NotifyInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    if input
        .user_id
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        input.user_id = Some(notify_user_id(&c)?);
    }
    let saved = cmx_portal::notify::store::publish(input).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(saved).map_err(cmx_portal::PortalError::from)?,
    )))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyMarkInput {
    #[serde(default)]
    pub center: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub all: bool,
}

/// `POST /api/notifications/mark-read` —— 标记已读：{ center, id } 标单条；{ all:true, center? } 标全部。
pub async fn notify_mark_read(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(input): Json<NotifyMarkInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match input
        .center
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    if input.all {
        let n = cmx_portal::notify::store::mark_all_read(&uid, center).await?;
        return Ok(Json(ApiResp::ok(serde_json::json!({ "marked": n }))));
    }
    let center = center.ok_or_else(|| cmx_api_types::Error::bad_request("标单条需提供 center"))?;
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| cmx_api_types::Error::bad_request("标单条需提供 id"))?;
    let changed = cmx_portal::notify::store::mark_read(&uid, center, id).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "changed": changed }))))
}

/// `GET /api/notifications/stream` —— SSE：服务端主动推送本用户的新通知与角标刷新。
/// 浏览器用 fetch + 流读消费（携带 Authorization 头），订阅进程内 broadcast，仅下发本人事件。
pub async fn notify_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let uid = match notify_user_id(&c) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 连接建立先推一次当前 counts，保证角标立刻准确（不必等下一次推送）。
    if let Ok(counts) = cmx_portal::notify::store::counts(&uid).await {
        let _ = tx.send(Ok(Event::default()
            .event("counts")
            .json_data(counts)
            .unwrap_or_default()));
    }

    // 订阅 broadcast：只转发属于本用户的事件。连接断开时该 task 自然结束。
    let mut sub = cmx_portal::notify::hub::subscribe();
    let uid_filter = uid.clone();
    tokio::spawn(async move {
        loop {
            match sub.recv().await {
                Ok(ev) => {
                    if ev.user_id != uid_filter {
                        continue;
                    }
                    let sent = tx.send(Ok(Event::default()
                        .event(&ev.kind)
                        .json_data(&ev.data)
                        .unwrap_or_default()));
                    if sent.is_err() {
                        break; // 客户端已断开
                    }
                }
                // 滞后丢消息：忽略，继续（计数以文件为准，下次 counts 事件会纠正）。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ───────────────────────── 功能启动器（自然语言打开功能）─────────────────────────
/// `GET /api/launcher/catalog` —— 全部可打开功能（轻量目录）。
pub async fn launcher_catalog(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::launcher::list_catalog().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/launcher/resolve` —— 把自然语言意图解析成可打开的功能（含完整 workspace 节点）。
pub async fn launcher_resolve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::launcher::ResolveInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::launcher::resolve(input).await?,
    )))
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
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::save_html_page(input).await?,
    )))
}

/// `POST /api/html-pages/batch` —— 批量取完整页面。
pub async fn batch_html_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/html-pages/:id` —— 单页（含 html）。
pub async fn get_html_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_page_by_id(&id).await?,
    )))
}

// ───────────────────────── 注册表只读派生（/registry/*）─────────────────────────

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
    let apps = cmx_portal::dam::store::list_applications(q.domain.as_deref(), q.active_only.unwrap_or(false)).await?;
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
    let mods = cmx_portal::dam::store::list_modules(q.domain.as_deref(), q.app.as_deref(), q.active_only.unwrap_or(false)).await?;
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

// ───────────────────────── 模块清单与资源 ─────────────────────────

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
    Ok(Json(ApiResp::ok(
        cmx_portal::definitions::store::get_definition(&q.to_ref()).await?,
    )))
}

/// `POST /api/definitions/config?domain=&...&file=` —— 保存定义（body 为文档）。
pub async fn definitions_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::definitions::store::save_definition(&q.to_ref(), &body).await?;
    Ok(Json(ApiResp::ok(
        serde_json::json!({ "ok": true, "saved": saved }),
    )))
}

/// `POST /api/definitions/batch` —— 批量读 + base 字段集。
pub async fn definitions_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::definitions::store::get_definitions_batch(&body).await?,
    )))
}

/// `DELETE /api/definitions/config?domain=&...&file=` —— 删除定义。
pub async fn definitions_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::definitions::store::delete_definition(&q.to_ref()).await?,
    )))
}

/// `POST /api/definitions/default?domain=&...&file=` —— 设为默认版本（同 stem 互斥）。
pub async fn definitions_set_default(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::definitions::store::set_default_version(&q.to_ref()).await?,
    )))
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
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/registry.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_schemas(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let schemas = cmx_portal::dict::schema::list_schemas_json().await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "schemas": schemas }))))
}

/// `POST /api/dict/_schema` —— 注册/更新 schema。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/registry.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_register_schema(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::schema::register_schema(&body).await?,
    )))
}

/// `POST /api/dict/multi-search` —— 多字典联查。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_multi_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::multi::execute(&body).await?,
    )))
}

/// `POST /api/dict/batch-data` —— 多字典内容批量加载。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_batch_data(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::api::batch_data_endpoint(&body).await?,
    )))
}

/// `POST /api/dict/:dictId/search` —— 单字典检索。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::api::search_endpoint(&p.dict_id, &body).await?,
    )))
}

/// `GET /api/dict/:dictId/suggest?q=` —— 自动补全。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_suggest(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Query(q): Query<DictQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::api::suggest_endpoint(&p.dict_id, q.q.as_deref().unwrap_or("")).await?,
    )))
}

/// `POST /api/dict/:dictId/entries?rebuild=` —— 写入条目。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_upsert_entries(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Query(q): Query<DictQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rebuild = q.rebuild.as_deref() == Some("true");
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::api::upsert_entries_endpoint(&p.dict_id, &body, rebuild).await?,
    )))
}

/// `DELETE /api/dict/:dictId/entries/:id` —— 删除单条目。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_delete_entry(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictEntryPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::repo::delete_entry(&p.dict_id, &p.id).await?,
    )))
}

/// `DELETE /api/dict/:dictId/entries` —— 清空条目。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_clear_entries(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::repo::clear_entries(&p.dict_id).await?,
    )))
}

/// `POST /api/dict/:dictId/deactivate` —— 停用一个码。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
pub async fn dict_deactivate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(p): Path<DictIdPath>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let code = body.get("code").and_then(|v| v.as_str()).unwrap_or("");
    let valid_to = body.get("validTo").and_then(|v| v.as_str());
    let successor = body.get("successorCode").and_then(|v| v.as_str());
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::write::deactivate(&p.dict_id, code, valid_to, successor).await?,
    )))
}

/// `POST /api/dict/:dictId/supersede` —— 停旧启新。
///
/// ⚠️ 不推荐使用 —— 基于 `data/dict/entries/*.json` 文件存储的旧接口。新增字典功能请走 `/api/dct/*`（数据库）。
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
    Ok(Json(ApiResp::ok(
        cmx_portal::dict::write::supersede(&p.dict_id, old_code, new_code, as_of, new_entry)
            .await?,
    )))
}

// ───────────────────────── 弹性组合 ─────────────────────────

/// flexible-combination DAM + scenario query（list 只用 domain/app/module；其余用全四段 + 任意锚点键）。
#[derive(Debug, Deserialize)]
pub struct FcQuery {
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

impl FcQuery {
    fn to_ref(&self) -> cmx_portal::flexible_combination::store::FcRef {
        cmx_portal::flexible_combination::store::FcRef {
            domain: self.domain.clone(),
            app: self.app.clone(),
            module: self.module.clone(),
            scenario: self.scenario.clone(),
        }
    }
    /// 把锚点键收成 serde_json Map（仅 rest，不含 DAM 四段）。
    fn anchor_map(&self) -> serde_json::Map<String, serde_json::Value> {
        self.rest
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect()
    }
}

/// `GET /api/flexible-combination/list` —— 列表。
pub async fn fc_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::flexible_combination::store::list_flexible_combinations(
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/flexible-combination/config` —— 读单个档案。
pub async fn fc_get_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::store::get_flexible_combination(&q.to_ref()).await?,
    )))
}

/// `POST /api/flexible-combination/config` —— 保存档案（含 validate）。
pub async fn fc_save_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    // 校验：无效 422（与 Node 一致用 fail code 422）
    let diagnostics = cmx_portal::flexible_combination::validator::validate_flexible_combination(&body);
    if !diagnostics
        .get("valid")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(Json(ApiResp::fail_with_data(
            422,
            "校验未通过",
            serde_json::json!({ "ok": false, "diagnostics": diagnostics }),
        )));
    }
    let saved =
        cmx_portal::flexible_combination::store::save_flexible_combination(&q.to_ref(), &body).await?;
    Ok(Json(ApiResp::ok(
        serde_json::json!({ "ok": true, "saved": saved }),
    )))
}

/// `DELETE /api/flexible-combination/config` —— 删除档案。
pub async fn fc_delete_config(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::store::delete_flexible_combination(&q.to_ref()).await?,
    )))
}

/// `POST /api/flexible-combination/default` —— 设为默认版本（同 scenario stem 互斥）。
pub async fn fc_set_default(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::store::set_default_version(&q.to_ref()).await?,
    )))
}

/// `GET /api/flexible-combination/resolve` —— 按锚点解析合并规则 → fields/columnModel。
pub async fn fc_resolve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::api::resolve(&q.to_ref(), &q.anchor_map()).await?,
    )))
}

/// `GET /api/flexible-combination/rule` —— 按锚点取规则 + 相关维度。
pub async fn fc_rule(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::api::rule(&q.to_ref(), &q.anchor_map()).await?,
    )))
}

/// `POST /api/flexible-combination/validate` —— 校验。
pub async fn fc_validate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let diagnostics = cmx_portal::flexible_combination::api::validate(&body, &q.to_ref()).await?;
    let valid = diagnostics
        .get("valid")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if valid {
        Ok(Json(ApiResp::ok(diagnostics)))
    } else {
        Ok(Json(ApiResp::fail_with_data(
            422,
            "校验未通过",
            diagnostics,
        )))
    }
}

/// `POST /api/flexible-combination/preview` —— 校验 + 解析预览。
pub async fn fc_preview(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::api::preview(&body, &q.to_ref()).await?,
    )))
}

// ───────────────────────── 三元定义统一注册（/api/defs/*） ─────────────────────────

/// `/api/defs/*` 查询参数：DAM + kind + drn（DRN 解析用）。
#[derive(serde::Deserialize)]
pub struct DefsQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// 单个 DRN 字符串（resolve/deps/compile 用）。
    #[serde(default)]
    pub drn: Option<String>,
    /// 锚点（compile 用，形如 gl_account=1122）。
    #[serde(default, flatten)]
    pub rest: std::collections::HashMap<String, String>,
}

impl DefsQuery {
    fn from_dam(&self) -> cmx_portal::flexible_combination::drn::FromDam {
        cmx_portal::flexible_combination::drn::FromDam {
            domain: self.domain.clone(),
            app: self.app.clone(),
            module: self.module.clone(),
        }
    }
}

/// `GET /api/defs/list` —— 按 kind/DAM 列出可引用定义（DCT/DOC/FLC/BASE）。
pub async fn defs_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefsQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::flexible_combination::defs::list(
        q.kind.as_deref(),
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/defs/resolve?drn=…&domain&app&module` —— 解析单个 DRN → 定义全文。
pub async fn defs_resolve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefsQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let drn = q
        .drn
        .as_deref()
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 drn 参数"))?;
    let def = cmx_portal::flexible_combination::defs::resolve(drn, &q.from_dam()).await?;
    Ok(Json(ApiResp::ok(def)))
}

/// `GET /api/defs/deps?drn=…` —— 某定义的直接依赖（imports/docRef/refDict → 绝对 DRN）。
pub async fn defs_deps(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefsQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let drn = q
        .drn
        .as_deref()
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 drn 参数"))?;
    let from = q.from_dam();
    let def = cmx_portal::flexible_combination::defs::resolve(drn, &from).await?;
    let deps = cmx_portal::flexible_combination::defs::dependencies_of(&def, &from);
    Ok(Json(ApiResp::ok(serde_json::json!({
        "drn": drn,
        "dependencies": deps,
    }))))
}

/// `GET /api/defs/compile?domain&app&module&scenario&<anchor>` —— FLC overlay 编译 + 按锚点解析。
/// 复用 flexible-combination resolve（已内置 overlay 展开）。
pub async fn defs_compile(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::flexible_combination::api::resolve(&q.to_ref(), &q.anchor_map()).await?,
    )))
}

// ───────────────────────── AI 对话中继 ─────────────────────────

/// `POST /api/ai/chat` —— 转发到 DeepSeek/OpenAI 兼容服务。未配置返回 501 业务码。
pub async fn ai_chat(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    if !cmx_portal::ai::is_configured() {
        return Ok(Json(ApiResp::fail(
            501,
            "AI 服务未配置：请设置 CMX_AI_API_KEY 或 DEEPSEEK_API_KEY",
        )));
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
    Ok(Json(ApiResp::ok(
        cmx_portal::agent::flow::message(&body).await?,
    )))
}

/// `POST /api/agent/message/stream` —— SSE 真流式事件。
///
/// 每个 agent 事件（planner 决策、plan、tool_call/tool_result、assistant 总结…）在产生的当下
/// 即经 mpsc 通道推送给客户端，而非跑完整个流程再一次性下发。flow 在独立 task 上运行，其 `emit`
/// 回调把事件即时投递到通道；SSE 流从通道逐条读取并下发。协议与 Node 一致：meta / agent_event* / done|error。
pub async fn agent_message_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let messages = cmx_portal::agent::flow::normalize_messages(
        body.get("messages").unwrap_or(&serde_json::Value::Null),
    );
    let context = body
        .get("context")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let conv_id = body
        .get("conversationId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("conv_{}", std::process::id()));

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 先推 meta（与 Node 协议一致）。
    let _ = tx.send(Ok(Event::default()
        .event("meta")
        .json_data(serde_json::json!({ "conversationId": conv_id }))
        .unwrap_or_default()));

    // flow 在独立 task 上运行：每个事件在产生当下即推送（真流式），而非跑完再批量下发。
    let conv_done = conv_id.clone();
    tokio::spawn(async move {
        let tx_emit = tx.clone();
        let result = cmx_portal::agent::flow::run_agent_flow(&messages, &context, move |ev| {
            let _ = tx_emit.send(Ok(Event::default()
                .event("agent_event")
                .json_data(&ev)
                .unwrap_or_default()));
        })
        .await;
        match result {
            Ok(_) => {
                let _ = tx.send(Ok(Event::default()
                    .event("done")
                    .json_data(serde_json::json!({ "conversationId": conv_done }))
                    .unwrap_or_default()));
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default()
                    .event("error")
                    .json_data(serde_json::json!({ "error": e.to_string() }))
                    .unwrap_or_default()));
            }
        }
    });

    // 通道 → SSE 流：逐条读取，客户端实时收到。
    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/agent/approvals/:id` —— 审批决定。
pub async fn agent_approval(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let decision = body.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    Ok(Json(ApiResp::ok(
        cmx_portal::agent::flow::handle_approval(&id, decision).await?,
    )))
}

// ───────────────────────── 服务目录（Bruno collection）─────────────────────────

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

// ───────────────────────── 模型中心（数据库初始化 + 模块部署，真实落库） ─────────────────────────

/// 从认证上下文取 (user_id, user_name)；缺省用占位，避免未登录环境（如本地）阻塞演示。
fn model_operator(c: &cmx_core::model::service::context::SVRContext) -> (String, String) {
    match c.auth_context.as_ref() {
        Some(a) => (
            if a.user_id.trim().is_empty() {
                "system".to_string()
            } else {
                a.user_id.clone()
            },
            if a.username.trim().is_empty() {
                "系统".to_string()
            } else {
                a.username.clone()
            },
        ),
        None => ("system".to_string(), "系统".to_string()),
    }
}

/// 模型中心查询参数（db_id 定位目标库）。
#[derive(Debug, Deserialize)]
pub struct ModelQuery {
    pub db_id: String,
}

/// `GET /api/model/db-state?db_id=` —— 库门闸 + 每模块每 kind scenario。
pub async fn model_db_state(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<ModelQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        super::model_center::db_state(&q.db_id).await?,
    )))
}

/// `POST /api/model/init` —— 初始化目标库（建台账系统表 + 写 meta + 历史）。
pub async fn model_init(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let db_id = body
        .get("db_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 db_id"))?;
    let (uid, uname) = model_operator(&c);
    Ok(Json(ApiResp::ok(
        super::model_center::init_db(db_id, &uid, &uname).await?,
    )))
}

/// `POST /api/model/deploy` —— 部署一批定义（create/upgrade）到目标库。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }
pub async fn model_deploy(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let db_id = body
        .get("db_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_api_types::Error::bad_request("缺少 db_id"))?;
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err(cmx_api_types::Error::bad_request("items 为空"));
    }
    let (uid, uname) = model_operator(&c);
    Ok(Json(ApiResp::ok(
        super::model_center::deploy(db_id, &items, &uid, &uname).await?,
    )))
}

/// `POST /api/model/deploy-plan-stream` —— SSE 流式生成部署执行计划（只读预览，不落库）。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }。
pub async fn model_deploy_plan_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return cmx_api_types::Error::bad_request("items 为空").into_response();
    }

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<super::model_center::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        super::model_center::deploy_plan_stream(&db_id, &items, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/deploy-stream` —— SSE 流式部署模块（编译/DDL/台账/历史/完成）。
/// body: { db_id, items:[{ kind, domain, application, module, file }] }。
pub async fn model_deploy_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return cmx_api_types::Error::bad_request("items 为空").into_response();
    }
    let (uid, uname) = model_operator(&c);

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<super::model_center::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        super::model_center::deploy_stream(&db_id, &items, &uid, &uname, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/init-plan-stream` —— SSE 流式生成初始化/系统表升级计划（只读预览，不落库）。
/// body: { db_id }。
pub async fn model_init_plan_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<super::model_center::InitEvent>();
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break;
                }
            }
        });
        super::model_center::init_plan_stream(&db_id, &etx).await;
        drop(etx);
        let _ = forward.await;
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `POST /api/model/init-stream` —— SSE 流式初始化（连接/建表/写台账/完成，实时推进度）。
/// body: { db_id }。EventSource 不能带鉴权头，前端用 fetch 流式读取（同通知中心）。
pub async fn model_init_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let db_id = match body.get("db_id").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => return cmx_api_types::Error::bad_request("缺少 db_id").into_response(),
    };
    let (uid, uname) = model_operator(&c);

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 后台跑初始化，把领域事件转成 SSE Event（named event + json data）推给客户端。
    tokio::spawn(async move {
        let (etx, mut erx) =
            tokio::sync::mpsc::unbounded_channel::<super::model_center::InitEvent>();
        // 转发 task：领域事件 → SSE。
        let sse_tx = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(e) = erx.recv().await {
                let evt = Event::default()
                    .event(&e.kind)
                    .json_data(&e.data)
                    .unwrap_or_default();
                if sse_tx.send(Ok(evt)).is_err() {
                    break; // 客户端断开
                }
            }
        });
        super::model_center::init_db_stream(&db_id, &uid, &uname, &etx).await;
        drop(etx); // 关闭领域通道 → forward 结束
        let _ = forward.await;
        // 补一个终止事件，前端据此关闭流。
        let _ = tx.send(Ok(Event::default().event("end").data("{}")));
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
