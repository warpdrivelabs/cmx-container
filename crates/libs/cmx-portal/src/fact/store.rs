//! fact store 实现。

use serde::{Deserialize, Serialize};

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::read_json;
use crate::util::{is_safe_json_file, is_safe_segment};

/// 事实文件引用（domain/app/module/file）。
#[derive(Debug, Clone, Deserialize)]
pub struct FactRef {
    pub domain: String,
    pub app: String,
    pub module: String,
    pub file: String,
}

/// list 查询过滤（任一缺省则该级放宽）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FactQuery {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
}

/// list 返回项。
#[derive(Debug, Clone, Serialize)]
pub struct FactItem {
    pub domain: String,
    pub app: String,
    pub module: String,
    pub file: String,
}

/// 校验 DAM 三段 + file，返回相对路径段。
fn validate_ref(r: &FactRef) -> PortalResult<[String; 4]> {
    for (k, v) in [("domain", &r.domain), ("app", &r.app), ("module", &r.module)] {
        let t = v.trim();
        if t.is_empty() {
            return Err(PortalError::bad_request(format!("缺少必填参数 {k}")));
        }
        if !is_safe_segment(t) {
            return Err(PortalError::bad_request(format!(
                "参数 {k} 非法（仅允许字母、数字、_-）：\"{v}\""
            )));
        }
    }
    let file = r.file.trim();
    if !is_safe_json_file(file) {
        return Err(PortalError::bad_request(format!(
            "参数 file 非法（须 *.json，仅允许字母、数字、._-）：\"{}\"",
            r.file
        )));
    }
    Ok([
        r.domain.trim().to_string(),
        r.app.trim().to_string(),
        r.module.trim().to_string(),
        file.to_string(),
    ])
}

/// 读取某 DAM+file 的事实数据（原样 JSON）。
pub async fn get_fact(r: &FactRef) -> PortalResult<serde_json::Value> {
    let parts = validate_ref(r)?;
    let path = data_path(["fact", &parts[0], &parts[1], &parts[2], &parts[3]]);
    match read_json::<serde_json::Value>(&path).await {
        Ok(v) => Ok(v),
        Err(PortalError::NotFound(_)) => Err(PortalError::not_found(format!(
            "事实数据不存在：{}/{}/{}/{}",
            r.domain, r.app, r.module, r.file
        ))),
        Err(e) => Err(e),
    }
}

/// 列出某 DAM 目录下的事实文件（按 domain/app/module 逐级过滤）。
pub async fn list_facts(q: &FactQuery) -> PortalResult<Vec<FactItem>> {
    let root = data_path(["fact"]);
    let want_domain = q.domain.as_deref().unwrap_or("").trim().to_string();
    let want_app = q.app.as_deref().unwrap_or("").trim().to_string();
    let want_module = q.module.as_deref().unwrap_or("").trim().to_string();

    let mut out: Vec<FactItem> = Vec::new();
    // 深度固定为 domain/app/module/file，三层目录 + 文件。
    let mut dirs: Vec<(std::path::PathBuf, Vec<String>)> = vec![(root, Vec::new())];
    while let Some((dir, parts)) = dirs.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(PortalError::Io(e)),
        };
        while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let ft = entry.file_type().await.map_err(PortalError::Io)?;
            let mut next = parts.clone();
            next.push(name.clone());
            if ft.is_dir() {
                // 逐级过滤
                if next.len() == 1 && !want_domain.is_empty() && want_domain != name {
                    continue;
                }
                if next.len() == 2 && !want_app.is_empty() && want_app != name {
                    continue;
                }
                if next.len() == 3 && !want_module.is_empty() && want_module != name {
                    continue;
                }
                dirs.push((entry.path(), next));
            } else if ft.is_file() && name.ends_with(".json") && parts.len() == 3 {
                out.push(FactItem {
                    domain: parts[0].clone(),
                    app: parts[1].clone(),
                    module: parts[2].clone(),
                    file: name,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        format!("{}/{}/{}/{}", a.domain, a.app, a.module, a.file)
            .cmp(&format!("{}/{}/{}/{}", b.domain, b.app, b.module, b.file))
    });
    Ok(out)
}
