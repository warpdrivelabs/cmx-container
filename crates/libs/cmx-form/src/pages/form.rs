//! 表单页存储：列表 JSON（`form-pages/pages-list.json`）+ 版本化文档
//! （`form-pages/sources/<id>_<ts>.json`，内容 `{ "form": "<CMX 表单 JSON 字符串>" }`）。
//!
//! 复刻 Node `lib/formPagesStore.js`：list（分页）/ save（版本化 upsert）/ get（读最新版本）。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, write_json_atomic};
use crate::util::{validate_id, write_lock};

/// 列表项摘要。
#[derive(Debug, Clone, Serialize)]
pub struct FormPageSummary {
    pub id: String,
    pub name: String,
    pub details: String,
}

/// 保存入参。
#[derive(Debug, Clone, Deserialize)]
pub struct FormPageInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub form: Option<String>,
}

fn list_path() -> std::path::PathBuf {
    data_path(["form-pages", "pages-list.json"])
}

fn source_path(fname: &str) -> std::path::PathBuf {
    data_path(["form-pages", "sources", fname])
}

/// 版本文件名 `<id>_<yyyyMMdd>_<HHmmssSSS>.json`（本地时间，与 Node 一致）。
fn version_file_name(id: &str) -> String {
    let now = chrono::Local::now();
    format!("{id}_{}.json", now.format("%Y%m%d_%H%M%S%3f"))
}

/// 读取列表文档的 pages 数组（缺失返回空）。
async fn load_pages() -> PortalResult<Vec<serde_json::Value>> {
    match read_json_opt(&list_path()).await? {
        Some(doc) => Ok(doc
            .get("pages")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

async fn persist_pages(pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(&list_path(), &json!({ "version": 1, "pages": pages }), true).await
}

/// 分页列出表单页。返回 `{ items, total, page, pageSize }`。
pub async fn list_form_pages_paged(page: Option<i64>, page_size: Option<i64>) -> PortalResult<serde_json::Value> {
    let p = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(20).clamp(1, 200);
    let pages = load_pages().await?;
    let total = pages.len() as i64;
    let start = ((p - 1) * size).max(0) as usize;
    let items: Vec<FormPageSummary> = pages
        .iter()
        .skip(start)
        .take(size as usize)
        .map(|row| FormPageSummary {
            id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            name: row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            details: row.get("details").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        })
        .collect();
    Ok(json!({ "items": items, "total": total, "page": p, "pageSize": size }))
}

/// 保存表单页（版本化 + 索引 upsert）。
pub async fn save_form_page(input: FormPageInput) -> PortalResult<serde_json::Value> {
    let id = validate_id(&input.id, "表单页 ID")?;
    let name = input.name.unwrap_or_default();
    let details = input.details.unwrap_or_default();
    let form = input
        .form
        .ok_or_else(|| PortalError::bad_request("form 必须为字符串（CMX 表单 JSON）"))?;

    let _guard = write_lock().lock().await;
    let fname = version_file_name(&id);
    write_json_atomic(&source_path(&fname), &json!({ "form": form }), true).await?;

    let mut pages = load_pages().await?;
    let row = json!({ "id": id, "name": name, "details": details, "latestFormFile": fname });
    if let Some(existing) = pages.iter_mut().find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str())) {
        *existing = row.clone();
    } else {
        pages.push(row.clone());
    }
    persist_pages(&pages).await?;
    Ok(row)
}

/// 按 id 读取表单页（含最新版本的 form 内容）。
pub async fn get_form_page_by_id(id: &str) -> PortalResult<serde_json::Value> {
    let pid = validate_id(id, "表单页 ID")?;
    let pages = load_pages().await?;
    let row = pages
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(pid.as_str()))
        .ok_or_else(|| PortalError::not_found("表单页不存在"))?;
    let fname = row
        .get("latestFormFile")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| PortalError::not_found("表单页未关联文档文件"))?;
    // 仅取 basename，防穿越
    let base = std::path::Path::new(fname)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let body = read_json_opt(&source_path(&base)).await?;
    let form = body
        .as_ref()
        .and_then(|b| b.get("form"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| PortalError::not_found("表单文档缺失或损坏"))?;
    Ok(json!({
        "id": row.get("id").cloned().unwrap_or(json!("")),
        "name": row.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "details": row.get("details").and_then(|v| v.as_str()).unwrap_or(""),
        "form": form,
    }))
}
