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
    /// 业务域（如 fi / hr），base 域特例为 "base"。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用标识（兼容字段，与 `app` 等价，优先取 application）。
    #[serde(default)]
    pub application: Option<String>,
    /// 应用标识（与 `application` 等价，作为兜底）。
    #[serde(default)]
    pub app: Option<String>,
    /// 模块标识（业务模块编码）。
    #[serde(default)]
    pub module: Option<String>,
    /// 文件名（兼容字段，与 `id` 等价，优先取 file）。
    #[serde(default)]
    pub file: Option<String>,
    /// 文件名（兼容字段，与 `file` 等价，作为兜底）。
    #[serde(default)]
    pub id: Option<String>,
}

impl DefRef {
    /// 归一应用标识：优先 `application`，缺失则取 `app`。
    fn app_value(&self) -> Option<&str> {
        self.application.as_deref().or(self.app.as_deref())
    }
    /// 归一文件名：优先 `file`，缺失则取 `id`。
    fn file_value(&self) -> Option<&str> {
        self.file.as_deref().or(self.id.as_deref())
    }
}

/// 校验四段（domain/application/module/file）并返回相对 definitions 根的路径段。
///
/// base 域特例：只需 domain + file，跳过 application/module 校验。
/// 各段通过 `is_safe_segment` / `is_safe_json_file` 防路径穿越。
///
/// # Arguments
///
/// * `r` - 定义文件引用（含 domain/application/module/file 四段）。
///
/// # Returns
///
/// 成功返回相对路径段数组（base 域为 2 段，其余为 4 段）；任一段缺失或非法返回 `PortalError::BadRequest`。
fn resolve_rel(r: &DefRef) -> PortalResult<Vec<String>> {
    // domain 必填，且须为安全段
    let domain = r.domain.as_deref().unwrap_or("").trim();
    if domain.is_empty() {
        return Err(PortalError::bad_request("缺少必填参数 domain"));
    }
    if !is_safe_segment(domain) {
        return Err(PortalError::bad_request(format!(
            "参数 domain 非法（仅允许字母、数字、_-）：\"{domain}\""
        )));
    }
    // file 须为 *.json 安全文件名
    let file = r.file_value().unwrap_or("").trim();
    if !is_safe_json_file(file) {
        return Err(PortalError::bad_request(format!(
            "参数 file 非法（须 *.json，仅允许字母、数字、._-）：\"{file}\""
        )));
    }
    // base 公共模板域特例：直接落在 meta/definitions/base/<file>
    if domain == "base" {
        return Ok(vec!["base".to_string(), file.to_string()]);
    }
    // 非 base 域：application / module 必填并校验安全段
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

/// 由相对路径段拼接 definitions 根下的绝对路径（data/meta/definitions/<rel_parts>）。
fn abs_path(rel_parts: &[String]) -> std::path::PathBuf {
    let mut p = data_path(["meta", "definitions"]);
    for seg in rel_parts {
        p.push(seg);
    }
    p
}

/// 读取定义文件（原样 JSON）。
///
/// # Arguments
///
/// * `r` - 定义文件引用，定位 `meta/definitions/<...>` 下的目标文件。
///
/// # Returns
///
/// 成功返回文件解析后的 JSON 值；文件不存在时返回 `PortalError::NotFound`（含定位信息）。
pub async fn get_definition(r: &DefRef) -> PortalResult<serde_json::Value> {
    let rel = resolve_rel(r)?;
    match read_json::<serde_json::Value>(&abs_path(&rel)).await {
        Ok(v) => Ok(v),
        // 文件缺失：转语义化 NotFound，并在错误信息里带上定位路径
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

/// 取定义文件的业务元信息节点（docMeta / dctMeta / seedMeta），三键探测。
/// BASE 用 baseMeta，由调用方单独处理，不走此函数。
fn meta_node(doc: &serde_json::Value) -> Option<&serde_json::Value> {
    doc.get("docMeta")
        .or_else(|| doc.get("dctMeta"))
        .or_else(|| doc.get("seedMeta"))
}

/// 由文档推断 base 文件名（DCT→baseDctMetaRef.file，DOC→baseDocMetaRef.file）。
///
/// # Arguments
///
/// * `doc` - 已加载的定义文档 JSON。
///
/// # Returns
///
/// 返回关联的 base 字段集文件名；无关联（非 DCT/DOC 或引用缺 file）时返回 `None`。
fn infer_base_file(doc: &serde_json::Value) -> Option<String> {
    // 按 metaKind 选择引用键：DCT 用 baseDctMetaRef，DOC 用 baseDocMetaRef
    let kind = meta_node(doc)
        .and_then(|m| m.get("metaKind"))
        .and_then(|v| v.as_str());
    let key = match kind {
        Some("DCT") => "baseDctMetaRef",
        Some("DOC") => "baseDocMetaRef",
        _ => return None,
    };
    // 取引用对象下的 file 字段，空串视为无引用
    doc.get(key)
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 文档 kind 判定（DCT/DOC/BASE/UNKNOWN）。
///
/// 优先取 `meta_node(doc).metaKind`；否则按是否含 `baseMeta` 判 BASE；都不匹配返回 UNKNOWN。
fn doc_kind(doc: &serde_json::Value) -> String {
    if let Some(k) = meta_node(doc)
        .and_then(|m| m.get("metaKind"))
        .and_then(|v| v.as_str())
    {
        return k.to_string();
    }
    // 无 docMeta/dctMeta/seedMeta.metaKind：含 baseMeta 判为 BASE，否则 UNKNOWN
    if doc.get("baseMeta").is_some() {
        "BASE".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

/// 批量读取定义 + 附带 base 字段集（去重）。
///
/// 输入支持两种形态：`{ refs: [...] }` 或顶层数组；refs 元素可为字符串（=file）或对象。
/// 当 `includeBase` 为真时，收集各主定义引用的 base 文件（去重）一并加载。
///
/// # Arguments
///
/// * `input` - 批量请求体，含 `refs`（或顶层数组）与可选的 `includeBase` 开关。
///
/// # Returns
///
/// 返回 `{ items, bases, basePaths, errors }`：items 为各定义摘要+全文，bases/basePaths 为附带 base 字段集。
pub async fn get_definitions_batch(input: &serde_json::Value) -> PortalResult<serde_json::Value> {
    // 是否附带 base 字段集，默认开启
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
    // 归一为 DefRef：字符串元素视为 file，对象元素反序列化
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
    // base 文件去重集合：多定义引用同一 base 时只加载一次
    let mut base_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    // 逐个读取主定义，收集摘要 + 全文 + 推断出的 base 引用
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
                // 按需收集 base 引用（去重），供后续统一加载
                if include_base
                    && let Some(bf) = infer_base_file(&doc) {
                        base_files.insert(bf);
                    }
            }
            // 单条失败不阻断，记录错误继续处理其余项
            Err(e) => errors.push(json!({ "ref": serde_json::to_value(r).unwrap_or(json!({})), "error": e.to_string() })),
        }
    }

    let mut bases = serde_json::Map::new();
    let mut base_paths = serde_json::Map::new();
    // 统一加载去重后的 base 字段集文件
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
///
/// 例：`cmxfico_dct_meta_v1.json` → `cmxfico_dct_meta`；无版本后缀则去掉 `.json`。
fn def_file_stem(file: &str) -> String {
    let base = file.strip_suffix(".json").unwrap_or(file);
    // 定位最后一个 _v，其后须全部为数字才视为版本后缀
    if let Some(idx) = base.rfind("_v") {
        let suffix = &base[idx + 2..];
        if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
            return base[..idx].to_string();
        }
    }
    base.to_string()
}

/// 统计 DOC 文档的表数量：主表数 + 每张表的 summaries/sum 子表数。
fn doc_table_count(doc: &serde_json::Value) -> usize {
    doc.get("voucherTables")
        .and_then(|v| v.as_array())
        .map(|tables| {
            tables
                .iter()
                .map(|t| {
                    // 每张表：自身 1 + summaries 子表数 + sum 子表数
                    1 + t
                        .get("summaries")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0)
                        + t.get("sum")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

/// 由已解析文档抽取列表摘要。
///
/// 公共字段含 domain/application/module/file/kind/version/versionName/isDefault/updatedAt/stem，
/// 再按 kind（DCT/DOC/BASE）补充各自的 title/moduleCode/remark/tableCount 或 fieldSetCount。
fn summarize(
    domain: &str,
    application: &str,
    module: &str,
    file: &str,
    doc: &serde_json::Value,
) -> serde_json::Value {
    let mm = meta_node(doc).cloned().unwrap_or(json!({}));
    let kind = doc_kind(doc);
    // 版本号：优先 docMeta/dctMeta/seedMeta.version，其次 baseMeta.version，缺省为 1
    let version = mm
        .get("version")
        .or_else(|| doc.get("baseMeta").and_then(|b| b.get("version")))
        .cloned()
        .unwrap_or(json!(1));
    // 版本名称（多版本下拉展示用，文件名承载不了，存 docMeta/dctMeta/seedMeta/baseMeta.versionName）。
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
    // 按 kind 补充各自特有的摘要字段（title/moduleCode/remark + 数量统计）
    match kind.as_str() {
        // DCT 数据字典：表数取自 dictionaryTables
        "DCT" => {
            obj.insert(
                "title".into(),
                json!(
                    mm.get("metaName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file)
                ),
            );
            obj.insert(
                "dctGroupCode".into(),
                json!(mm.get("dctGroupCode").and_then(|v| v.as_str()).unwrap_or("")),
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
            // DOC 业务单据：表数含主表与 summaries/sum 子表
            obj.insert(
                "title".into(),
                json!(
                    mm.get("metaName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file)
                ),
            );
            obj.insert(
                "docCode".into(),
                json!(mm.get("docCode").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert(
                "remark".into(),
                json!(mm.get("remark").and_then(|v| v.as_str()).unwrap_or("")),
            );
            obj.insert("tableCount".into(), json!(doc_table_count(doc)));
        }
        // BASE 字段集模板：字段集数量取自 fieldSets
        "BASE" => {
            let bm = doc.get("baseMeta").cloned().unwrap_or(json!({}));
            obj.insert(
                "title".into(),
                json!(bm.get("metaName").and_then(|v| v.as_str()).unwrap_or(file)),
            );
            obj.insert(
                "metaCode".into(),
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
///
/// 递归扫描 `meta/definitions/` 三层目录（domain→application→module），
/// base 域特例直接扫描 `base/`。结果按 domain/application/module/file 排序。
///
/// # Arguments
///
/// * `kind` - 过滤的元数据类型（DCT/DOC/BASE），`None` 表示不过滤。
/// * `domain` - 过滤的业务域，`None` 表示不过滤。
/// * `application` - 过滤的应用标识，`None` 表示不过滤。
/// * `module` - 过滤的模块标识，`None` 表示不过滤。
///
/// # Returns
///
/// 返回各定义的摘要列表；根目录不存在时返回空列表。
pub async fn list_definitions(
    kind: Option<&str>,
    domain: Option<&str>,
    application: Option<&str>,
    module: Option<&str>,
) -> PortalResult<Vec<serde_json::Value>> {
    let root = data_path(["meta", "definitions"]);
    // 归一过滤参数为 trimmed 大小写字符串（kind 转大写）
    let want_kind = kind.unwrap_or("").to_uppercase();
    let want_domain = domain.unwrap_or("").trim().to_string();
    let want_app = application.unwrap_or("").trim().to_string();
    let want_module = module.unwrap_or("").trim().to_string();

    let mut out: Vec<serde_json::Value> = Vec::new();

    // 根目录缺失视为空结果，不报错
    let mut domains = match tokio::fs::read_dir(&root).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(PortalError::Io(e)),
    };
    // 第一层：domain
    while let Some(d) = domains.next_entry().await.map_err(PortalError::Io)? {
        let dname = d.file_name().to_string_lossy().to_string();
        // 跳过隐藏目录与非目录
        if dname.starts_with('.') || !d.file_type().await.map_err(PortalError::Io)?.is_dir() {
            continue;
        }
        // base 域特例：直接扫描 base/*.json，无 application/module 层级
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
        // 第二层：application
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
            // 第三层：module
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
    // 按 domain/application/module/file 字典序排序，保证列表稳定
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
///
/// 读取出错的单文件不阻断扫描，以 `kind:"UNKNOWN"` + error 记录到结果中。
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
        // 仅处理安全的 *.json 文件，跳过目录与非法文件名
        if !f.file_type().await.map_err(PortalError::Io)?.is_file() || !is_safe_json_file(&fname) {
            continue;
        }
        match read_json::<serde_json::Value>(&f.path()).await {
            Ok(doc) => {
                let item = summarize(domain, application, module, &fname, &doc);
                // kind 过滤：非目标类型跳过
                if !want_kind.is_empty() && item["kind"].as_str().unwrap_or("") != want_kind {
                    continue;
                }
                out.push(item);
            }
            // 损坏文件以 UNKNOWN + error 记录，不阻断扫描
            Err(e) => out.push(json!({
                "domain": domain, "application": application, "app": application,
                "module": module, "file": fname, "kind": "UNKNOWN", "error": e.to_string()
            })),
        }
    }
    Ok(())
}

/// 保存定义（补 updatedAt，原子写）。
///
/// # Arguments
///
/// * `r` - 定义文件引用，定位落盘路径。
/// * `doc` - 待保存的文档 JSON，必须是对象。
///
/// # Returns
///
/// 成功返回写入的文档（含自动补的 `updatedAt`）；非对象请求体返回 `PortalError::BadRequest`。
pub async fn save_definition(
    r: &DefRef,
    doc: &serde_json::Value,
) -> PortalResult<serde_json::Value> {
    if !doc.is_object() {
        return Err(PortalError::bad_request("请求体必须是对象"));
    }
    let rel = resolve_rel(r)?;
    let mut merged = doc.clone();
    // 自动补 updatedAt 时间戳，供版本管理列表展示
    if let Some(obj) = merged.as_object_mut() {
        obj.insert(
            "updatedAt".to_string(),
            json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        );
    }
    // 全局写锁，保证原子写不被并发覆盖
    let _guard = write_lock().lock().await;
    write_json_atomic(&abs_path(&rel), &merged, true).await?;
    Ok(merged)
}

/// 在文档的业务元信息节点（docMeta/dctMeta/seedMeta，或 baseMeta）上写 isDefault 标记，返回是否有改动。
///
/// 按 kind 选键：DOC→docMeta、DCT→dctMeta、SEED→seedMeta、BASE→baseMeta。
fn set_doc_default_flag(doc: &mut serde_json::Value, value: bool) -> bool {
    // 先判 kind 决定写哪个键：DOC/DCT/SEED 写对应业务节点，BASE 写 baseMeta。
    let key = match doc_kind(doc).as_str() {
        "DOC" => "docMeta",
        "DCT" => "dctMeta",
        "SEED" => "seedMeta",
        _ => "baseMeta",
    };
    let obj = doc.as_object_mut();
    let Some(obj) = obj else { return false };
    // 确保 meta 节点存在且为对象
    let meta = obj.entry(key).or_insert_with(|| json!({}));
    let Some(meta) = meta.as_object_mut() else {
        return false;
    };
    // 仅当当前值与目标不同时才改动，避免无谓写入
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
///
/// 仅改动 isDefault 变化的文件，并为其补 updatedAt。返回受影响文件列表。
///
/// # Arguments
///
/// * `r` - 目标定义文件引用（base 域不支持版本管理，会返回错误）。
///
/// # Returns
///
/// 返回 `{ ok, default, changed }`，changed 为实际改动了 isDefault 的文件名列表。
pub async fn set_default_version(r: &DefRef) -> PortalResult<serde_json::Value> {
    // base 公共模板无版本管理，直接拒绝
    if r.domain.as_deref().unwrap_or("").trim() == "base" {
        return Err(PortalError::bad_request("base 公共模板无版本管理"));
    }
    let rel = resolve_rel(r)?;
    let target_file = rel.last().cloned().unwrap_or_default();
    let dir = abs_path(&rel[..rel.len() - 1]);
    // stem 用于识别同一逻辑定义的所有版本（含 _vN 后缀）
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
    // 扫描同目录下同 stem 的所有版本，做互斥置位
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
        // 目标文件置 true，其余置 false
        let want = fname == target_file;
        let mut doc = match read_json::<serde_json::Value>(&f.path()).await {
            Ok(d) => d,
            Err(_) => continue, // 损坏文件跳过，不阻断
        };
        // 仅当 isDefault 变化才写入，并补 updatedAt
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
///
/// # Arguments
///
/// * `r` - 待删除定义文件引用（base 域拒绝删除）。
///
/// # Returns
///
/// 成功返回 `{ ok: true }`；文件不存在返回 `PortalError::NotFound`。
pub async fn delete_definition(r: &DefRef) -> PortalResult<serde_json::Value> {
    // base 公共模板受保护，禁止删除
    if r.domain.as_deref().unwrap_or("").trim() == "base" {
        return Err(PortalError::bad_request("base 公共模板不可删除"));
    }
    let rel = resolve_rel(r)?;
    let _guard = write_lock().lock().await;
    match tokio::fs::remove_file(abs_path(&rel)).await {
        Ok(()) => Ok(json!({ "ok": true })),
        // 文件缺失：转语义化 NotFound
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
