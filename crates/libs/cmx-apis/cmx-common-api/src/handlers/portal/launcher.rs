//! 功能启动器 handler（自然语言打开功能）。

use axum::Json;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 解析功能入口。
///
/// `POST /api/launcher/resolve` —— 把自然语言意图解析成可打开的功能（含完整
/// workspace 节点），供 AI 助手「我要…」直接打开功能。body：
///
/// ```json
/// { "query": "自然语言意图描述" }
/// ```
#[utoipa::path(
    post,
    path = "/api/launcher/resolve",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "解析出的可打开功能（含完整 workspace 节点）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn launcher_resolve(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::launcher::ResolveInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::launcher::resolve(input).await?,
    )))
}
