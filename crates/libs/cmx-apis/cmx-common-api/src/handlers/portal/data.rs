//! 事实数据 / 帮助中心 handler。

use axum::Json;
use axum::extract::{Path, Query};
use serde::Deserialize;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 事实文件四段路径参数（domain / app / module / file）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct FactPath {
    /// 所属域 id。
    pub domain: String,
    /// 所属应用 id。
    pub app: String,
    /// 所属模块 id。
    pub module: String,
    /// 事实文件名（须 `*.json`）。
    pub file: String,
}

/// 帮助文档路径参数（domain / app / module / file）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Path)]
pub struct HelpPath {
    /// 所属域 id。
    pub domain: String,
    /// 所属应用 id。
    pub app: String,
    /// 所属模块 id。
    pub module: String,
    /// 帮助文件名（须 `*.json`）。
    pub file: String,
}

/// 列出事实文件。
///
/// `GET /api/fact/list?domain=&app=&module=` —— 事实文件索引列表；三级过滤均可选，
/// 缺省则该级放宽。
#[utoipa::path(
    get,
    path = "/api/fact/list",
    params(
        ("domain" = Option<String>, Query, description = "域 id 过滤（可选）"),
        ("app" = Option<String>, Query, description = "应用 id 过滤（可选）"),
        ("module" = Option<String>, Query, description = "模块 id 过滤（可选）")
    ),
    responses(
        (status = 200, description = "事实文件索引 {items}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn list_facts(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::fact::store::FactQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::fact::store::list_facts(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// 读取事实文件。
///
/// `POST /api/fact/get` —— body `{ domain, app, module, file }`（file 须 `*.json`）。
#[utoipa::path(
    post,
    path = "/api/fact/get",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "事实文件内容 JSON", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn get_fact_post(
    CmxSvrContext(_c): CmxSvrContext,
    Json(r): Json<cmx_portal::fact::store::FactRef>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::fact::store::get_fact(&r).await?,
    )))
}

/// 读取事实文件。
///
/// `GET /api/fact/{domain}/{app}/{module}/{file}` —— 路径参数版，语义同
/// `POST /api/fact/get`。既有接口，保留路径参数（新接口规范不再如此设计）。
#[utoipa::path(
    get,
    path = "/api/fact/{domain}/{app}/{module}/{file}",
    params(FactPath),
    responses(
        (status = 200, description = "事实文件内容 JSON", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
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

/// 列出帮助目录。
///
/// `GET /api/help/catalog?domain=&app=&module=` —— 轻量目录项（不含正文 / 示例），
/// 供 explorer 搜索建树；三级过滤均可选，缺省则该级放宽。
#[utoipa::path(
    get,
    path = "/api/help/catalog",
    params(
        ("domain" = Option<String>, Query, description = "域 id 过滤（可选）"),
        ("app" = Option<String>, Query, description = "应用 id 过滤（可选）"),
        ("module" = Option<String>, Query, description = "模块 id 过滤（可选）")
    ),
    responses(
        (status = 200, description = "帮助目录轻量项 {items}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn help_catalog(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<cmx_portal::help::store::HelpQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_portal::help::store::list_catalog(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// 读取帮助文档。
///
/// `POST /api/help/get` —— 完整文档（含正文 / 示例）；body `{ domain, app, module, file }`。
#[utoipa::path(
    post,
    path = "/api/help/get",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "完整帮助文档（title / summary / content / examples 等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn help_get_post(
    CmxSvrContext(_c): CmxSvrContext,
    Json(r): Json<cmx_portal::help::store::HelpRef>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let doc = cmx_portal::help::store::get_doc(&r).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(doc).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// 读取帮助文档。
///
/// `GET /api/help/doc/{domain}/{app}/{module}/{file}` —— 路径参数版，语义同
/// `POST /api/help/get`。既有接口，保留路径参数（新接口规范不再如此设计）。
#[utoipa::path(
    get,
    path = "/api/help/doc/{domain}/{app}/{module}/{file}",
    params(HelpPath),
    responses(
        (status = 200, description = "完整帮助文档（title / summary / content / examples 等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
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

/// 保存帮助文档。
///
/// `POST /api/help/doc` —— upsert（新建 / 更新）。body：
///
/// ```json
/// {
///   "domain": "fi", "app": "cmxfico", "module": "gl",
///   "file": "缺省由 id 推导为 <id>.json",
///   "id": "主题 id（缺省由 file 推导）",
///   "path": "模块内分级路径（斜杠分级）",
///   "title": "文档标题", "summary": "摘要",
///   "keywords": ["搜索关键词"], "order": 1,
///   "content": "markdown 正文", "examples": [], "actions": {}
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/help/doc",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "保存结果 {saved}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn help_save_doc(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::help::store::HelpDocInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_portal::help::store::save_doc(input).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "saved": saved }))))
}

/// 删除帮助文档。
///
/// `DELETE /api/help/doc/{domain}/{app}/{module}/{file}` —— 按 DAM + file 删除。
/// 既有接口，保留 DELETE 方法（新接口规范不再如此设计）。
#[utoipa::path(
    delete,
    path = "/api/help/doc/{domain}/{app}/{module}/{file}",
    params(HelpPath),
    responses(
        (status = 200, description = "删除结果 {ok}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
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
