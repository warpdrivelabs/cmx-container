//! 弹性组合 store（按 DAM + scenario 分层文件存储）。复刻 `lib/flexibleCombinationStore.js`。

use serde::Deserialize;
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json, write_json_atomic};
use crate::util::{is_safe_segment, write_lock};

/// DAM + scenario 引用。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FcRef {
    /// 业务域（如 fi / hr）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用标识。
    #[serde(default)]
    pub app: Option<String>,
    /// 模块标识。
    #[serde(default)]
    pub module: Option<String>,
    /// 场景标识（弹性组合的逻辑档案名，可带 `_v<N>` 版本后缀）。
    #[serde(default)]
    pub scenario: Option<String>,
}

/// 校验四段并返回相对 flexible-combination 根的路径段。
///
/// 四段（domain/app/module/scenario）均必填且须为安全段；scenario 末尾追加 `.json`。
///
/// # Arguments
///
/// * `r` - 弹性组合引用，含 domain/app/module/scenario 四段。
///
/// # Returns
///
/// 成功返回相对路径段数组（末段已附 `.json`）；任一段缺失或非法返回 `PortalError::BadRequest`。
fn resolve_rel(r: &FcRef) -> PortalResult<Vec<String>> {
    let segs = [
        ("domain", r.domain.as_deref()),
        ("app", r.app.as_deref()),
        ("module", r.module.as_deref()),
        ("scenario", r.scenario.as_deref()),
    ];
    let mut out = Vec::with_capacity(4);
    // 逐段校验：必填 + 安全段（防路径穿越）
    for (k, v) in segs {
        let v = v.unwrap_or("").trim();
        if v.is_empty() {
            return Err(PortalError::bad_request(format!("缺少必填参数 {k}")));
        }
        if !is_safe_segment(v) {
            return Err(PortalError::bad_request(format!(
                "参数 {k} 非法（仅允许字母、数字、_-）：\"{v}\""
            )));
        }
        out.push(v.to_string());
    }
    // scenario 末段补 .json 后缀
    let scenario = out
        .pop()
        .expect("invariant: 循环已 push 四段(domain/app/module/scenario),out 必非空");
    out.push(format!("{scenario}.json"));
    Ok(out)
}

/// 由相对路径段拼接 flexible-combination 根下的绝对路径（data/meta/flexible-combination/<rel>）。
fn abs_path(rel: &[String]) -> std::path::PathBuf {
    let mut p = data_path(["meta", "flexible-combination"]);
    for seg in rel {
        p.push(seg);
    }
    p
}

/// 剥去 scenario 末尾的 `_v<N>` 版本后缀，返回逻辑档案 stem（多版本聚合用）。
///
/// 例：`account_v2` → `account`；无版本后缀原样返回。
fn scenario_stem(scenario: &str) -> String {
    // 定位最后一个 _v，其后须全部为数字才视为版本后缀
    if let Some(idx) = scenario.rfind("_v") {
        let suffix = &scenario[idx + 2..];
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
            return scenario[..idx].to_string();
        }
    }
    scenario.to_string()
}

/// 读取弹性组合。
///
/// # Arguments
///
/// * `r` - 弹性组合引用，定位 `meta/flexible-combination/<...>` 下的目标文件。
///
/// # Returns
///
/// 成功返回文件解析后的 JSON 值；文件不存在时返回 `PortalError::NotFound`（含定位信息）。
pub async fn get_flexible_combination(r: &FcRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    match read_json::<serde_json::Value>(&abs_path(&rel)).await {
        Ok(v) => Ok(v),
        // 文件缺失：转语义化 NotFound 并带定位路径
        Err(PortalError::NotFound(_)) => Err(PortalError::not_found(format!(
            "弹性组合不存在：{}/{}/{}/{}",
            r.domain.as_deref().unwrap_or(""),
            r.app.as_deref().unwrap_or(""),
            r.module.as_deref().unwrap_or(""),
            r.scenario.as_deref().unwrap_or("")
        ))),
        Err(e) => Err(e),
    }
}

/// 扫描已保存档案，按 domain/app/module 逐级过滤，返回摘要列表。
///
/// 使用迭代式 DFS（栈）遍历三层目录，在第三层收集 *.json 文件并抽取摘要。
/// 每条档案含版本/标题/规则数/维度数等摘要字段，供前端列表展示。
///
/// # Arguments
///
/// * `domain` - 过滤的业务域，`None` 表示不过滤。
/// * `app` - 过滤的应用标识，`None` 表示不过滤。
/// * `module` - 过滤的模块标识，`None` 表示不过滤。
///
/// # Returns
///
/// 返回各档案的摘要列表，按 domain/app/module/scenario 排序。
pub async fn list_flexible_combinations(
    domain: Option<&str>,
    app: Option<&str>,
    module: Option<&str>,
) -> PortalResult<Vec<serde_json::Value>> {
    let root = data_path(["meta", "flexible-combination"]);
    // 归一过滤参数
    let wd = domain.unwrap_or("").trim().to_string();
    let wa = app.unwrap_or("").trim().to_string();
    let wm = module.unwrap_or("").trim().to_string();
    let mut out: Vec<serde_json::Value> = Vec::new();
    // 迭代式 DFS：栈元素为 (当前目录, 已累积的路径段)
    let mut stack: Vec<(std::path::PathBuf, Vec<String>)> = vec![(root, Vec::new())];
    while let Some((dir, parts)) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            // 目录缺失：跳过该子树
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(PortalError::Io(e)),
        };
        while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过隐藏文件
            if name.starts_with('.') {
                continue;
            }
            let ft = entry.file_type().await.map_err(PortalError::Io)?;
            let mut next = parts.clone();
            next.push(name.clone());
            if ft.is_dir() {
                // 按层级应用 domain/app/module 过滤，提前剪枝
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
                // 第三层 *.json：抽取档案摘要
                let scenario = name.trim_end_matches(".json").to_string();
                let (d, a, m) = (parts[0].clone(), parts[1].clone(), parts[2].clone());
                match read_json::<serde_json::Value>(&entry.path()).await {
                    Ok(doc) => {
                        // 摘要锚点维度：顶层 anchorDimensions（旧字段，多数档案为空）为空时
                        // 回退取各规则 anchor.dimensions 的并集（真源），避免列表误显示「无锚点」
                        let anchor_dims = {
                            let top = doc.get("anchorDimensions").filter(|v| v.is_array()).cloned().unwrap_or(json!([]));
                            if top.as_array().is_some_and(|a| !a.is_empty()) {
                                top
                            } else {
                                let mut dims: Vec<String> = Vec::new();
                                if let Some(rules) = doc.get("rules").and_then(|v| v.as_array()) {
                                    for r in rules {
                                        if let Some(arr) = r.get("anchor").and_then(|a| a.get("dimensions")).and_then(|v| v.as_array()) {
                                            for d in arr.iter().filter_map(|x| x.as_str()) {
                                                if !dims.iter().any(|x| x == d) {
                                                    dims.push(d.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                                json!(dims)
                            }
                        };
                        out.push(json!({
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
                        "anchorDimensions": anchor_dims,
                        "status": doc.get("status").and_then(|v| v.as_str()).unwrap_or("draft"),
                        "tags": doc.get("tags").filter(|v| v.is_array()).cloned().unwrap_or(json!([])),
                        "ruleCount": doc.get("rules").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                        "dimensionCount": doc.get("dimensions").and_then(|v| v.as_object()).map(|o| o.len()).unwrap_or(0),
                        "updatedAt": doc.get("updatedAt").and_then(|v| v.as_str()).unwrap_or(""),
                    }));
                    }
                    // 损坏文件以 error 记录，不阻断扫描
                    Err(e) => out.push(json!({ "domain": d, "app": a, "module": m, "scenario": scenario, "error": e.to_string() })),
                }
            }
        }
    }
    // 按 domain/app/module/scenario 字典序排序，保证列表稳定
    out.sort_by(|a, b| {
        let ka = format!(
            "{}/{}/{}/{}",
            a["domain"].as_str().unwrap_or(""),
            a["app"].as_str().unwrap_or(""),
            a["module"].as_str().unwrap_or(""),
            a["scenario"].as_str().unwrap_or("")
        );
        let kb = format!(
            "{}/{}/{}/{}",
            b["domain"].as_str().unwrap_or(""),
            b["app"].as_str().unwrap_or(""),
            b["module"].as_str().unwrap_or(""),
            b["scenario"].as_str().unwrap_or("")
        );
        ka.cmp(&kb)
    });
    Ok(out)
}

/// 保存档案（规范化外壳 + updatedAt，原子写）。
///
/// 校验文档必含 dimensions（对象）和 rules（数组），然后规范化外壳字段（版本/标题/状态等），
/// 可选字段 columnModel / docRef 仅在存在时才写入。
///
/// # Arguments
///
/// * `r` - 弹性组合引用，定位落盘路径。
/// * `doc` - 待保存的文档 JSON，必须是对象且含 dimensions/rules。
///
/// # Returns
///
/// 成功返回规范化后的文档（含自动补的 `updatedAt`）；校验失败返回 `PortalError::BadRequest`。
pub async fn save_flexible_combination(
    r: &FcRef,
    doc: &serde_json::Value,
) -> PortalResult<serde_json::Value> {
    if !doc.is_object() {
        return Err(PortalError::bad_request("请求体必须是对象"));
    }
    // dimensions 必须为对象
    if !doc
        .get("dimensions")
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        return Err(PortalError::bad_request("缺少 dimensions"));
    }
    // rules 必须为数组
    if !doc.get("rules").map(|v| v.is_array()).unwrap_or(false) {
        return Err(PortalError::bad_request("缺少 rules 数组"));
    }
    let rel = resolve_rel(r)?;
    // 标题优先级：title > name > caption
    let title = doc
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| doc.get("name").and_then(|v| v.as_str()))
        .or_else(|| doc.get("caption").and_then(|v| v.as_str()))
        .unwrap_or("");
    // 构造规范化外壳：固定字段集 + dimensions/rules 透传 + updatedAt
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
        merged
            .as_object_mut()
            .expect("invariant: merged 由 json!({{...}}) 构造,必为对象")
            .insert("columnModel".to_string(), cm.clone());
    }
    if let Some(dr) = doc.get("docRef").filter(|v| v.is_object()) {
        merged
            .as_object_mut()
            .expect("invariant: merged 由 json!({{...}}) 构造,必为对象")
            .insert("docRef".to_string(), dr.clone());
    }
    // 全局写锁，保证原子写不被并发覆盖
    let _guard = write_lock().lock().await;
    write_json_atomic(&abs_path(&rel), &merged, true).await?;
    Ok(merged)
}

/// 设置默认版本：目标档案 isDefault=true，同 stem 的兄弟版本全部置 false（原子、互斥）。
///
/// 仅改动 isDefault 变化的文件并补 updatedAt。返回受影响 scenario 列表。
///
/// # Arguments
///
/// * `r` - 目标弹性组合引用。
///
/// # Returns
///
/// 返回 `{ ok, default, changed }`，changed 为实际改动了 isDefault 的 scenario 列表。
pub async fn set_default_version(r: &FcRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    let target_file = rel.last().cloned().unwrap_or_default();
    let dir = abs_path(&rel[..rel.len() - 1]);
    let target_scenario = r.scenario.as_deref().unwrap_or("").trim().to_string();
    // stem 用于识别同一逻辑档案的所有版本（含 _vN 后缀）
    let stem = scenario_stem(&target_scenario);

    let _guard = write_lock().lock().await;
    // 目标文件必须存在。
    if read_json::<serde_json::Value>(&abs_path(&rel))
        .await
        .is_err()
    {
        return Err(PortalError::not_found(format!(
            "弹性组合不存在：{}",
            rel.join("/")
        )));
    }
    let now = json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let mut changed: Vec<String> = Vec::new();
    // 扫描同目录下同 stem 的所有版本，做互斥置位
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
        // 目标档案置 true，其余置 false
        let want = fname == target_file;
        let mut doc = match read_json::<serde_json::Value>(&f.path()).await {
            Ok(d) => d,
            Err(_) => continue, // 损坏文件跳过，不阻断
        };
        // 仅当 isDefault 变化才写入，并补 updatedAt
        let cur = doc
            .get("isDefault")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
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
///
/// # Arguments
///
/// * `r` - 待删除弹性组合引用。
///
/// # Returns
///
/// 成功返回 `{ ok: true }`；文件不存在返回 `PortalError::NotFound`。
pub async fn delete_flexible_combination(r: &FcRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    let _guard = write_lock().lock().await;
    match tokio::fs::remove_file(abs_path(&rel)).await {
        Ok(()) => Ok(json!({ "ok": true })),
        // 文件缺失：转语义化 NotFound 并带定位路径
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(PortalError::not_found(format!(
            "弹性组合不存在：{}/{}/{}/{}",
            r.domain.as_deref().unwrap_or(""),
            r.app.as_deref().unwrap_or(""),
            r.module.as_deref().unwrap_or(""),
            r.scenario.as_deref().unwrap_or("")
        ))),
        Err(e) => Err(PortalError::Io(e)),
    }
}
