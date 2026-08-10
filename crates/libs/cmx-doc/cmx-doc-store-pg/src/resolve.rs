//! DOC 定义解析链路（读定义 + base + parse，带缓存）。
//!
//! 本模块是 doc-api / hier_service 共享的「读定义链路」单一出处，消除原先两份复刻：
//! - [`resolve_doc_file_smart`]：file 智能定位（`doc`(moduleCode) > `file` 显式 > 盲选默认）
//! - [`resolve_doc_meta`]：smart + 缓存 + DefRef + get_definition + load_base + parse
//! - [`load_base`]：读 `baseDocMetaRef.file` -> get_definition
//!
//! 对齐 dct 的 resolve_dict 在 store-pg 层的位置（定义解析下沉到 store，handler 只调用）。

use std::sync::Arc;

use cmx_api_types::Result;
use cmx_doc_model::DocMetaView;
use cmx_model_meta::definitions::resolve::resolve_doc_file;
use cmx_model_meta::definitions::store::{get_definition, DefRef};
use serde_json::Value;

/// 智能定位 DOC 定义文件名。
///
/// 定位优先级（对齐 dct 的 resolve_dict：前端只传 code，file 由后端解析）：
///   1. `doc`（moduleCode）有值 -> [`resolve_doc_file`] 精确定位（读 moduleMeta.moduleCode 匹配）
///   2. `file` 显式指定且干净 -> 直接用（覆盖场景，如版本台账 restore 指定历史文件）
///   3. 都缺失 -> [`resolve_doc_file`] 盲选默认/最高版本
///
/// `file`/`doc` 的脏值（空串 / "undefined" / "null"）一律视为缺失。
/// 返回最终落定的 file 名。
pub async fn resolve_doc_file_smart(
    domain: &str,
    app: &str,
    module: &str,
    file: Option<&str>,
    doc: Option<&str>,
) -> Result<String> {
    // 脏值（空串 / "undefined" / "null"）一律视为缺失
    let doc = doc.filter(|v| !v.is_empty() && *v != "undefined" && *v != "null");
    let file = file.filter(|v| !v.is_empty() && *v != "undefined" && *v != "null");
    match doc {
        Some(d) => resolve_doc_file(domain, app, module, Some(d)).await,
        _ => match file {
            Some(f) => Ok(f.to_string()),
            None => resolve_doc_file(domain, app, module, None).await,
        },
    }
}

/// 读单据定义 + base 字段集，解析为 DocMetaView（命中缓存则直接返回）。
///
/// 定位优先级（对齐 dct 的 resolve_dict：前端只传 code，file 由后端解析）：
///   1. `doc`（moduleCode）有值 -> [`resolve_doc_file`] 精确定位
///   2. `file` 显式指定且干净 -> 直接用
///   3. 都缺失 -> [`resolve_doc_file`] 盲选默认/最高版本
///
/// 返回 `(meta, file)`：file 是最终落定的定义文件名，供版本台账等需 file 的场景复用。
pub async fn resolve_doc_meta(
    domain: &str,
    app: &str,
    module: &str,
    file: Option<&str>,
    doc: Option<&str>,
) -> Result<(Arc<DocMetaView>, String)> {
    let file = resolve_doc_file_smart(domain, app, module, file, doc).await?;
    let key = crate::cache::doc_key(domain, app, module, &file);
    if let Some(hit) = crate::cache::get(&key) {
        return Ok((hit, file));
    }

    // 读主定义
    let doc_ref = DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    let doc = get_definition(&doc_ref).await?;

    // 读 base 字段集（从 baseDocMetaRef.file 推断；无则空）
    let base = load_base(&doc).await;

    let view = Arc::new(DocMetaView::parse(&doc, &base)?);
    crate::cache::put(key, view.clone());
    Ok((view, file))
}

/// 从定义的 baseDocMetaRef.file 读 base 字段集（域=base）；失败返回 Null。
pub async fn load_base(doc: &Value) -> Value {
    let base_file = doc
        .get("baseDocMetaRef")
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str());
    let Some(base_file) = base_file else {
        return Value::Null;
    };
    let base_ref = DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(base_file.to_string()),
        id: None,
        kind: None,
    };
    get_definition(&base_ref).await.unwrap_or(Value::Null)
}
