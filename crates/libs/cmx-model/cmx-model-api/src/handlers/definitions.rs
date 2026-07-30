//! 定义中心（DCT/DOC/BASE）handler（cmx-model-api 层）。

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use cmx_api::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

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
    // 元数据定义定位主键（值须为 *.json 文件名）。file 字段保留以兼容旧调用方；
    // 定位段取 file 优先、缺省回退 id（见 DefRef::file_value）。前端统一只发 id。
    #[serde(default)]
    pub id: Option<String>,
}

impl DefQuery {
    fn to_ref(&self) -> cmx_model_meta::definitions::store::DefRef {
        cmx_model_meta::definitions::store::DefRef {
            domain: self.domain.clone(),
            application: self.application.clone(),
            app: None,
            module: self.module.clone(),
            file: self.file.clone(),
            id: self.id.clone(),
            kind: self.kind.clone(),
        }
    }
}

// ───────────────────────── 定义中心 ─────────────────────────

/// `GET /api/definitions/list?kind=&domain=&application=&module=` —— 列表。
pub async fn definitions_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let items = cmx_model_meta::definitions::store::list_definitions(
        q.kind.as_deref(),
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "items": items }))))
}

/// `GET /api/definitions/config?domain=&application=&module=&file=&id=` —— 读单个定义。
///
/// 定位段取 `file` 优先、`id` 兜底（与 DefRef::file_value 一致）：
/// - 以 `.json` 结尾（显式文件名）→ 直接按文件路径读。
/// - 非 `.json`（业务编码）→ 按 kind 反查默认/最新版本文件：
///   BASE 调 resolve_base_file（按 moduleCode，仅需 domain=base）；
///   DOC 调 resolve_doc_file（按 moduleMeta.moduleCode，需 domain/app/module 坐标）；
///   DCT 调 resolve_dict_file（按 dictCode，需坐标）并过滤 dictionaryTables 只返回命中单表。
pub async fn definitions_get(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let id_val = q.file.clone().or_else(|| q.id.clone());
    let is_filename = id_val
        .as_deref()
        .map(|v| v.ends_with(".json"))
        .unwrap_or(true);
    if is_filename {
        return Ok(Json(ApiResp::ok(
            cmx_model_meta::definitions::store::get_definition(&q.to_ref()).await?,
        )));
    }
    // 业务编码定位：按 kind 分流
    // - BASE：仅需 domain=base + id=<moduleCode>（如 base_dct_meta），无需 application/module。
    // - DOC/DCT：需 kind/domain/application/module 四段齐全。
    let code = id_val.as_deref().unwrap_or("");
    let domain = q.domain.as_deref().unwrap_or("");
    let app = q.application.as_deref().unwrap_or("");
    let module = q.module.as_deref().unwrap_or("");
    let kind = q.kind.as_deref().unwrap_or("");
    if kind == "BASE" {
        if domain.is_empty() {
            return Err(cmx_api_types::Error::BadRequest(
                "BASE 业务编码定位需要 domain（约定为 base）".into(),
            ));
        }
        if code.is_empty() {
            return Err(cmx_api_types::Error::BadRequest(
                "BASE 业务编码定位需要 id（= moduleCode，如 base_dct_meta）".into(),
            ));
        }
        let file = cmx_model_meta::definitions::resolve::resolve_base_file(domain, code).await?;
        let mut doc = cmx_model_meta::definitions::store::get_definition(
            &cmx_model_meta::definitions::store::DefRef {
                domain: Some(domain.to_string()),
                file: Some(file),
                ..Default::default()
            },
        )
        .await?;
        return Ok(Json(ApiResp::ok(doc.take())));
    }
    if domain.is_empty() || app.is_empty() || module.is_empty() {
        return Err(cmx_api_types::Error::BadRequest(
            "业务编码定位需要 kind/domain/application/module 坐标".into(),
        ));
    }
    let mut doc = match kind {
        "DOC" => {
            let file =
                cmx_model_meta::definitions::resolve::resolve_doc_file(domain, app, module, Some(code))
                    .await?;
            cmx_model_meta::definitions::store::get_definition(
                &cmx_model_meta::definitions::store::DefRef {
                    domain: Some(domain.to_string()),
                    application: Some(app.to_string()),
                    module: Some(module.to_string()),
                    file: Some(file),
                    ..Default::default()
                },
            )
            .await?
        }
        "DCT" => {
            let file =
                cmx_model_meta::definitions::resolve::resolve_dict_file(domain, app, module, code)
                    .await?;
            let mut d = cmx_model_meta::definitions::store::get_definition(
                &cmx_model_meta::definitions::store::DefRef {
                    domain: Some(domain.to_string()),
                    application: Some(app.to_string()),
                    module: Some(module.to_string()),
                    file: Some(file),
                    ..Default::default()
                },
            )
            .await?;
            // 单表化：只保留命中的那张字典表（dictCode/tableName 任一匹配），保留 moduleMeta 头与 baseDctMetaRef
            if let Some(tables) = d.get_mut("dictionaryTables").and_then(|v| v.as_array_mut()) {
                tables.retain(|t| cmx_model_meta::definitions::resolve::dict_matches(t, code));
            }
            d
        }
        other => {
            return Err(cmx_api_types::Error::BadRequest(format!(
                "业务编码定位仅支持 kind=BASE/DOC/DCT，收到 {other:?}"
            )));
        }
    };
    Ok(Json(ApiResp::ok(doc.take())))
}

/// `POST /api/definitions/config?domain=&...&file=` —— 保存定义（body 为文档）。
pub async fn definitions_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let saved = cmx_model_meta::definitions::store::save_definition(&q.to_ref(), &body).await?;
    Ok(Json(ApiResp::ok(
        serde_json::json!({ "ok": true, "saved": saved }),
    )))
}

/// `POST /api/definitions/batch` —— 批量读 + base 字段集。
///
/// ref 的定位段（file/id）为业务编码（非 .json）时按 kind 反查：
/// DOC 按 moduleCode、DCT 按 dictCode（且结果只保留命中单表）。
/// 反查在 handler 层完成（改写成 .json ref 后交给 store），DCT 单表过滤在结果回写后做。
pub async fn definitions_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let raw_refs = body
        .get("refs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    // 需反查的 DCT ref：反查后的文件名 → dictCode（用于结果单表过滤）
    let mut dct_filters: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut rewritten: Vec<serde_json::Value> = Vec::with_capacity(raw_refs.len());
    for r in &raw_refs {
        // 字符串 ref 或 .json 对象 ref：原样透传
        let obj = match r.as_object() {
            Some(o) => o,
            None => {
                rewritten.push(r.clone());
                continue;
            }
        };
        let id_val = obj
            .get("file")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if id_val.is_empty() || id_val.ends_with(".json") {
            rewritten.push(r.clone());
            continue;
        }
        // 业务编码：按 kind 反查文件名
        let kind = obj.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let domain = obj.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let app = obj
            .get("application")
            .or_else(|| obj.get("app"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let module = obj.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let file = match kind {
            "DOC" => {
                cmx_model_meta::definitions::resolve::resolve_doc_file(
                    domain,
                    app,
                    module,
                    Some(id_val),
                )
                .await?
            }
            "DCT" => {
                let f = cmx_model_meta::definitions::resolve::resolve_dict_file(
                    domain, app, module, id_val,
                )
                .await?;
                dct_filters.insert(f.clone(), id_val.to_string());
                f
            }
            _ => {
                return Err(cmx_api_types::Error::BadRequest(format!(
                    "业务编码批量定位仅支持 kind=DOC/DCT，收到 ref={r}"
                )));
            }
        };
        let mut new_obj = obj.clone();
        new_obj.insert("file".into(), serde_json::json!(file));
        new_obj.remove("id");
        rewritten.push(serde_json::Value::Object(new_obj));
    }
    let mut new_body = body.clone();
    new_body["refs"] = serde_json::Value::Array(rewritten);
    let mut result = cmx_model_meta::definitions::store::get_definitions_batch(&new_body).await?;
    // DCT 单表过滤：对记录的 DCT item 只保留命中 dictCode 的那张表
    if !dct_filters.is_empty()
        && let Some(items) = result.get_mut("items").and_then(|v| v.as_array_mut())
    {
        for it in items.iter_mut() {
            let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(code) = dct_filters.get(file)
                && let Some(tables) = it
                    .get_mut("doc")
                    .and_then(|d| d.get_mut("dictionaryTables"))
                    .and_then(|v| v.as_array_mut())
            {
                tables.retain(|t| cmx_model_meta::definitions::resolve::dict_matches(t, code));
            }
        }
    }
    Ok(Json(ApiResp::ok(result)))
}

/// `DELETE /api/definitions/config?domain=&...&file=` —— 删除定义。
pub async fn definitions_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_meta::definitions::store::delete_definition(&q.to_ref()).await?,
    )))
}

/// `POST /api/definitions/default?domain=&...&file=` —— 设为默认版本（同 stem 互斥）。
pub async fn definitions_set_default(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<DefQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_model_meta::definitions::store::set_default_version(&q.to_ref()).await?,
    )))
}
