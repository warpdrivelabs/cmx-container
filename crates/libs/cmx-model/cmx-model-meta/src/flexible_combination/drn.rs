//! drn —— Definition Resource Name（跨 DAM 引用标识）解析器，JS `drn.js` 的 Rust 对端。
//!
//!   drn:<domain>/<app>/<module>/<kind>/<name>[@<version>][#<table>[.<field>]]
//!
//! 裸 code / 两段简写 / @别名 见 JS 对端文档。本模块提供解析、归一（补全继承段 + 展开别名）、
//! 格式化、落盘路径、可见性判定。领域无关、纯函数、可单测。

use serde_json::Value;

pub const DRN_KINDS: [&str; 4] = ["DCT", "DOC", "FLC", "BASE"];
pub const DRN_VISIBILITY: [&str; 4] = ["private", "app", "domain", "public"];

/// 结构化 DRN（各段缺省为 None；别名单独标记）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drn {
    /// 是否为别名形式（`@cc`）。
    pub is_alias: bool,
    /// 别名字符串（仅 is_alias 为真时有值）。
    pub alias: Option<String>,
    /// 业务域。
    pub domain: Option<String>,
    /// 应用标识。
    pub app: Option<String>,
    /// 模块标识。
    pub module: Option<String>,
    /// 资源类型（DCT/DOC/FLC/BASE）。
    pub kind: Option<String>,
    /// 资源名称。
    pub name: Option<String>,
    /// 版本号。
    pub version: Option<u64>,
    /// 深链表名。
    pub table: Option<String>,
    /// 深链字段名。
    pub field: Option<String>,
}

/// 判定一个 DRN 段是否合法：非空、长度 ≤64、仅含字母数字与 `-_`。
fn is_seg(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 解析 DRN 字符串（不补全、不展开别名）。
///
/// 支持四种形式：别名（`@cc[#t.f]`）、绝对（`drn:d/a/m/k/n[@v][#t.f]`）、
/// 两段简写（`KIND/name`）、裸 name。解析结果各段按需填充。
///
/// # Arguments
///
/// * `input` - 待解析的 DRN 字符串。
///
/// # Returns
///
/// 成功返回结构化 `Drn`；格式非法（空串、段非法、段数不支持、版本非正整数等）返回 `Err(message)`。
pub fn parse_drn(input: &str) -> Result<Drn, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("DRN 不能为空".into());
    }
    let mut out = Drn::default();

    // 别名 @cc 或 @cc#table[.field]
    if let Some(rest) = s.strip_prefix('@') {
        let mut alias_part = rest;
        // 别名后可带深链片段
        if let Some(hash) = rest.find('#') {
            let frag = &rest[hash + 1..];
            alias_part = &rest[..hash];
            parse_fragment(frag, &mut out, input)?;
        }
        if !is_seg(alias_part) {
            return Err(format!("DRN 别名非法：{input}"));
        }
        out.is_alias = true;
        out.alias = Some(alias_part.to_string());
        return Ok(out);
    }

    let has_prefix = s.starts_with("drn:");
    let mut body = if has_prefix { &s[4..] } else { s };

    // 深链 #table[.field]
    if let Some(hash) = body.find('#') {
        let frag = &body[hash + 1..];
        body = &body[..hash];
        parse_fragment(frag, &mut out, input)?;
    }
    // 版本 @version
    let mut core = body;
    if let Some(at) = body.find('@') {
        let ver = &body[at + 1..];
        core = &body[..at];
        // 版本须为正整数
        let v: u64 = ver
            .parse()
            .map_err(|_| format!("DRN 版本必须是正整数：{input}"))?;
        out.version = Some(v);
    }

    // 拆分核心段并逐段校验合法性
    let segs: Vec<&str> = core.split('/').collect();
    for seg in &segs {
        if !is_seg(seg) {
            return Err(format!("DRN 段非法「{seg}」：{input}"));
        }
    }

    if has_prefix {
        // 绝对 DRN 须恰好 5 段
        if segs.len() != 5 {
            return Err(format!("绝对 DRN 需 5 段：{input}"));
        }
        assign5(&mut out, &segs, input)?;
        return Ok(out);
    }
    // 无前缀：按段数区分 1/2/5 三种简写形式
    match segs.len() {
        1 => out.name = Some(segs[0].to_string()),
        2 => {
            // 两段简写首段须为合法 kind
            if !DRN_KINDS.contains(&segs[0]) {
                return Err(format!("DRN 两段简写首段须为 kind：{input}"));
            }
            out.kind = Some(segs[0].to_string());
            out.name = Some(segs[1].to_string());
        }
        5 => assign5(&mut out, &segs, input)?,
        _ => return Err(format!("DRN 段数只支持 1/2/5：{input}")),
    }
    Ok(out)
}

/// 把 5 段绝对 DRN 写入 `Drn`，并校验第 4 段为合法 kind。
fn assign5(out: &mut Drn, segs: &[&str], input: &str) -> Result<(), String> {
    out.domain = Some(segs[0].to_string());
    out.app = Some(segs[1].to_string());
    out.module = Some(segs[2].to_string());
    // 第 4 段须为合法 kind
    if !DRN_KINDS.contains(&segs[3]) {
        return Err(format!("DRN kind 非法「{}」：{input}", segs[3]));
    }
    out.kind = Some(segs[3].to_string());
    out.name = Some(segs[4].to_string());
    Ok(())
}

/// 解析深链片段（`#table` 或 `#table.field`）写入 `Drn.table` / `Drn.field`。
fn parse_fragment(frag: &str, out: &mut Drn, input: &str) -> Result<(), String> {
    if frag.is_empty() {
        return Err(format!("DRN 深链片段为空：{input}"));
    }
    match frag.find('.') {
        // table.field 形式
        Some(dot) => {
            let t = &frag[..dot];
            let f = &frag[dot + 1..];
            if !is_seg(t) || !is_seg(f) {
                return Err(format!("DRN 深链 table.field 非法：{input}"));
            }
            out.table = Some(t.to_string());
            out.field = Some(f.to_string());
        }
        // 仅 table 形式
        None => {
            if !is_seg(frag) {
                return Err(format!("DRN 深链 table 非法：{input}"));
            }
            out.table = Some(frag.to_string());
        }
    }
    Ok(())
}

/// 引用方 DAM（继承源）。
///
/// 相对 DRN 缺失的 domain/app/module 段从引用方 DAM 继承补全。
#[derive(Debug, Clone, Default)]
pub struct FromDam {
    /// 引用方业务域。
    pub domain: Option<String>,
    /// 引用方应用标识。
    pub app: Option<String>,
    /// 引用方模块标识。
    pub module: Option<String>,
}

/// 绝对 DRN（各段已补全）。
#[derive(Debug, Clone, PartialEq)]
pub struct AbsDrn {
    /// 业务域。
    pub domain: String,
    /// 应用标识。
    pub app: String,
    /// 模块标识。
    pub module: String,
    /// 资源类型（DCT/DOC/FLC/BASE）。
    pub kind: String,
    /// 资源名称。
    pub name: String,
    /// 版本号。
    pub version: Option<u64>,
    /// 深链表名。
    pub table: Option<String>,
    /// 深链字段名。
    pub field: Option<String>,
}

/// 归一为绝对 DRN：补全继承段、展开别名。imports: [{alias, drn}]。
///
/// 流程：先 `parse_drn` → 若为别名则查 imports 展开目标 DRN → 补全缺失 DAM 段 →
/// 校验 kind/name 必填 → BASE 特例（domain=base, app/module 占位为 `_`）。
///
/// # Arguments
///
/// * `input` - 待归一的 DRN 字符串（可为别名/简写/绝对形式）。
/// * `from` - 引用方 DAM，用于补全相对引用缺失的 domain/app/module。
/// * `default_kind` - kind 缺失时的兜底类型（如 `"DCT"`）。
/// * `imports` - 顶层 imports 别名表（`[{alias, drn}]`），用于展开 `@别名`。
///
/// # Returns
///
/// 成功返回各段补全后的 `AbsDrn`；别名未声明、缺 kind/name 或 DAM 无法补全时返回 `Err(message)`。
pub fn normalize_drn(
    input: &str,
    from: &FromDam,
    default_kind: Option<&str>,
    imports: Option<&Value>,
) -> Result<AbsDrn, String> {
    let mut p = parse_drn(input)?;

    // 别名：在 imports 别名表中查找目标 DRN 并展开
    if p.is_alias {
        let alias = p.alias.clone().unwrap_or_default();
        let hit = imports
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|x| x.get("alias").and_then(|a| a.as_str()) == Some(alias.as_str()))
            })
            .and_then(|x| x.get("drn").and_then(|d| d.as_str()));
        let target = hit.ok_or_else(|| format!("DRN 别名未在 imports 声明：@{alias}"))?;
        // 保留别名上挂的深链片段，目标 DRN 无深链时回填
        let frag_table = p.table.clone();
        let frag_field = p.field.clone();
        p = parse_drn(target)?;
        if frag_table.is_some() && p.table.is_none() {
            p.table = frag_table;
            p.field = frag_field;
        }
    }

    let name = p.name.ok_or("DRN 缺少 name")?;
    // kind 缺失则取兜底默认
    let kind = p
        .kind
        .or_else(|| default_kind.map(String::from))
        .ok_or_else(|| format!("DRN 缺少 kind 且上下文未提供默认：{name}"))?;

    // BASE 特例：domain 固定为 base，app/module 占位为 _
    if kind == "BASE" {
        return Ok(AbsDrn {
            domain: "base".into(),
            app: "_".into(),
            module: "_".into(),
            kind,
            name,
            version: p.version,
            table: p.table,
            field: p.field,
        });
    }
    // 非 BASE：从引用方 DAM 补全缺失的 domain/app/module
    let domain = p.domain.or_else(|| from.domain.clone());
    let app = p.app.or_else(|| from.app.clone());
    let module = p.module.or_else(|| from.module.clone());
    match (domain, app, module) {
        (Some(domain), Some(app), Some(module)) => Ok(AbsDrn {
            domain,
            app,
            module,
            kind,
            name,
            version: p.version,
            table: p.table,
            field: p.field,
        }),
        _ => Err(format!("DRN 无法补全 DAM：{name}")),
    }
}

/// 绝对 DRN → 规范字符串。
///
/// 输出形如 `drn:<domain>/<app>/<module>/<kind>/<name>[@<version>][#<table>[.<field>]]`。
pub fn format_drn(abs: &AbsDrn) -> String {
    let mut s = format!(
        "drn:{}/{}/{}/{}/{}",
        abs.domain, abs.app, abs.module, abs.kind, abs.name
    );
    // 版本后缀
    if let Some(v) = abs.version {
        s.push_str(&format!("@{v}"));
    }
    // 深链后缀
    if let Some(t) = &abs.table {
        s.push_str(&format!("#{t}"));
        if let Some(f) = &abs.field {
            s.push_str(&format!(".{f}"));
        }
    }
    s
}

/// 绝对 DRN → 落盘相对路径 `<domain>/<app>/<module>/<kind>/<name>[_v<n>].json`。
///
/// # Arguments
///
/// * `abs` - 已补全的绝对 DRN。
/// * `with_version` - 是否在文件名中带上 `_v<N>` 版本后缀。
pub fn drn_to_path(abs: &AbsDrn, with_version: bool) -> String {
    let file = if with_version {
        // 带版本：有 version 则附 _vN，否则用裸 name
        if let Some(v) = abs.version {
            format!("{}_v{}", abs.name, v)
        } else {
            abs.name.clone()
        }
    } else {
        abs.name.clone()
    };
    format!(
        "{}/{}/{}/{}/{}.json",
        abs.domain, abs.app, abs.module, abs.kind, file
    )
}

/// 可见性：目标 visibility 是否允许 from 引用。缺省(None)按 public 放行。
///
/// 四级可见性：private（同模块）/ app（同应用）/ domain（同域）/ public（全局）。
pub fn drn_visible_from(target_visibility: Option<&str>, target: &AbsDrn, from: &FromDam) -> bool {
    let fd = from.domain.as_deref().unwrap_or("");
    let fa = from.app.as_deref().unwrap_or("");
    let fm = from.module.as_deref().unwrap_or("");
    // 缺省按 public 放行
    match target_visibility.unwrap_or("public") {
        "private" => target.domain == fd && target.app == fa && target.module == fm,
        "app" => target.domain == fd && target.app == fa,
        "domain" => target.domain == fd,
        _ => true,
    }
}

/// 把 refDict/dictId 引用归一为「有效 dictId」，兼容两种写法：
///
/// - 裸 code（如 `cost_center`）：原样返回，走全局字典注册表按 code 查（向后兼容，无需 from）。
/// - DRN / 别名（`@cc` / `DCT/x` / `drn:dom/app/mod/DCT/x`）：经 normalize_drn 展开，取 name 段作为有效 dictId（字典 schema 按 dictId 全局唯一，故 name 即查找键；DAM 段保留供将来按域查）。
///
/// 无法归一时（缺 from 的相对引用 / 未声明别名等）退回原值，尽力而为、不阻断。
pub fn effective_dict_id(raw: &str, from: &FromDam, imports: Option<&Value>) -> String {
    let s = raw.trim();
    // 裸 code 快速路径：不含 DRN 结构标记（@/drn:/）且无斜杠 → 原样返回
    if !s.starts_with('@') && !s.starts_with("drn:") && !s.contains('/') {
        return s.to_string();
    }
    // DRN/别名/简写：归一为绝对 DRN 后取 name 段作为有效 dictId
    match normalize_drn(s, from, Some("DCT"), imports) {
        Ok(abs) => abs.name,
        // 归一失败退回原值，尽力而为、不阻断
        Err(_) => s.to_string(),
    }
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
    fn parse_full_with_version_and_deeplink() {
        let p = parse_drn("drn:fi/cmxfico/gl/DOC/gl_md_doc_meta@2#voucher_detail.cashflow_item_id")
            .unwrap();
        assert_eq!(p.domain.as_deref(), Some("fi"));
        assert_eq!(p.kind.as_deref(), Some("DOC"));
        assert_eq!(p.version, Some(2));
        assert_eq!(p.table.as_deref(), Some("voucher_detail"));
        assert_eq!(p.field.as_deref(), Some("cashflow_item_id"));
    }

    #[test]
    fn bare_and_two_seg() {
        assert_eq!(
            parse_drn("cost_center").unwrap().name.as_deref(),
            Some("cost_center")
        );
        let p = parse_drn("DCT/cost_center").unwrap();
        assert_eq!(p.kind.as_deref(), Some("DCT"));
    }

    #[test]
    fn alias_with_fragment() {
        let p = parse_drn("@v#voucher_detail.amount").unwrap();
        assert!(p.is_alias);
        assert_eq!(p.alias.as_deref(), Some("v"));
        assert_eq!(p.table.as_deref(), Some("voucher_detail"));
    }

    #[test]
    fn invalid_inputs() {
        assert!(parse_drn("").is_err());
        assert!(parse_drn("drn:fi/cmxfico/gl/DOC").is_err());
        assert!(parse_drn("drn:fi/cmxfico/gl/XXX/n").is_err());
        assert!(parse_drn("drn:fi/cmxfico/gl/DOC/n@x").is_err());
        assert!(parse_drn("a/b/c").is_err());
    }

    #[test]
    fn normalize_inherits_and_absolute() {
        let abs = normalize_drn("cost_center", &from(), Some("DCT"), None).unwrap();
        assert_eq!(abs.app, "cmxfico");
        assert_eq!(abs.kind, "DCT");
        let abs = normalize_drn(
            "drn:fi/shared-md/masterdata/DCT/cost_center@1",
            &from(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(abs.app, "shared-md");
        assert_eq!(abs.version, Some(1));
    }

    #[test]
    fn normalize_alias_expand() {
        let imports = json!([{"alias":"cc","drn":"drn:fi/shared-md/masterdata/DCT/cost_center"}]);
        let abs = normalize_drn("@cc", &from(), None, Some(&imports)).unwrap();
        assert_eq!(abs.app, "shared-md");
        assert_eq!(abs.name, "cost_center");
    }

    #[test]
    fn normalize_base_placeholder() {
        let abs = normalize_drn("BASE/base_doc_meta", &from(), None, None).unwrap();
        assert_eq!(abs.domain, "base");
        assert_eq!(abs.app, "_");
    }

    #[test]
    fn normalize_errors() {
        assert!(normalize_drn("cost_center", &from(), None, None).is_err()); // no kind
        assert!(normalize_drn("@nope", &from(), None, Some(&json!([]))).is_err());
    }

    #[test]
    fn format_and_path_roundtrip() {
        let s = "drn:fi/shared-md/masterdata/DCT/cost_center@1";
        let abs = normalize_drn(s, &from(), None, None).unwrap();
        assert_eq!(format_drn(&abs), s);
        assert_eq!(
            drn_to_path(&abs, false),
            "fi/shared-md/masterdata/DCT/cost_center.json"
        );
        assert_eq!(
            drn_to_path(&abs, true),
            "fi/shared-md/masterdata/DCT/cost_center_v1.json"
        );
    }

    #[test]
    fn visibility_levels() {
        let target = AbsDrn {
            domain: "fi".into(),
            app: "shared-md".into(),
            module: "masterdata".into(),
            kind: "DCT".into(),
            name: "x".into(),
            version: None,
            table: None,
            field: None,
        };
        let same_app = FromDam {
            domain: Some("fi".into()),
            app: Some("shared-md".into()),
            module: Some("other".into()),
        };
        let other_domain = FromDam {
            domain: Some("hr".into()),
            app: Some("x".into()),
            module: Some("y".into()),
        };
        assert!(drn_visible_from(Some("app"), &target, &same_app));
        assert!(!drn_visible_from(Some("app"), &target, &other_domain));
        assert!(drn_visible_from(Some("public"), &target, &other_domain));
        assert!(drn_visible_from(None, &target, &other_domain));
    }

    #[test]
    fn effective_dict_id_bare_and_drn() {
        // 裸 code 原样（向后兼容，无需 from）
        assert_eq!(
            effective_dict_id("cost_center", &FromDam::default(), None),
            "cost_center"
        );
        // 绝对 DRN → name 段
        assert_eq!(
            effective_dict_id(
                "drn:fi/shared-md/masterdata/DCT/cost_center@1",
                &from(),
                None
            ),
            "cost_center"
        );
        // 别名 → 展开后 name 段
        let imports = json!([{"alias":"cc","drn":"drn:fi/shared-md/masterdata/DCT/currency"}]);
        assert_eq!(
            effective_dict_id("@cc", &from(), Some(&imports)),
            "currency"
        );
        // 两段简写 DCT/x → name 段
        assert_eq!(effective_dict_id("DCT/partner", &from(), None), "partner");
        // 无法归一（未声明别名）→ 退回原值，不阻断
        assert_eq!(
            effective_dict_id("@nope", &from(), Some(&json!([]))),
            "@nope"
        );
    }
}
