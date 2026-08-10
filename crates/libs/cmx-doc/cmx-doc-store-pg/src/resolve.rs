//! DOC 定义解析链路（读定义 + base + parse，带缓存）。
//!
//! 本模块是 doc-api / hier_service 共享的「读定义链路」单一出处，消除原先两份复刻：
//! - [`resolve_doc_file_smart`]：file 智能定位（`doc`(moduleCode) > `file` 显式 > 盲选默认）
//! - [`resolve_doc_meta`]：smart + 缓存 + DefRef + get_definition + load_base + parse
//! - [`load_base`]：读 `baseDocMetaRef.file` -> get_definition
//!
//! 对齐 dct 的 resolve_dict 在 store-pg 层的位置（定义解析下沉到 store，handler 只调用）。

use std::sync::Arc;

use cmx_api_types::{Error, Result};
use cmx_doc_model::DocMetaView;
use cmx_model_meta::definitions::coord::{self, DamPartial};
use cmx_model_meta::definitions::resolve::resolve_doc_file;
use cmx_model_meta::definitions::store::{get_definition, DefRef};
use serde_json::Value;

/// DocMetaView 缓存代数守卫：定义树代数变化（进程内写 bump / 带外手动改文件）时
/// 清空整个 DocMetaView 缓存，强制下次重读重解析——手动改定义无需重启。
async fn doc_cache_guard() {
    static LAST_SEEN: std::sync::OnceLock<std::sync::atomic::AtomicU64> =
        std::sync::OnceLock::new();
    // 注意：Rust 2024 中 `gen` 是保留关键字，按编译器建议加 `r#` 转义。
    let r#gen = coord::definitions_generation().await;
    let last = LAST_SEEN.get_or_init(|| std::sync::atomic::AtomicU64::new(0));
    if last.swap(r#gen, std::sync::atomic::Ordering::SeqCst) != r#gen {
        crate::cache::clear();
    }
}

/// DAM 坐标归一化：三段齐全 → 原样返回；缺失/部分 → 按 doc(moduleCode) > file 全局反查补全。
///
/// 三者全缺（DAM 不全且无 doc/file）→ 400：跨模块盲选默认不安全。
async fn normalize_coord(
    domain: Option<&str>,
    app: Option<&str>,
    module: Option<&str>,
    file: Option<&str>,
    doc: Option<&str>,
) -> Result<(String, String, String)> {
    let domain = coord::clean_str(domain);
    let app = coord::clean_str(app);
    let module = coord::clean_str(module);
    if let (Some(d), Some(a), Some(m)) = (domain, app, module) {
        return Ok((d.to_string(), a.to_string(), m.to_string()));
    }
    let partial = DamPartial {
        domain: domain.map(str::to_string),
        application: app.map(str::to_string),
        module: module.map(str::to_string),
    };
    let c = match (coord::clean_str(doc), coord::clean_str(file)) {
        (Some(dc), _) => coord::resolve_dam_by_code("DOC", dc, &partial).await?,
        (None, Some(f)) => coord::resolve_dam_by_file("DOC", f, &partial).await?,
        (None, None) => {
            return Err(Error::bad_request(
                "无法定位单据定义：请至少提供 doc(moduleCode)、file 或完整 DAM 坐标",
            ))
        }
    };
    Ok((c.domain, c.application, c.module))
}

/// 智能定位 DOC 定义文件名。
///
/// 调用方约定：DAM 三段坐标已在 [`resolve_doc_meta`] 层归一化为齐全 `&str` 后再传入本函数；
/// 本函数不再处理 DAM 缺失/部分，只负责 file/doc 的定位与脏值归一。
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
/// **DAM 咽喉点归一化**：`domain`/`app`/`module` 三段可选，缺失/部分时按
/// `doc`(moduleCode) > `file` 全局反查补全（详见 [`normalize_coord`]）；三者全缺返回 400。
/// 归一化后坐标齐全，再交由 [`resolve_doc_file_smart`] 落定 file。
///
/// 定位优先级（file 层；对齐 dct 的 resolve_dict：前端只传 code，file 由后端解析）：
///   1. `doc`（moduleCode）有值 -> [`resolve_doc_file`] 精确定位
///   2. `file` 显式指定且干净 -> 直接用
///   3. 都缺失 -> [`resolve_doc_file`] 盲选默认/最高版本
///
/// 返回 `(meta, file)`：file 是最终落定的定义文件名，供版本台账等需 file 的场景复用。
pub async fn resolve_doc_meta(
    domain: Option<&str>,
    app: Option<&str>,
    module: Option<&str>,
    file: Option<&str>,
    doc: Option<&str>,
) -> Result<(Arc<DocMetaView>, String)> {
    // 代数守卫：带外定义变更自动逐出缓存（手动改文件无需重启）。
    doc_cache_guard().await;
    // 坐标归一化：DAM 缺失/部分 → 全局反查补全（doc > file）。
    let (domain, app, module) = normalize_coord(domain, app, module, file, doc).await?;
    let file = resolve_doc_file_smart(&domain, &app, &module, file, doc).await?;
    let key = crate::cache::doc_key(&domain, &app, &module, &file);
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
