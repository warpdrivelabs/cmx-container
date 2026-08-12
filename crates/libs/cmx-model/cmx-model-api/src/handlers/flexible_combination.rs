//! 弹性组合 handler（cmx-model-api 层）。

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Result};

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
    fn to_ref(&self) -> cmx_model_meta::flexible_combination::store::FcRef {
        cmx_model_meta::flexible_combination::store::FcRef {
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
    let items = cmx_model_meta::flexible_combination::store::list_flexible_combinations(
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
        cmx_model_meta::flexible_combination::store::get_flexible_combination(&q.to_ref()).await?,
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
    let diagnostics =
        cmx_model_meta::flexible_combination::validator::validate_flexible_combination(&body);
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
        cmx_model_meta::flexible_combination::store::save_flexible_combination(&q.to_ref(), &body)
            .await?;
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
        cmx_model_meta::flexible_combination::store::delete_flexible_combination(&q.to_ref()).await?,
    )))
}

/// `POST /api/flexible-combination/default` —— 设为默认版本（同 scenario stem 互斥）。
pub async fn fc_set_default(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_meta::flexible_combination::store::set_default_version(&q.to_ref()).await?,
    )))
}

/// `GET /api/flexible-combination/resolve` —— 按锚点解析合并规则 → fields/columnModel。
pub async fn fc_resolve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_meta::flexible_combination::api::resolve(&q.to_ref(), &q.anchor_map()).await?,
    )))
}

/// `GET /api/flexible-combination/rule` —— 按锚点取规则 + 相关维度。
pub async fn fc_rule(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_meta::flexible_combination::api::rule(&q.to_ref(), &q.anchor_map()).await?,
    )))
}

/// `POST /api/flexible-combination/validate` —— 校验。
pub async fn fc_validate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<FcQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let diagnostics = cmx_model_meta::flexible_combination::api::validate(&body, &q.to_ref()).await?;
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
        cmx_model_meta::flexible_combination::api::preview(&body, &q.to_ref()).await?,
    )))
}
