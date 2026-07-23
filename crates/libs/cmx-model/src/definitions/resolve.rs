//! 业务编码 → 定义文件解析（DOC / DCT 共享的定义层逻辑）。
//!
//! 从 `definitions::store` 读设计期定义 JSON，把运行时的 `domain/app/module/<业务编码>`
//! 坐标解析成具体定义文件名（`*_doc_meta_v1.json` / `*_dct_meta_v1.json`）。纯定义层逻辑
//! （不碰任何 DB 物理表），被三方共享：
//! - `cmx-api` 的「业务编码定位」handler（`/definitions/config` 按 kind=DOC/DCT 反查）；
//! - `cmx-doc-api`（`/doc/*` 装载/回存前定位单据定义）；
//! - `cmx-dct-api`（`/dct/*` 定位字典定义）。
//!
//! 抽到 cmx-model 是为了避免 `cmx-api ⇄ cmx-doc/cmx-dct` 环：三方都已（直接或间接）依赖
//! cmx-model，故解析器放这里三方可共用。错误统一返回 `cmx_api_types::Error::business_error`
//! （与迁移前 `BizError::business(..).into()` 的 HTTP 语义完全一致：code=BusinessError/200）。

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;
use tokio::sync::RwLock;
use tracing::warn;

use cmx_api_types::{Error, Result};

use super::store::{self, DefRef};

/// DOC 定义文件解析结果缓存（键 `domain/app/module[/doc]` → file）。
static DOC_FILE_CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
/// DCT 定义文件解析结果缓存（键 `domain/app/module/dict` → file）。
static DICT_FILE_CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn doc_file_cache() -> &'static RwLock<HashMap<String, String>> {
    DOC_FILE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn dict_file_cache() -> &'static RwLock<HashMap<String, String>> {
    DICT_FILE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn not_found(msg: String) -> Error {
    Error::business_error(msg)
}

/// 判断一张字典表（`dictionaryTables[]` 元素）是否命中目标编码：
/// `dictMeta.dictCode` 或 `dictMeta.tableName` 任一等于 `target`。
pub fn dict_matches(t: &Value, target: &str) -> bool {
    let m = match t.get("dictMeta") {
        Some(m) => m,
        None => return false,
    };
    m.get("dictCode").and_then(|v| v.as_str()) == Some(target)
        || m.get("tableName").and_then(|v| v.as_str()) == Some(target)
}

/// 判断一份 DOC 定义是否命中目标编码：`moduleMeta.moduleCode` 等于 `target`。
pub fn doc_matches(doc: &Value, target: &str) -> bool {
    doc.get("moduleMeta")
        .and_then(|m| m.get("moduleCode"))
        .and_then(|v| v.as_str())
        == Some(target)
}

// ============================================================================
// 候选文件排序与脏状态检测（确定性选版本 + 跨副本一致）
// ============================================================================
//
// 背景：原实现从 `groups: HashMap` 直接 collect 候选，HashMap 迭代顺序不定导致：
// 1. **多副本部署**下不同节点可能选到不同文件（违反 AGENTS §五集群一致性）。
// 2. **进程重启**后可能选到不同文件（运行时缓存又掩盖了这一问题）。
// 这里统一改为 (isDefault 降序 → version 降序 → file 升序) 排序，让任意副本 / 任意
// 重启都能稳定收敛到同一份。

/// 按 (isDefault 降序, version 降序, file 升序) 排序候选文件名。
///
/// # Arguments
///
/// * `candidates` - 待排序的文件名列表（原地排序）。
/// * `entries` - `(stem, file, is_default, version)` 摘要列表，提供排序所需的元信息。
pub fn sort_candidates_by_default(
    candidates: &mut [String],
    entries: &[(String, String, bool, u64)],
) {
    candidates.sort_by(|a, b| {
        let meta = |f: &str| entries.iter().find(|(_, ef, _, _)| ef == f);
        let ad = meta(a).map(|(_, _, d, _)| *d).unwrap_or(false);
        let bd = meta(b).map(|(_, _, d, _)| *d).unwrap_or(false);
        bd.cmp(&ad) // isDefault=true 优先
            .then_with(|| {
                let av = meta(a).map(|(_, _, _, v)| *v).unwrap_or(0);
                let bv = meta(b).map(|(_, _, _, v)| *v).unwrap_or(0);
                bv.cmp(&av) // 同 isDefault 时 version 降序
            })
            .then_with(|| a.cmp(b)) // 仍相等时 file 名升序稳定
    });
}

/// 检测并 warn「同 stem 多 isDefault=true」脏状态。
///
/// 设计契约：同 stem 组内至多一个 `isDefault=true`（由 `set_default_version` 强制）。
/// 该状态多见于手工编辑 JSON / 跨环境 git pull / 绕过 set_default_version 直写。
/// 不阻断流程——`pick` 仍按 (isDefault 优先, version 最大) 收敛——但提醒运维清理。
pub fn warn_stem_multi_default(
    kind: &str,
    domain: &str,
    app: &str,
    module: &str,
    entries: &[(String, String, bool, u64)],
) {
    let mut by_stem: HashMap<&str, Vec<&str>> = HashMap::new();
    for (stem, file, is_default, _) in entries {
        if *is_default {
            by_stem
                .entry(stem.as_str())
                .or_default()
                .push(file.as_str());
        }
    }
    for (stem, files) in by_stem {
        if files.len() > 1 {
            warn!(
                "[{kind}] 同 stem 多 isDefault=true 脏状态: domain={domain}, app={app}, module={module}, \
                 stem={stem}, files={files:?} —— pick 将收敛到 isDefault 且 version 最大的那份，\
                 建议清理多余 isDefault=true 的文件"
            );
        }
    }
}

/// 解析 DOC 定义文件：`doc` 有值（URL query 中传入的 moduleCode）时按 `moduleMeta.moduleCode` 精确定位；缺失时盲选默认/最高版本。
pub async fn resolve_doc_file(
    domain: &str,
    app: &str,
    module: &str,
    doc: Option<&str>,
) -> Result<String> {
    // 缓存键：doc 有值时四段（精确定位），缺失时三段（盲选默认）。
    let cache_key = match doc {
        Some(d) if !d.is_empty() => format!("{domain}/{app}/{module}/{d}"),
        _ => format!("{domain}/{app}/{module}"),
    };
    if let Some(f) = doc_file_cache().read().await.get(&cache_key).cloned() {
        return Ok(f);
    }
    let items = store::list_definitions(Some("DOC"), Some(domain), Some(app), Some(module)).await?;
    // 提取 owned 摘要元组，避开对 items 的引用生命周期纠缠（同 DCT 写法）。
    // (stem, file, is_default, version)：stem 用于分组，其余用于选版本。
    let entries: Vec<(String, String, bool, u64)> = items
        .iter()
        .filter_map(|it| {
            let stem = it.get("stem").and_then(|v| v.as_str())?.to_string();
            let file = it.get("file").and_then(|v| v.as_str())?.to_string();
            let is_default = it
                .get("isDefault")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let version = it.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
            Some((stem, file, is_default, version))
        })
        .collect();
    if entries.is_empty() {
        return Err(not_found(format!(
            "未在 {domain}/{app}/{module} 下找到 DOC 定义文件"
        )));
    }
    // 按 stem 分组，每组选出代表（isDefault 优先，否则 version 最大）——同 DCT。
    let mut groups: HashMap<String, Vec<(String, bool, u64)>> = HashMap::new();
    for (stem, file, is_default, version) in &entries {
        groups
            .entry(stem.clone())
            .or_default()
            .push((file.clone(), *is_default, *version));
    }
    // C1：脏状态检测——同 stem 多 isDefault=true。
    warn_stem_multi_default("DOC", domain, app, module, &entries);
    let pick = |arr: &[(String, bool, u64)]| -> Option<String> {
        // 优先 isDefault=true 的；无则全员；组内取 version 最大者的 file。
        let any_default = arr.iter().any(|(_, d, _)| *d);
        arr.iter()
            .filter(|(_, d, _)| if any_default { *d } else { true })
            .max_by_key(|(_, _, v)| *v)
            .map(|(f, _, _)| f.clone())
    };
    // doc 有值：仿 DCT resolve_dict_file，逐候选文件读 moduleMeta.moduleCode 验证匹配（精确定位）。
    if let Some(module_code) = doc.filter(|d| !d.is_empty()) {
        // 收集候选文件（每组代表优先），按 (isDefault, version, file) 确定性排序。
        let mut candidates: Vec<String> = groups.values().filter_map(|arr| pick(arr)).collect();
        sort_candidates_by_default(&mut candidates, &entries);
        // 代表都没命中时，回退扫描该 stem 组其余版本（防 isDefault 版本恰好 moduleCode 不符）。
        // 同样按确定性顺序排序，跨副本一致。
        let mut fallback: Vec<String> = Vec::new();
        for (_, file, _, _) in &entries {
            if !candidates.contains(file) {
                fallback.push(file.clone());
            }
        }
        sort_candidates_by_default(&mut fallback, &entries);
        // 逐候选验证 moduleCode，收集所有命中的（同 moduleCode 多文件时按 isDefault/version 选最优）。
        let mut hits: Vec<(String, bool, u64)> = Vec::new();
        let entry_meta = |file: &str| -> (bool, u64) {
            entries
                .iter()
                .find(|(_, f, _, _)| f == file)
                .map(|(_, _, d, v)| (*d, *v))
                .unwrap_or((false, 0))
        };
        for f in candidates.iter().chain(fallback.iter()) {
            let doc_ref = DefRef {
                domain: Some(domain.to_string()),
                application: Some(app.to_string()),
                app: Some(app.to_string()),
                module: Some(module.to_string()),
                file: Some(f.clone()),
                id: None,
                kind: None,
            };
            let doc_json = match store::get_definition(&doc_ref).await {
                Ok(d) => d,
                Err(_) => continue,
            };
            if doc_matches(&doc_json, module_code) {
                let (is_default, version) = entry_meta(f);
                hits.push((f.clone(), is_default, version));
            }
        }
        // C2：跨 stem 同 moduleCode 重复定义——按确定性顺序选第一份（pick 内已收敛 isDefault/version），其余 warn。
        if hits.len() > 1 {
            let files: Vec<&str> = hits.iter().map(|(f, _, _)| f.as_str()).collect();
            warn!(
                "[DOC] 跨 stem 同 moduleCode 重复定义: domain={domain}, app={app}, module={module}, \
                 moduleCode={module_code}, files={files:?} —— 选用 pick 收敛结果，建议清理重复定义"
            );
        }
        if let Some(resolved) = pick(&hits) {
            doc_file_cache()
                .write()
                .await
                .insert(cache_key, resolved.clone());
            return Ok(resolved);
        }
        return Err(not_found(format!(
            "未在 {domain}/{app}/{module} 下找到 moduleCode={module_code} 的 DOC 定义文件"
        )));
    }
    // doc 缺失：盲选默认（向后兼容）。收集各组代表，再做一次全局选代表（跨 stem 取 isDefault 优先 / version 最大）。
    // DOC 一个 module 通常单 stem 单默认版本；多 stem 时按同一规则收敛到唯一结果。
    let mut candidates: Vec<(String, bool, u64)> = groups
        .values()
        .filter_map(|arr| {
            let f = pick(arr)?;
            let any_default = arr.iter().any(|(_, d, _)| *d);
            let top_version = arr
                .iter()
                .filter(|(_, d, _)| if any_default { *d } else { true })
                .map(|(_, _, v)| *v)
                .max()
                .unwrap_or(0);
            Some((f, any_default, top_version))
        })
        .collect();
    // 跨 stem 候选按 (isDefault, version, file) 确定性排序，让多副本/重启都收敛到同一份。
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1) // isDefault 降序
            .then_with(|| b.2.cmp(&a.2)) // version 降序
            .then_with(|| a.0.cmp(&b.0)) // file 升序
    });
    if candidates.is_empty() {
        return Err(not_found(format!(
            "未在 {domain}/{app}/{module} 下解析出可用的 DOC 默认定义"
        )));
    }
    let resolved = candidates
        .first()
        .map(|(f, _, _)| f.clone())
        .ok_or_else(|| {
            not_found(format!(
                "未在 {domain}/{app}/{module} 下解析出可用的 DOC 默认定义"
            ))
        })?;
    doc_file_cache()
        .write()
        .await
        .insert(cache_key, resolved.clone());
    Ok(resolved)
}

/// 解析 DCT 定义文件：逐候选文件读 `dictionaryTables[]` 找含目标 `dict`（dictCode/tableName）的那份。
pub async fn resolve_dict_file(
    domain: &str,
    app: &str,
    module: &str,
    dict: &str,
) -> Result<String> {
    let cache_key = format!("{domain}/{app}/{module}/{dict}");
    if let Some(f) = dict_file_cache().read().await.get(&cache_key).cloned() {
        return Ok(f);
    }
    let items = store::list_definitions(Some("DCT"), Some(domain), Some(app), Some(module)).await?;
    // 提取 owned 摘要元组，避开对 items 的引用生命周期纠缠。
    // (stem, file, is_default, version)：stem 用于分组，其余用于选版本。
    let entries: Vec<(String, String, bool, u64)> = items
        .iter()
        .filter_map(|it| {
            let stem = it.get("stem").and_then(|v| v.as_str())?.to_string();
            let file = it.get("file").and_then(|v| v.as_str())?.to_string();
            let is_default = it
                .get("isDefault")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let version = it.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
            Some((stem, file, is_default, version))
        })
        .collect();
    // 按 stem 分组，每组选出代表（isDefault 优先，否则 version 最大）。
    let mut groups: HashMap<String, Vec<(String, bool, u64)>> = HashMap::new();
    for (stem, file, is_default, version) in &entries {
        groups
            .entry(stem.clone())
            .or_default()
            .push((file.clone(), *is_default, *version));
    }
    // C1：脏状态检测——同 stem 多 isDefault=true。
    warn_stem_multi_default("DCT", domain, app, module, &entries);
    let pick = |arr: &[(String, bool, u64)]| -> Option<String> {
        // 优先 isDefault=true 的；无则全员；组内取 version 最大者的 file。
        let any_default = arr.iter().any(|(_, d, _)| *d);
        arr.iter()
            .filter(|(_, d, _)| if any_default { *d } else { true })
            .max_by_key(|(_, _, v)| *v)
            .map(|(f, _, _)| f.clone())
    };
    // 收集候选文件（每组代表优先），按 (isDefault, version, file) 确定性排序，
    // 让多副本/进程重启都收敛到同一份。
    let mut candidates: Vec<String> = Vec::new();
    for arr in groups.values() {
        if let Some(f) = pick(arr) {
            candidates.push(f);
        }
    }
    sort_candidates_by_default(&mut candidates, &entries);
    // 代表都没命中时，回退扫描该 stem 组其余版本（防 isDefault 版本恰好不含该 dict）。
    // 同样按确定性顺序排序，跨副本一致。
    let mut fallback: Vec<String> = Vec::new();
    for (_, file, _, _) in &entries {
        if !candidates.contains(file) {
            fallback.push(file.clone());
        }
    }
    sort_candidates_by_default(&mut fallback, &entries);
    // 收集所有命中文件：第一个（按确定性顺序）作为返回，>1 个时 warn 提醒脏数据。
    let mut hits: Vec<String> = Vec::new();
    for f in candidates.iter().chain(fallback.iter()) {
        let doc_ref = DefRef {
            domain: Some(domain.to_string()),
            application: Some(app.to_string()),
            app: Some(app.to_string()),
            module: Some(module.to_string()),
            file: Some(f.clone()),
            id: None,
            kind: None,
        };
        let doc = match store::get_definition(&doc_ref).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        let hit = doc
            .get("dictionaryTables")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|t| dict_matches(t, dict)))
            .unwrap_or(false);
        if hit {
            hits.push(f.clone());
        }
    }
    match hits.first() {
        Some(first) => {
            // C2：跨 stem 同 dictCode 重复定义——挑第一份（确定性收敛），其余 warn。
            if hits.len() > 1 {
                warn!(
                    "[DCT] 跨 stem 同 dictCode 重复定义: domain={domain}, app={app}, module={module}, \
                     dict={dict}, files={hits:?} —— 选用 {first}，建议清理重复定义"
                );
            }
            dict_file_cache().write().await.insert(cache_key, first.clone());
            Ok(first.clone())
        }
        None => Err(not_found(format!(
            "未在 {domain}/{app}/{module} 下找到含字典 {dict} 的 DCT 定义文件"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个 entries 摘要，模拟 resolve 时的 (stem, file, is_default, version)。
    fn entries(items: &[(&str, &str, bool, u64)]) -> Vec<(String, String, bool, u64)> {
        items
            .iter()
            .map(|(s, f, d, v)| (s.to_string(), f.to_string(), *d, *v))
            .collect()
    }

    #[test]
    fn sort_candidates_prefer_default() {
        // a/b 无 default；c 是 default；期望 c 排最前
        let ents = entries(&[
            ("a", "a_v1.json", false, 1),
            ("b", "b_v3.json", false, 3),
            ("c", "c_v1.json", true, 1),
        ]);
        let mut candidates = vec![
            "a_v1.json".to_string(),
            "b_v3.json".to_string(),
            "c_v1.json".to_string(),
        ];
        sort_candidates_by_default(&mut candidates, &ents);
        assert_eq!(
            candidates,
            vec![
                "c_v1.json".to_string(), // isDefault=true 优先
                "b_v3.json".to_string(), // version desc
                "a_v1.json".to_string(),
            ]
        );
    }

    #[test]
    fn sort_candidates_version_desc_within_same_default() {
        // 同 isDefault=false 时按 version 降序
        let ents = entries(&[
            ("a", "a_v1.json", false, 1),
            ("b", "b_v2.json", false, 2),
            ("c", "c_v3.json", false, 3),
        ]);
        let mut candidates = vec![
            "a_v1.json".to_string(),
            "b_v2.json".to_string(),
            "c_v3.json".to_string(),
        ];
        sort_candidates_by_default(&mut candidates, &ents);
        assert_eq!(
            candidates,
            vec![
                "c_v3.json".to_string(),
                "b_v2.json".to_string(),
                "a_v1.json".to_string(),
            ]
        );
    }

    #[test]
    fn sort_candidates_file_lex_as_final_tiebreaker() {
        // 同 isDefault + 同 version 时按 file 名升序稳定
        let ents = entries(&[
            ("a", "z_v1.json", false, 1),
            ("b", "a_v1.json", false, 1),
            ("c", "m_v1.json", false, 1),
        ]);
        let mut candidates = vec![
            "z_v1.json".to_string(),
            "a_v1.json".to_string(),
            "m_v1.json".to_string(),
        ];
        sort_candidates_by_default(&mut candidates, &ents);
        assert_eq!(
            candidates,
            vec![
                "a_v1.json".to_string(),
                "m_v1.json".to_string(),
                "z_v1.json".to_string(),
            ]
        );
    }

    #[test]
    fn sort_candidates_default_beats_higher_version() {
        // isDefault=true 即使 version 更低也排前面
        let ents = entries(&[
            ("a", "a_v10.json", false, 10),
            ("b", "b_v1.json", true, 1),
        ]);
        let mut candidates = vec![
            "a_v10.json".to_string(),
            "b_v1.json".to_string(),
        ];
        sort_candidates_by_default(&mut candidates, &ents);
        assert_eq!(
            candidates,
            vec!["b_v1.json".to_string(), "a_v10.json".to_string()]
        );
    }

    #[test]
    fn sort_candidates_empty_is_noop() {
        let ents = entries(&[]);
        let mut candidates: Vec<String> = vec![];
        sort_candidates_by_default(&mut candidates, &ents);
        assert!(candidates.is_empty());
    }
}
