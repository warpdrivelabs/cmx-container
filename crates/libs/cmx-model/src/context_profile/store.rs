//! 上下文档案 store（按 DAM + scenario 分层文件存储）。复刻 `lib/contextProfileStore.js`。

use serde::Deserialize;
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json, write_json_atomic};
use crate::util::{is_safe_segment, write_lock};

/// DAM + scenario 引用。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CpRef {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub scenario: Option<String>,
}

/// 校验四段并返回相对 context-profile 根的路径段。
fn resolve_rel(r: &CpRef) -> PortalResult<Vec<String>> {
    let segs = [
        ("domain", r.domain.as_deref()),
        ("app", r.app.as_deref()),
        ("module", r.module.as_deref()),
        ("scenario", r.scenario.as_deref()),
    ];
    let mut out = Vec::with_capacity(4);
    for (k, v) in segs {
        let v = v.unwrap_or("").trim();
        if v.is_empty() {
            return Err(PortalError::bad_request(format!("缺少必填参数 {k}")));
        }
        if !is_safe_segment(v) {
            return Err(PortalError::bad_request(format!("参数 {k} 非法（仅允许字母、数字、_-）：\"{v}\"")));
        }
        out.push(v.to_string());
    }
    let scenario = out.pop().unwrap();
    out.push(format!("{scenario}.json"));
    Ok(out)
}

fn abs_path(rel: &[String]) -> std::path::PathBuf {
    let mut p = data_path(["meta", "context-profile"]);
    for seg in rel {
        p.push(seg);
    }
    p
}

/// 剥去 scenario 末尾的 `_v<N>` 版本后缀，返回逻辑档案 stem（多版本聚合用）。
/// 例：`account_v2` → `account`；无版本后缀原样返回。
fn scenario_stem(scenario: &str) -> String {
    if let Some(idx) = scenario.rfind("_v") {
        let suffix = &scenario[idx + 2..];
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
            return scenario[..idx].to_string();
        }
    }
    scenario.to_string()
}

/// 读取上下文档案（含 BC：回退旧 subdivision 目录）。
pub async fn get_context_profile(r: &CpRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    match read_json::<serde_json::Value>(&abs_path(&rel)).await {
        Ok(v) => Ok(v),
        Err(PortalError::NotFound(_)) => {
            // BC：旧 meta/subdivision/
            let mut legacy = data_path(["meta", "subdivision"]);
            for seg in &rel {
                legacy.push(seg);
            }
            match read_json::<serde_json::Value>(&legacy).await {
                Ok(v) => Ok(v),
                Err(_) => Err(PortalError::not_found(format!(
                    "上下文档案不存在：{}/{}/{}/{}",
                    r.domain.as_deref().unwrap_or(""),
                    r.app.as_deref().unwrap_or(""),
                    r.module.as_deref().unwrap_or(""),
                    r.scenario.as_deref().unwrap_or("")
                ))),
            }
        }
        Err(e) => Err(e),
    }
}

/// 扫描已保存档案，按 domain/app/module 逐级过滤，返回摘要列表。
pub async fn list_context_profiles(domain: Option<&str>, app: Option<&str>, module: Option<&str>) -> PortalResult<Vec<serde_json::Value>> {
    let root = data_path(["meta", "context-profile"]);
    let wd = domain.unwrap_or("").trim().to_string();
    let wa = app.unwrap_or("").trim().to_string();
    let wm = module.unwrap_or("").trim().to_string();
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, Vec<String>)> = vec![(root, Vec::new())];
    while let Some((dir, parts)) = stack.pop() {
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
                if next.len() == 1 && !wd.is_empty() && wd != name {
                    continue;
                }
                if next.len() == 2 && !wa.is_empty() && wa != name {
                    continue;
                }
                if next.len() == 3 && !wm.is_empty() && wm != name {
                    continue;
                }
                stack.push((entry.path(), next));
            } else if ft.is_file() && name.ends_with(".json") && parts.len() == 3 {
                let scenario = name.trim_end_matches(".json").to_string();
                let (d, a, m) = (parts[0].clone(), parts[1].clone(), parts[2].clone());
                match read_json::<serde_json::Value>(&entry.path()).await {
                    Ok(doc) => out.push(json!({
                        "domain": d, "app": a, "module": m, "scenario": scenario,
                        "version": doc.get("version").cloned().unwrap_or(json!(1)),
                        "versionName": doc.get("versionName").and_then(|v| v.as_str()).unwrap_or(""),
                        "isDefault": doc.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false),
                        "stem": scenario_stem(&scenario),
                        "title": doc.get("title").and_then(|v| v.as_str())
                            .or_else(|| doc.get("name").and_then(|v| v.as_str()))
                            .or_else(|| doc.get("caption").and_then(|v| v.as_str()))
                            .unwrap_or(&scenario),
                        "description": doc.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                        "anchorDimensions": doc.get("anchorDimensions").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
                        "status": doc.get("status").and_then(|v| v.as_str()).unwrap_or("draft"),
                        "tags": doc.get("tags").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
                        "ruleCount": doc.get("rules").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                        "dimensionCount": doc.get("dimensions").and_then(|v| v.as_object()).map(|o| o.len()).unwrap_or(0),
                        "updatedAt": doc.get("updatedAt").and_then(|v| v.as_str()).unwrap_or(""),
                    })),
                    Err(e) => out.push(json!({ "domain": d, "app": a, "module": m, "scenario": scenario, "error": e.to_string() })),
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let ka = format!("{}/{}/{}/{}", a["domain"].as_str().unwrap_or(""), a["app"].as_str().unwrap_or(""), a["module"].as_str().unwrap_or(""), a["scenario"].as_str().unwrap_or(""));
        let kb = format!("{}/{}/{}/{}", b["domain"].as_str().unwrap_or(""), b["app"].as_str().unwrap_or(""), b["module"].as_str().unwrap_or(""), b["scenario"].as_str().unwrap_or(""));
        ka.cmp(&kb)
    });
    Ok(out)
}

/// 保存档案（规范化外壳 + updatedAt，原子写）。
pub async fn save_context_profile(r: &CpRef, doc: &serde_json::Value) -> PortalResult<serde_json::Value> {
    if !doc.is_object() {
        return Err(PortalError::bad_request("请求体必须是对象"));
    }
    if !doc.get("dimensions").map(|v| v.is_object()).unwrap_or(false) {
        return Err(PortalError::bad_request("缺少 dimensions"));
    }
    if !doc.get("rules").map(|v| v.is_array()).unwrap_or(false) {
        return Err(PortalError::bad_request("缺少 rules 数组"));
    }
    let rel = resolve_rel(r)?;
    let title = doc.get("title").and_then(|v| v.as_str())
        .or_else(|| doc.get("name").and_then(|v| v.as_str()))
        .or_else(|| doc.get("caption").and_then(|v| v.as_str()))
        .unwrap_or("");
    let mut merged = json!({
        "version": doc.get("version").cloned().unwrap_or(json!(1)),
        "versionName": doc.get("versionName").and_then(|v| v.as_str()).unwrap_or(""),
        "isDefault": doc.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false),
        "scenario": r.scenario.as_deref().unwrap_or(""),
        "domain": r.domain.as_deref().unwrap_or(""),
        "app": r.app.as_deref().unwrap_or(""),
        "module": r.module.as_deref().unwrap_or(""),
        "title": title,
        "anchorDimensions": doc.get("anchorDimensions").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
        "description": doc.get("description").and_then(|v| v.as_str()).unwrap_or(""),
        "status": doc.get("status").and_then(|v| v.as_str()).unwrap_or("draft"),
        "tags": doc.get("tags").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
        "dimensions": doc.get("dimensions").cloned().unwrap_or(json!({})),
        "rules": doc.get("rules").cloned().unwrap_or(json!([])),
        "updatedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });
    // 可选字段 columnModel / docRef（存在才带）
    if let Some(cm) = doc.get("columnModel").filter(|v| v.is_object()) {
        merged.as_object_mut().unwrap().insert("columnModel".to_string(), cm.clone());
    }
    if let Some(dr) = doc.get("docRef").filter(|v| v.is_object()) {
        merged.as_object_mut().unwrap().insert("docRef".to_string(), dr.clone());
    }
    let _guard = write_lock().lock().await;
    write_json_atomic(&abs_path(&rel), &merged, true).await?;
    Ok(merged)
}

/// 设置默认版本：目标档案 isDefault=true，同 stem 的兄弟版本全部置 false（原子、互斥）。
/// 仅改动 isDefault 变化的文件并补 updatedAt。返回受影响 scenario 列表。
pub async fn set_default_version(r: &CpRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    let target_file = rel.last().cloned().unwrap_or_default();
    let dir = abs_path(&rel[..rel.len() - 1]);
    let target_scenario = r.scenario.as_deref().unwrap_or("").trim().to_string();
    let stem = scenario_stem(&target_scenario);

    let _guard = write_lock().lock().await;
    // 目标文件必须存在。
    if read_json::<serde_json::Value>(&abs_path(&rel)).await.is_err() {
        return Err(PortalError::not_found(format!("上下文档案不存在：{}", rel.join("/"))));
    }
    let now = json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let mut changed: Vec<String> = Vec::new();
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) => return Err(PortalError::Io(e)),
    };
    while let Some(f) = rd.next_entry().await.map_err(PortalError::Io)? {
        let fname = f.file_name().to_string_lossy().to_string();
        if !f.file_type().await.map_err(PortalError::Io)?.is_file() || !fname.ends_with(".json") {
            continue;
        }
        let scen = fname.trim_end_matches(".json").to_string();
        // 仅同一逻辑档案（同 stem）的兄弟版本参与互斥。
        if scenario_stem(&scen) != stem {
            continue;
        }
        let want = fname == target_file;
        let mut doc = match read_json::<serde_json::Value>(&f.path()).await {
            Ok(d) => d,
            Err(_) => continue, // 损坏文件跳过，不阻断
        };
        let cur = doc.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false);
        if cur != want {
            if let Some(obj) = doc.as_object_mut() {
                obj.insert("isDefault".to_string(), json!(want));
                obj.insert("updatedAt".to_string(), now.clone());
            }
            write_json_atomic(&f.path(), &doc, true).await?;
            changed.push(scen);
        }
    }
    Ok(json!({ "ok": true, "default": target_scenario, "changed": changed }))
}

/// 删除档案文件。
pub async fn delete_context_profile(r: &CpRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    let _guard = write_lock().lock().await;
    match tokio::fs::remove_file(abs_path(&rel)).await {
        Ok(()) => Ok(json!({ "ok": true })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(PortalError::not_found(format!(
            "上下文档案不存在：{}/{}/{}/{}",
            r.domain.as_deref().unwrap_or(""),
            r.app.as_deref().unwrap_or(""),
            r.module.as_deref().unwrap_or(""),
            r.scenario.as_deref().unwrap_or("")
        ))),
        Err(e) => Err(PortalError::Io(e)),
    }
}