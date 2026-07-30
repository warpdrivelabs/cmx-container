//! 域 / 菜单 / 活动 / 工作区节点 handler。

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use tracing::debug;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

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
