//! 域 / 菜单 / 活动 / 工作区节点 handler。

use axum::Json;
use axum::extract::Path;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

// ─── 已弃用接口（POST /api/domains/tree + GET /api/menu/tree 统一替代）──────────
// 以下三个 handler 及其 Query 结构体已随路由（portal/mod.rs:61-63）一并注释弃用。
// service 层 domains.rs / activities.rs 整体注释；menu_pages.rs 保留（launcher 依赖
// get_menu_page_json）。回退时取消此处注释 + 路由注释 + service 文件注释即可恢复。
//
// #[derive(Debug, Deserialize)]
// pub struct MenuQuery {
//     #[serde(default)]
//     pub menu: String,
// }
//
// #[derive(Debug, Deserialize)]
// pub struct ActivitiesQuery {
//     #[serde(default)]
//     pub name: String,
// }
//
// /// `GET /api/domains` —— 域清单（DAM 优先派生，回退 activities/domains.json）。
// pub async fn get_domains(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     debug!("{:<12} - handler::get_domains", "HANDLER");
//     Ok(Json(ApiResp::ok(
//         cmx_portal::meta::domains::get_domains_doc().await?,
//     )))
// }
//
// /// `GET /api/menu-pages?menu=…` —— 菜单 JSON。
// pub async fn get_menu_pages(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<MenuQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::meta::menu_pages::get_menu_page_json(&q.menu).await?,
//     )))
// }
//
// /// `GET /api/activities?name=…` —— 域应用清单。
// pub async fn get_activities(
//     State(_s): State<CmxAppState>,
//     CmxSvrContext(_c): CmxSvrContext,
//     Query(q): Query<ActivitiesQuery>,
// ) -> Result<Json<ApiResp<serde_json::Value>>> {
//     Ok(Json(ApiResp::ok(
//         cmx_portal::meta::activities::get_activities_doc(&q.name).await?,
//     )))
// }

/// 列出工作区节点。
///
/// `GET /api/workspace-nodes` —— 全部工作区节点的列表摘要（不含 workspace 配置详情）。
#[utoipa::path(
    get,
    path = "/api/workspace-nodes",
    responses(
        (status = 200, description = "工作区节点列表摘要", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn list_workspace_nodes(
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::workspace_nodes::list_workspace_nodes().await?,
    )))
}

/// 取工作区节点。
///
/// `GET /api/workspace-nodes/{id}` —— 单个节点的完整定义（含 workspace 配置对象）。
#[utoipa::path(
    get,
    path = "/api/workspace-nodes/{id}",
    params(
        ("id" = String, Path, description = "工作区节点 id")
    ),
    responses(
        (status = 200, description = "节点完整定义（含 workspace 配置对象）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn get_workspace_node(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::get_workspace_node_by_id(&id).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// 保存工作区节点。
///
/// `POST /api/workspace-nodes` —— upsert（新建 / 更新）。body：
///
/// ```json
/// {
///   "id": "新建时可为空，由服务端生成",
///   "name": "节点名称",
///   "icon": "图标名",
///   "details": "详情描述",
///   "workspace": { "工作区配置，必须为对象": "..." }
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/workspace-nodes",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "保存后的节点完整记录", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn save_workspace_node(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::meta::workspace_nodes::WorkspaceNodeInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let rec = cmx_portal::meta::workspace_nodes::save_workspace_node(input).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(rec).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// 删除工作区节点。
///
/// `DELETE /api/workspace-nodes/{id}` —— 按节点 id 删除。既有接口，保留 DELETE
/// 方法与路径参数（新接口规范不再如此设计）。
#[utoipa::path(
    delete,
    path = "/api/workspace-nodes/{id}",
    params(
        ("id" = String, Path, description = "工作区节点 id")
    ),
    responses(
        (status = 200, description = "删除结果", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn delete_workspace_node(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::meta::workspace_nodes::delete_workspace_node(&id).await?,
    )))
}
