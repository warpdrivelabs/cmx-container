//! 事实数据 / 帮助中心 handler。

use axum::Json;
use axum::extract::{Path, Query};
use serde::Deserialize;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

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

/// `GET /api/fact/list?domain=&app=&module=` —— 列出事实文件。
pub async fn list_facts(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::fact::store::FactQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::fact::store::list_facts(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/fact/get` —— 读取事实文件（请求体 { domain, app, module, file }）。
pub async fn get_fact_post(
    CmxSvrContext(_c): CmxSvrContext,
    Json(r): Json<cmx_portal::fact::store::FactRef>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::fact::store::get_fact(&r).await?,
    )))
}

/// `GET /api/fact/:domain/:app/:module/:file` —— 读取事实文件（路径参数）。
pub async fn get_fact_path(
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

/// `GET /api/help/catalog?domain=&app=&module=` —— 帮助目录（轻量项，供 explorer 搜索建树）。
pub async fn help_catalog(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::help::store::HelpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::help::store::list_catalog(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `POST /api/help/get` —— 读取完整帮助文档（请求体 { domain, app, module, file }）。
pub async fn help_get_post(
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
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::help::store::HelpDocInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::help::store::save_doc(input).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// `DELETE /api/help/doc/:domain/:app/:module/:file` —— 删除帮助文档。
pub async fn help_delete_doc(
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
