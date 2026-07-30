//! defs —— 三元定义（DCT/DOC/FLC）统一注册与 DRN 解析服务。
//!
//! 详见 docs/三元定义统一与跨DAM引用架构方案.html §6。把散在 definitions / flexible-combination
//! 两个 store 的定义，按 DRN 统一解析、构建依赖图、按需 overlay 编译。领域无关地建立在既有 store 之上。
//!
//! DRN → 落盘映射：
//!   DCT/DOC → meta/definitions/<domain>/<app>/<module>/<name>[_v<n>].json  (definitions::store)
//!   FLC     → meta/flexible-combination/<domain>/<app>/<module>/<name>.json (flexible_combination::store)
//!   BASE    → meta/definitions/base/<name>.json

use serde_json::{Value, json};

use crate::definitions::store::{DefRef, get_definition, list_definitions};
use crate::error::{PortalError, PortalResult};
use crate::flexible_combination::drn::{AbsDrn, FromDam, normalize_drn};
use crate::flexible_combination::store::{FcRef, get_flexible_combination};

/// 解析一个 DRN 字符串（引用方 DAM 由 `from` 提供）→ 定义全文 JSON。
///
/// # Arguments
///
/// * `drn` - 待解析的 DRN 字符串。
/// * `from` - 引用方 DAM，用于补全相对引用。
///
/// # Errors
///
/// DRN 归一失败（格式非法/缺 kind/无法补全 DAM）或目标定义加载失败时返回 `PortalError`。
pub async fn resolve(drn: &str, from: &FromDam) -> PortalResult<Value> {
    let abs = normalize_drn(drn, from, None, None).map_err(PortalError::bad_request)?;
    load_abs(&abs).await
}

/// 按绝对 DRN 加载定义全文。
///
/// 按 kind 路由到不同 store：FLC → flexible-combination store，BASE/DCT/DOC → definitions store。
pub async fn load_abs(abs: &AbsDrn) -> PortalResult<Value> {
    match abs.kind.as_str() {
        // FLC：弹性组合档案
        "FLC" => {
            let r = FcRef {
                domain: Some(abs.domain.clone()),
                app: Some(abs.app.clone()),
                module: Some(abs.module.clone()),
                scenario: Some(abs.name.clone()),
            };
            get_flexible_combination(&r).await
        }
        // BASE：公共字段集模板（落 meta/definitions/base/）
        "BASE" => {
            let r = DefRef {
                domain: Some("base".into()),
                application: None,
                app: None,
                module: None,
                file: Some(with_json(&abs.name, abs.version)),
                id: None,
                kind: None,
            };
            get_definition(&r).await
        }
        // DCT / DOC：数据字典 / 业务单据
        _ => {
            let r = DefRef {
                domain: Some(abs.domain.clone()),
                application: Some(abs.app.clone()),
                app: Some(abs.app.clone()),
                module: Some(abs.module.clone()),
                file: Some(with_json(&abs.name, abs.version)),
                id: None,
                kind: None,
            };
            get_definition(&r).await
        }
    }
}

/// 由 name + 可选版本拼装落盘文件名（带 `.json` 后缀）。
///
/// 有版本 → `<name>_v<N>.json`；无版本且 name 已含 `.json` → 原样；否则追加 `.json`。
fn with_json(name: &str, version: Option<u64>) -> String {
    match version {
        Some(v) => format!("{name}_v{v}.json"),
        None => {
            if name.ends_with(".json") {
                name.to_string()
            } else {
                format!("{name}.json")
            }
        }
    }
}

/// 拆文件名 stem 的 `_v<N>` 版本后缀 → (stem, Some(N))；无后缀 → (name, None)。
fn split_name_version(stem: &str) -> (&str, Option<u64>) {
    // 定位最后一个 _v，其后须全部为数字才视为版本后缀
    if let Some(idx) = stem.rfind("_v") {
        let ver = &stem[idx + 2..];
        if !ver.is_empty()
            && ver.bytes().all(|b| b.is_ascii_digit())
            && let Ok(n) = ver.parse::<u64>()
        {
            return (&stem[..idx], Some(n));
        }
    }
    (stem, None)
}

/// 列出可引用定义（按 kind/DAM 过滤，供编辑器 DRN 选择器）。复用 definitions 列表 + 追加 FLC 列表。
///
/// DCT/DOC/BASE 来自 definitions store，FLC 来自 flexible-combination store。
/// 返回的 FLC 项会被补上 `kind:"FLC"` 标记，与 definitions 列表项对齐。
///
/// # Arguments
///
/// * `kind` - 过滤的资源类型（DCT/DOC/BASE/FLC），`None` 表示列出全部。
/// * `domain` / `app` / `module` - DAM 过滤条件，`None` 表示不过滤。
///
/// # Returns
///
/// 返回各定义/档案的摘要列表（已统一带 kind 字段）。
pub async fn list(
    kind: Option<&str>,
    domain: Option<&str>,
    app: Option<&str>,
    module: Option<&str>,
) -> PortalResult<Vec<Value>> {
    let mut items = Vec::new();
    let want = kind.unwrap_or("").to_uppercase();

    // DCT/DOC/RPT/BASE 来自 definitions
    if want.is_empty() || want == "DCT" || want == "DOC" || want == "RPT" || want == "BASE" {
        let defs = list_definitions(kind, domain, app, module).await?;
        items.extend(defs);
    }
    // FLC 来自 flexible-combination，补 kind 标记后追加
    if want.is_empty() || want == "FLC" {
        let fcs =
            crate::flexible_combination::store::list_flexible_combinations(domain, app, module)
                .await?;
        for mut it in fcs {
            if let Some(obj) = it.as_object_mut() {
                obj.insert("kind".to_string(), json!("FLC"));
            }
            items.push(it);
        }
    }
    Ok(items)
}

/// 一个定义直接依赖的 DRN 列表（out 边）：imports[].drn + docRef + 字段 refDict。
///
/// 每项形如 `{ ref, drn(归一后，可能 null), resolved, via }`：
/// - `via` 标注依赖来源（"imports" / "docRef" / "refDict"）。
/// - `resolved` 表示该引用能否归一为绝对 DRN。
///
/// # Arguments
///
/// * `def` - 定义文档 JSON。
/// * `from` - 引用方 DAM，用于归一相对引用。
///
/// # Returns
///
/// 返回直接依赖列表（含原始引用与归一结果），归一失败的项 resolved 为 false 但不剔除。
pub fn dependencies_of(def: &Value, from: &FromDam) -> Vec<Value> {
    // 先收集 (raw, default_kind, via) 三元组
    let mut raw: Vec<(String, Option<&'static str>, &'static str)> = Vec::new();

    // 依赖来源一：imports[].drn（别名/DRN 原样收集）
    if let Some(imports) = def.get("imports").and_then(|v| v.as_array()) {
        for imp in imports {
            if let Some(d) = imp.get("drn").and_then(|v| v.as_str()) {
                raw.push((d.to_string(), None, "imports"));
            }
        }
    }
    // 依赖来源二：docRef（构造绝对 DRN，默认 kind=DOC）
    if let Some(dr) = def.get("docRef").filter(|v| v.is_object())
        && let Some(file) = dr.get("file").and_then(|v| v.as_str())
    {
        let (name, ver) = split_name_version(file.trim_end_matches(".json"));
        let domain = dr.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let app = dr
            .get("app")
            .or_else(|| dr.get("application"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let module = dr.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let ver_suffix = ver.map(|v| format!("@{v}")).unwrap_or_default();
        raw.push((
            format!("drn:{domain}/{app}/{module}/DOC/{name}{ver_suffix}"),
            Some("DOC"),
            "docRef",
        ));
    }
    // 依赖来源三：refDict（voucherTables 字段引用 + dimensions 字典引用，默认 kind=DCT）
    let mut dicts = std::collections::BTreeSet::new();
    if let Some(tables) = def.get("voucherTables").and_then(|v| v.as_array()) {
        for t in tables {
            if let Some(fields) = t.get("fields").and_then(|v| v.as_array()) {
                for f in fields {
                    if let Some(rd) = f.get("refDict").and_then(|v| v.as_str()) {
                        dicts.insert(rd.to_string());
                    }
                }
            }
        }
    }
    if let Some(dims) = def.get("dimensions").and_then(|v| v.as_object()) {
        for (_, dim) in dims {
            if let Some(id) = dim
                .get("dict")
                .and_then(|d| d.get("dictId"))
                .and_then(|v| v.as_str())
            {
                dicts.insert(id.to_string());
            }
        }
    }
    for d in dicts {
        raw.push((d, Some("DCT"), "refDict"));
    }

    // 统一归一为绝对 DRN，输出 { ref, drn, resolved, via }
    raw.into_iter()
        .map(|(r, kind, via)| {
            let norm = normalize_drn(&r, from, kind, def.get("imports"))
                .ok()
                .map(|a| crate::flexible_combination::drn::format_drn(&a));
            json!({
                "ref": r,
                "drn": norm,
                "resolved": norm.is_some(),
                "via": via,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from() -> FromDam {
        FromDam {
            domain: Some("fi".into()),
            app: Some("cmxfico".into()),
            module: Some("gl".into()),
        }
    }

    #[test]
    fn with_json_variants() {
        assert_eq!(with_json("x", None), "x.json");
        assert_eq!(with_json("x", Some(2)), "x_v2.json");
        assert_eq!(with_json("x.json", None), "x.json");
    }

    #[test]
    fn deps_from_docref_and_refdict() {
        let def = json!({
            "docRef": {"domain":"fi","app":"cmxfico","module":"gl","file":"gl_md_doc_meta_v1.json"},
            "dimensions": {"cc": {"dict": {"dictId":"cost_center"}}}
        });
        let deps = dependencies_of(&def, &from());
        let vias: Vec<_> = deps.iter().filter_map(|d| d["via"].as_str()).collect();
        assert!(vias.contains(&"docRef"));
        assert!(vias.contains(&"refDict"));
        // docRef 归一到绝对 DRN（_v1 后缀 → @1）
        let doc = deps.iter().find(|d| d["via"] == json!("docRef")).unwrap();
        assert_eq!(doc["drn"], json!("drn:fi/cmxfico/gl/DOC/gl_md_doc_meta@1"));
        assert_eq!(doc["resolved"], json!(true));
    }

    #[test]
    fn deps_from_imports() {
        let def = json!({
            "imports": [{"alias":"cc","drn":"drn:fi/shared-md/masterdata/DCT/cost_center"}],
            "voucherTables": [{"fields":[{"id":"x","refDict":"@cc"}]}]
        });
        let deps = dependencies_of(&def, &from());
        let imp = deps.iter().find(|d| d["via"] == json!("imports")).unwrap();
        assert_eq!(
            imp["drn"],
            json!("drn:fi/shared-md/masterdata/DCT/cost_center")
        );
    }
}
