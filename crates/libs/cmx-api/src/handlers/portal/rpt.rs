//! 旧报表设计器兼容计算接口。
//!
//! 新“报表设计工作台”不使用该接口；这里仅恢复历史 html 报表设计器的预览入口，避免旧页面
//! 点击预览时报 404。

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

#[derive(Debug, Deserialize)]
pub struct RptComputeReq {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub application: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub org: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
}

pub async fn rpt_compute(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<RptComputeReq>,
) -> Result<Json<ApiResp<Value>>> {
    let def_ref = json!({
        "domain": req.domain,
        "application": req.application,
        "module": req.module,
        "file": req.file,
    });
    Ok(Json(ApiResp::ok(json!({
        "values": {},
        "fetches": {},
        "context": {
            "org": req.org,
            "period": req.period,
        },
        "definition": def_ref,
        "message": "旧报表设计器兼容预览接口已恢复；新报表设计工作台使用 /api/report-design/*。",
    }))))
}
