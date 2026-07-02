//! 定义中心 store 实现。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json, write_json_atomic};
use crate::util::{is_safe_json_file, is_safe_segment, write_lock};

/// 定义文件引用。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DefRef {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub application: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

impl DefRef {
    fn app_value(&self) -> Option<&str> {
        self.application.as_deref().or(self.app.as_deref())
    }
    fn file_value(&self) -> Option<&str> {
        self.file.as_deref().or(self.id.as_deref())
    }
}

/// 校验四段并返回相对 definitions 根的路径段。
fn resolve_rel(r: &DefRef) -> PortalResult<Vec<String>> {
    let domain = r.domain.as_deref().unwrap_or("").trim();
    if domain.is_empty() {
        return Err(PortalError::bad_request("缺少必填参数 domain"));
    }
    if !is_safe_segment(domain) {
        return Err(PortalError::bad_request(format!(
            "参数 domain 非法（仅允许字母、数字、_-）：\"{domain}\""
        )));
    }
    let file = r.file_value().unwrap_or("").trim();
    if !is_safe_json_file(file) {
        return Err(PortalError::bad_request(format!(
            "参数 file 非法（须 *.json，仅允许字母、数字、._-）：\"{file}\""
        )));
    }
    if domain == "base" {
        return Ok(vec!["base".to_string(), file.to_string()]);
    }
    let app = r.app_value().unwrap_or("").trim();
    if app.is_empty() {
        return Err(PortalError::bad_request("缺少必填参数 application"));
    }
    if !is_safe_segment(app) {
        return Err(PortalError::bad_request(format!(
            "参数 application 非法（仅允许字母、数字、_-）：\"{app}\""
        )));
    }
    let module = r.module.as_deref().unwrap_or("").trim();
    if module.is_empty() {
        return Err(PortalError::bad_request("缺少必填参数 module"));
    }
    if !is_safe_segment(module) {
        return Err(PortalError::bad_request(format!(
            "参数 module 非法（仅允许字母、数字、_-）：\"{module}\""
        )));
    }
    Ok(vec![
        domain.to_string(),
        app.to_string(),
        module.to_string(),
        file.to_string(),
    ])
}

fn abs_path(rel_parts: &[String]) -> std::path::PathBuf {
    let mut p = data_path(["meta", "definitions"]);
    for seg in rel_parts {
        p.push(seg);
    }
    p
}

/// 读取定义文件（原样 JSON）。
pub async fn get_definition(r: &DefRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    match read_json::<serde_json::Value>(&abs_path(&rel)).await {
        Ok(v) => Ok(v),
        Err(PortalError::NotFound(_)) => Err(PortalError::not_found(format!(
            "定义文件不存在：{}/{}/{}/{}",
            r.domain.as_deref().unwrap_or(""),
            r.app_value().unwrap_or(""),
            r.module.as_deref().unwrap_or(""),
            r.file_value().unwrap_or("")
        ))),
        Err(e) => Err(e),
    }
}

/// 由文档推断 base 文件名（DCT→baseDctMetaRef.file，DOC→baseDocMetaRef.file）。
fn infer_base_file(doc: &serde_json::Value) -> Option<String> {
    let kind = doc
        .get("moduleMeta")
        .and_then(|m| m.get("metaKind"))
        .and_then(|v| v.as_str());
    let key = match kind {
        Some("DCT") => "baseDctMetaRef",
        Some("DOC") => "baseDocMetaRef",
        _ => return None,
    };
    doc.get(key)
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 文档 kind 判定。
fn doc_kind(doc: &serde_json::Value) -> String {
    if let Some(k) = doc
        .get("moduleMeta")
        .and_then(|m| m.get("metaKind"))
        .and_then(|v| v.as_str())
    {
        return k.to_string();
    }
    if doc.get("baseMeta").is_some() {
        "BASE".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

/// 批量读取定义 + 附带 base 字段集（去重）。
pub async fn get_definitions_batch(input: &serde_json::Value) -> PortalResult<serde_json::Value> {
    let include_base = input
        .get("includeBase")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // refs：{ refs: [...] } 或顶层数组；元素为字符串(=file) 或对象
    let raw_refs: Vec<serde_json::Value> =
        if let Some(arr) = input.get("refs").and_then(|v| v.as_array()) {
            arr.clone()
        } else if let Some(arr) = input.as_array() {
            arr.clone()
        } else {
            vec![]
        };
    let refs: Vec<DefRef> = raw_refs
        .iter()
        .map(|r| {
            if let Some(s) = r.as_str() {
                DefRef {
                    file: Some(s.to_string()),
                    ..Default::default()
                }
            } else {
                serde_json::from_value::<DefRef>(r.clone()).unwrap_or_default()
            }
        })
        .collect();

    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut base_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for r in &refs {
        match get_definition(r).await {
            Ok(doc) => {
                let kind = doc_kind(&doc);
                let rel = resolve_rel(r).unwrap_or_default();
                items.push(json!({
                    "domain": r.domain.clone().unwrap_or_default(),
                    "application": r.app_value().unwrap_or("").to_string(),
                    "app": r.app_value().unwrap_or("").to_string(),
                    "module": r.module.clone().unwrap_or_default(),
                    "file": r.file_value().unwrap_or("").to_string(),
                    "kind": kind,
                    "path": abs_path(&rel).to_string_lossy(),
                    "relPath": rel.join("/"),
                    "doc": doc.clone(),
                }));
                if include_base
                    && let Some(bf) = infer_base_file(&doc) {
                        base_files.insert(bf);
                    }
            }
            Err(e) => errors.push(json!({ "ref": serde_json::to_value(r).unwrap_or(json!({})), "error": e.to_string() })),
        }
    }

    let mut bases = serde_json::Map::new();
    let mut base_paths = serde_json::Map::new();
    if include_base {
        for file in &base_files {
            let bref = DefRef {
                domain: Some("base".to_string()),
                file: Some(file.clone()),
                ..Default::default()
            };
            match get_definition(&bref).await {
                Ok(doc) => {
                    bases.insert(file.clone(), doc);
                    let rel = resolve_rel(&bref).unwrap_or_default();
                    base_paths.insert(file.clone(), json!({ "path": abs_path(&rel).to_string_lossy(), "relPath": rel.join("/") }));
                }
                Err(e) => errors.push(
                    json!({ "ref": { "domain": "base", "file": file }, "error": e.to_string() }),
                ),
            }
        }
    }

    Ok(json!({ "items": items, "bases": bases, "basePaths": base_paths, "errors": errors }))
}

/// 剥去定义文件名末尾的 `_v<N>.json` 版本后缀，返回逻辑定义 stem（用于多版本聚合）。
/// 例：`cmxfico_dct_meta_v1.json` → `cmxfico_dct_meta`；无版本后缀则去掉 `.json`。
fn def_file_stem(file: &str) -> String {
    let base = file.strip_suffix(".json").unwrap_or(file);
    if let Some(idx) = base.rfind("_v") {
        let suffix = &base[idx + 2..];
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
            return base[..idx].to_string();
        }
    }
    base.to_string()
}

/// 由已解析文档抽取列表摘要。
fn summarize(
    domain: &str,
    application: &str,
    module: &str,
    file: &str,
    doc: &serde_json::Value,
) -> serde_json::Value {
    let mm = doc.get("moduleMeta").cloned().unwrap_or(json!({}));
    let kind = doc_kind(doc);
    let version = mm
        .get("version")
        .or_else(|| doc.get("baseMeta").and_then(|b| b.get("version")))
        .cloned()
        .unwrap_or(json!(1));
    // 版本名称（多版本下拉展示用，文件名承载不了，存 moduleMeta/baseMeta.versionName）。
    let version_name = mm
        .get("versionName")
        .or_else(|| doc.get("baseMeta").and_then(|b| b.get("versionName")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // 是否默认版本（多版本管理用，同 stem 组内至多一个为 true）。
    let is_default = mm
        .get("isDefault")
        .or_else(|| doc.get("baseMeta").and_then(|b| b.get("isDefault")))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // 最后修改时间（save 时写入，供版本管理列表展示）。
    let updated_at = doc
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // stem：剥去文件名末尾的 _v<N>.json，使前端可按「逻辑定义」聚合多版本文件。
    let stem = def_file_stem(file);
    let mut base = json!({
        "domain": domain, "application": application, "app": application,
        "module": module, "file": file, "kind": kind,
        "version": version, "versionName": version_name,
        "isDefault": is_default, "updatedAt": updated_at, "stem": stem,
    });
    let obj = base.as_object_mut().unwrap();
    match kind.as_str() {
        "DCT" => {
            obj.insert(
                "title".into(),
                json!(
                    mm.get("moduleName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file)
                ),
            );
            obj.insert(
                "moduleCode".into(),
                json!(mm.get("moduleCode").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "remark".into(),
                json!(mm.get("remark").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "tableCount".into(),
                json!(
                    doc.get("dictionaryTables")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0)
                ),
            );
        }
        "DOC" => {
            obj.insert(
                "title".into(),
                json!(
                    mm.get("moduleName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file)
                ),
            );
            obj.insert(
                "moduleCode".into(),
                json!(mm.get("moduleCode").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "remark".into(),
                json!(mm.get("remark").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "tableCount".into(),
                json!(
                    doc.get("voucherTables")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0)
                ),
            );
        }
        "BASE" => {
            let bm = doc.get("baseMeta").cloned().unwrap_or(json!({}));
            obj.insert(
                "title".into(),
                json!(bm.get("metaName").and_then(|v| v.as_str()).unwrap_or(file)),
            );
            obj.insert(
                "moduleCode".into(),
                json!(bm.get("metaCode").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "remark".into(),
                json!(bm.get("remark").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "fieldSetCount".into(),
                json!(
                    doc.get("fieldSets")
                        .and_then(|v| v.as_object())
                        .map(|o| o.len())
                        .unwrap_or(0)
                ),
            );
        }
        _ => {
            obj.insert("title".into(), json!(file));
        }
    }
    base
}

/// 列出定义（kind/domain/application/module 过滤）。
pub async fn list_definitions(
    kind: Option<&str>,
    domain: Option<&str>,
    application: Option<&str>,
    module: Option<&str>,
) -> PortalResult<Vec<serde_json::Value>> {
    let root = data_path(["meta", "definitions"]);
    let want_kind = kind.unwrap_or("").to_uppercase();
    let want_domain = domain.unwrap_or("").trim().to_string();
    let want_app = application.unwrap_or("").trim().to_string();
    let want_module = module.unwrap_or("").trim().to_string();

    let mut out: Vec<serde_json::Value> = Vec::new();

    let mut domains = match tokio::fs::read_dir(&root).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(PortalError::Io(e)),
    };
    while let Some(d) = domains.next_entry().await.map_err(PortalError::Io)? {
        let dname = d.file_name().to_string_lossy().to_string();
        if dname.starts_with('.') || !d.file_type().await.map_err(PortalError::Io)?.is_dir() {
            continue;
        }
        if dname == "base" {
            if !want_domain.is_empty() && want_domain != "base" {
                continue;
            }
            push_files_in_dir(&d.path(), "base", "", "", &want_kind, &mut out).await?;
            continue;
        }
        if !want_domain.is_empty() && want_domain != dname {
            continue;
        }
        let mut apps = match tokio::fs::read_dir(d.path()).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Some(a) = apps.next_entry().await.map_err(PortalError::Io)? {
            let aname = a.file_name().to_string_lossy().to_string();
            if aname.starts_with('.') || !a.file_type().await.map_err(PortalError::Io)?.is_dir() {
                continue;
            }
            if !want_app.is_empty() && want_app != aname {
                continue;
            }
            let mut mods = match tokio::fs::read_dir(a.path()).await {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            while let Some(m) = mods.next_entry().await.map_err(PortalError::Io)? {
                let mname = m.file_name().to_string_lossy().to_string();
                if mname.starts_with('.') || !m.file_type().await.map_err(PortalError::Io)?.is_dir()
                {
                    continue;
                }
                if !want_module.is_empty() && want_module != mname {
                    continue;
                }
                push_files_in_dir(&m.path(), &dname, &aname, &mname, &want_kind, &mut out).await?;
            }
        }
    }
    out.sort_by(|a, b| {
        let ka = format!(
            "{}/{}/{}/{}",
            a["domain"].as_str().unwrap_or(""),
            a["application"].as_str().unwrap_or(""),
            a["module"].as_str().unwrap_or(""),
            a["file"].as_str().unwrap_or("")
        );
        let kb = format!(
            "{}/{}/{}/{}",
            b["domain"].as_str().unwrap_or(""),
            b["application"].as_str().unwrap_or(""),
            b["module"].as_str().unwrap_or(""),
            b["file"].as_str().unwrap_or("")
        );
        ka.cmp(&kb)
    });
    Ok(out)
}

/// 扫描一个目录下的 *.json，summarize 后按 wantKind 过滤后推入 out。
async fn push_files_in_dir(
    dir: &std::path::Path,
    domain: &str,
    application: &str,
    module: &str,
    want_kind: &str,
    out: &mut Vec<serde_json::Value>,
) -> PortalResult<()> {
    let mut files = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    while let Some(f) = files.next_entry().await.map_err(PortalError::Io)? {
        let fname = f.file_name().to_string_lossy().to_string();
        if !f.file_type().await.map_err(PortalError::Io)?.is_file() || !is_safe_json_file(&fname) {
            continue;
        }
        match read_json::<serde_json::Value>(&f.path()).await {
            Ok(doc) => {
                let item = summarize(domain, application, module, &fname, &doc);
                if !want_kind.is_empty() && item["kind"].as_str().unwrap_or("") != want_kind {
                    continue;
                }
                out.push(item);
            }
            Err(e) => out.push(json!({
                "domain": domain, "application": application, "app": application,
                "module": module, "file": fname, "kind": "UNKNOWN", "error": e.to_string()
            })),
        }
    }
    Ok(())
}

/// 保存定义（补 updatedAt，原子写）。
pub async fn save_definition(
    r: &DefRef,
    doc: &serde_json::Value,
) -> PortalResult<serde_json::Value> {
    if !doc.is_object() {
        return Err(PortalError::bad_request("请求体必须是对象"));
    }
    let rel = resolve_rel(r)?;
    let mut merged = doc.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert(
            "updatedAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        );
    }
    let _guard = write_lock().lock().await;
    write_json_atomic(&abs_path(&rel), &merged, true).await?;
    Ok(merged)
}

/// 在文档的 moduleMeta（或 baseMeta）上写 isDefault 标记，返回是否有改动。
fn set_doc_default_flag(doc: &mut serde_json::Value, value: bool) -> bool {
    let key = if doc.get("baseMeta").is_some() && doc.get("moduleMeta").is_none() {
        "baseMeta"
    } else {
        "moduleMeta"
    };
    let obj = doc.as_object_mut();
    let Some(obj) = obj else { return false };
    let meta = obj.entry(key).or_insert_with(|| json!({}));
    let Some(meta) = meta.as_object_mut() else {
        return false;
    };
    let cur = meta
        .get("isDefault")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if cur == value {
        return false;
    }
    meta.insert("isDefault".to_string(), json!(value));
    true
}

/// 设置默认版本：目标文件 isDefault=true，同 stem 的兄弟版本全部置 false（原子、互斥）。
/// 仅改动 isDefault 变化的文件，并为其补 updatedAt。返回受影响文件列表。
pub async fn set_default_version(r: &DefRef) -> PortalResult<serde_json::Value> {
    if r.domain.as_deref().unwrap_or("").trim() == "base" {
        return Err(PortalError::bad_request("base 公共模板无版本管理"));
    }
    let rel = resolve_rel(r)?;
    let target_file = rel.last().cloned().unwrap_or_default();
    let dir = abs_path(&rel[..rel.len() - 1]);
    let stem = def_file_stem(&target_file);

    let _guard = write_lock().lock().await;
    // 目标文件必须存在。
    if read_json::<serde_json::Value>(&abs_path(&rel))
        .await
        .is_err()
    {
        return Err(PortalError::not_found(format!(
            "定义文件不存在：{}",
            rel.join("/")
        )));
    }
    let now = json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let mut changed: Vec<String> = Vec::new();
    let mut dir_rd = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) => return Err(PortalError::Io(e)),
    };
    while let Some(f) = dir_rd.next_entry().await.map_err(PortalError::Io)? {
        let fname = f.file_name().to_string_lossy().to_string();
        if !f.file_type().await.map_err(PortalError::Io)?.is_file() || !is_safe_json_file(&fname) {
            continue;
        }
        // 仅同一逻辑定义（同 stem）的兄弟版本参与互斥。
        if def_file_stem(&fname) != stem {
            continue;
        }
        let want = fname == target_file;
        let mut doc = match read_json::<serde_json::Value>(&f.path()).await {
            Ok(d) => d,
            Err(_) => continue, // 损坏文件跳过，不阻断
        };
        if set_doc_default_flag(&mut doc, want) {
            if let Some(obj) = doc.as_object_mut() {
                obj.insert("updatedAt".to_string(), now.clone());
            }
            write_json_atomic(&f.path(), &doc, true).await?;
            changed.push(fname);
        }
    }
    Ok(json!({ "ok": true, "default": target_file, "changed": changed }))
}

/// 删除定义（base 域不可删）。
pub async fn delete_definition(r: &DefRef) -> PortalResult<serde_json::Value> {
    if r.domain.as_deref().unwrap_or("").trim() == "base" {
        return Err(PortalError::bad_request("base 公共模板不可删除"));
    }
    let rel = resolve_rel(r)?;
    let _guard = write_lock().lock().await;
    match tokio::fs::remove_file(abs_path(&rel)).await {
        Ok(()) => Ok(json!({ "ok": true })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(PortalError::not_found(format!(
            "定义文件不存在：{}/{}/{}/{}",
            r.domain.as_deref().unwrap_or(""),
            r.app_value().unwrap_or(""),
            r.module.as_deref().unwrap_or(""),
            r.file_value().unwrap_or("")
        ))),
        Err(e) => Err(PortalError::Io(e)),
    }
}
