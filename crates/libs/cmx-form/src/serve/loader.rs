//! 页面资产加载器：索引 / v2 分片读取、安全路径拼接、源码装载与 rev 计算。
//!
//! 解析唯一规则：relPath 相对索引文件所在目录（native = `native_dir`，html = manifest
//! 所在的 `html_dir`），加载器不感知任何布局约定（规范见 [`super::config`] 模块文档）。
//! null 容错（`doc:null` 等显式 null → 空串）取自 mdm 版并全布局生效。

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::cache::content_rev;
use crate::pages::native::NativePageFull;

use super::error::PageServeError;

/// 把可能为 JSON `null` 的字符串字段反序列化为空串（serde `default` 只覆盖缺失字段，
/// 不覆盖显式 `null`——门户清单里 `doc:null` 写法会让整分片解析失败并静默清空）。
fn null_str<'de, D>(de: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(de)?.unwrap_or_default())
}

// ============================================================================
// native-pages：索引 index.json + 源文件（relPath 相对 native_dir）
// ============================================================================

/// native 索引行。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct IndexEntry {
    pub id: String,
    #[serde(default, deserialize_with = "null_str")]
    pub name: String,
    #[serde(default, deserialize_with = "null_str")]
    pub details: String,
    #[serde(default, rename = "sourceType", deserialize_with = "null_str")]
    pub source_type: String,
    #[serde(rename = "relPath")]
    pub rel_path: String,
}

#[derive(Debug, Deserialize)]
struct IndexFile {
    #[serde(default)]
    pages: Vec<IndexEntry>,
}

/// 读 native 页面索引（`<native_dir>/index.json`）。
///
/// 文件缺失 → 空集（未部署页面属常态，不告警）；存在但解析失败 → 空集 + warn
/// （降级哲学，绝不 500 整个服务）。
pub(crate) fn read_index(native_dir: &Path) -> Vec<IndexEntry> {
    let p = native_dir.join("index.json");
    match std::fs::read_to_string(&p) {
        Ok(t) => match serde_json::from_str::<IndexFile>(&t) {
            Ok(f) => f.pages,
            Err(e) => {
                tracing::warn!(
                    target: "cmx::form::serve",
                    path = %p.display(),
                    error = %e,
                    "native 页索引解析失败，降级为空集"
                );
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    }
}

/// 安全拼接源文件绝对路径：relPath 相对 `base`（索引文件所在目录），
/// 拒绝空段与 `..` 越界。
pub(crate) fn safe_join(base: &Path, rel: &str) -> Option<std::path::PathBuf> {
    if rel.is_empty() {
        return None;
    }
    let mut p = base.to_path_buf();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == ".." {
            return None;
        }
        p.push(seg);
    }
    Some(p)
}

/// 由 relPath 扩展名推导 sourceType 兜底（js/mjs → js；html/htm → html；其余空串）。
pub(crate) fn source_type_from_rel(rel: &str) -> String {
    let l = rel.to_lowercase();
    if l.ends_with(".js") || l.ends_with(".mjs") {
        "js".into()
    } else if l.ends_with(".html") || l.ends_with(".htm") {
        "html".into()
    } else {
        String::new()
    }
}

/// 由索引项 + 源码组装 `NativePageFull`（源文件缺失 → NotFound）。
pub(crate) fn load_native_full(
    native_dir: &Path,
    e: &IndexEntry,
) -> Result<NativePageFull, PageServeError> {
    let abs = safe_join(native_dir, &e.rel_path)
        .ok_or_else(|| PageServeError::BadRequest(format!("native page relPath 非法: {}", e.rel_path)))?;
    let source = std::fs::read_to_string(&abs).map_err(|_| {
        PageServeError::NotFound(format!("native page 源文件缺失: {}", e.rel_path))
    })?;
    let rev = content_rev(source.as_bytes());
    Ok(NativePageFull {
        id: e.id.clone(),
        name: e.name.clone(),
        details: e.details.clone(),
        source_type: if e.source_type.is_empty() {
            source_type_from_rel(&e.rel_path)
        } else {
            e.source_type.clone()
        },
        rel_path: e.rel_path.clone(),
        rev,
        source,
    })
}

// ============================================================================
// html-pages：v2 manifest index.json + 分片 index/<domain>.pages.json
// （行内 relPath 相对 html_dir，即 manifest 所在目录）
// ============================================================================

/// html 索引行（id 为 domain.app.module.page 命名空间）。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HtmlRow {
    pub id: String,
    #[serde(default, deserialize_with = "null_str")]
    pub name: String,
    #[serde(default, deserialize_with = "null_str")]
    pub details: String,
    #[serde(default, deserialize_with = "null_str")]
    pub domain: String,
    #[serde(default, deserialize_with = "null_str")]
    pub app: String,
    #[serde(default, deserialize_with = "null_str")]
    pub module: String,
    #[serde(default, deserialize_with = "null_str")]
    pub doc: String,
    #[serde(rename = "relPath")]
    pub rel_path: String,
}

#[derive(Debug, Deserialize)]
struct HtmlManifest {
    #[serde(default)]
    domains: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HtmlShard {
    #[serde(default)]
    pages: Vec<HtmlRow>,
}

/// 读全部 html 行（遍历 manifest 声明的每个域分片）。
///
/// manifest 缺失/坏解析 → 空集（manifest 解析失败补 warn）；单分片坏解析 → 跳过 + warn。
pub(crate) fn read_html_rows(html_dir: &Path) -> Vec<HtmlRow> {
    let manifest_path = html_dir.join("index.json");
    let manifest: HtmlManifest = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => match serde_json::from_str(&t) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    target: "cmx::form::serve",
                    path = %manifest_path.display(),
                    error = %e,
                    "html manifest 解析失败，降级为空集"
                );
                return Vec::new();
            }
        },
        Err(_) => HtmlManifest { domains: Vec::new() },
    };
    let mut rows = Vec::new();
    for dom in &manifest.domains {
        let shard = html_dir.join("index").join(format!("{dom}.pages.json"));
        if let Ok(t) = std::fs::read_to_string(&shard) {
            match serde_json::from_str::<HtmlShard>(&t) {
                Ok(s) => rows.extend(s.pages),
                Err(e) => {
                    tracing::warn!(
                        target: "cmx::form::serve",
                        path = %shard.display(),
                        error = %e,
                        "html 分片解析失败，跳过该域"
                    );
                }
            }
        }
    }
    rows
}

/// 由 html 行 + 源码组装门户同构 JSON（字段与插入序对齐门户 read_full_from_row）。
pub(crate) fn load_html_full(
    html_dir: &Path,
    r: &HtmlRow,
) -> Result<Value, PageServeError> {
    let abs = safe_join(html_dir, &r.rel_path)
        .ok_or_else(|| PageServeError::BadRequest(format!("html page relPath 非法: {}", r.rel_path)))?;
    let html = std::fs::read_to_string(&abs).map_err(|_| {
        PageServeError::NotFound(format!("HTML 源码文件缺失或损坏: {}", r.rel_path))
    })?;
    let rev = content_rev(html.as_bytes());
    Ok(json!({
        "id": r.id, "name": r.name, "details": r.details,
        "domain": r.domain, "app": r.app, "module": r.module, "doc": r.doc,
        "relPath": r.rel_path, "rev": rev, "html": html,
    }))
}
