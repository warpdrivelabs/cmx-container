//! 功能启动器 handler（自然语言打开功能）。

use axum::Json;
use axum::extract::State;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

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
