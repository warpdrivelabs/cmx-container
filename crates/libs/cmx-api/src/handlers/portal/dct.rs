//! 数据字典（DCT）数据装载/回存 HTTP handler —— tokio-postgres 驱动，直读/写 cf_* 物理表。
//!
//! 端点：
//!   - `GET  /api/dct/meta`         —— 字典显示元数据（列 caption/类型/PK/是否自分级）
//!   - `POST /api/dct/data/search`  —— 装载字典数据（flat / 自分级 children，parentId 过滤）
//!   - `POST /api/dct/entries`      —— 回存（upsert，merge 语义）
//!   - `DELETE /api/dct/entries/{id}` —— 删除一行
//!
//! 分层：handler 读定义 JSON（`definitions::store::get_definition`）拿到目标字典表的
//! tableName/列/主键，构造参数化 SQL，经 `cmx_database_pg` 的 tokio-postgres 管理器执行。
//! 不复用 doc/ 的 DocMetaView/loader（字典是单表，无需跨层机制），保持精简。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::debug;

use cmx_database_pg::get_default_pg_db_manager;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

// ============================================================================
// 请求参数
// ============================================================================

/// `/api/dct/*` 共用坐标：定位定义文件 + 其中哪张字典表。
///
/// `file` 可选：缺失时由 [`resolve_dict_file`] 在 domain/app/module 下自动扫描
/// 含该 dictCode 的 DCT 文件（优先 isDefault、回退 version 最大）。
/// 这样前端运行时只需传 domain/app/module/dict 四元（运行时 host 无 file 坐标）。
#[derive(Debug, Deserialize)]
pub struct DctQuery {
    pub domain: String,
    pub application: String,
    pub module: String,
    /// 定义文件名（如 cmxfico_dct_meta_v1.json）；可选，缺失时自动解析。
    pub file: Option<String>,
    /// 字典表 dictCode（如 currency / gl_account / bus_partner）
    pub dict: String,
}

// ============================================================================
// 元数据解析：从定义 JSON 找到目标字典表 + 合并列
// ============================================================================

/// 解析出的字典表视图（供 SQL 构造 + 元数据投影用）。
struct DictView {
    dict_code: String,
    dict_name: String,
    table_name: String,
    id_field: String,
    code_field: String,
    label_field: String,
    parent_field: Option<String>,
    self_hierarchy: bool,
    /// 合并后的列（own fields + 全部 *FieldSet 引用），去重保序。
    columns: Vec<DictColumn>,
    /// 主键列名（有 id 用 id；无 id 用 code）。
    pk: String,
    /// 落库前列级校验规范（进程内缓存，含类型/长度/精度/nullable）。
    spec: std::sync::Arc<cmx_biz::validation::TableSpec>,
}

#[derive(Clone)]
struct DictColumn {
    name: String,
    caption: String,
    data_type: String,
    is_pk: bool,
    nullable: bool,
    /// 维度类型（attribute|dimension），供前端列模型分组/排序。
    dim_type: String,
    /// 引用字典编码（如 comp_unit）。空 = 非字典列。
    ref_dict: String,
    /// 显示字段（字典回显用，如 name）。
    display_field: String,
    /// 写回字段（字典选值写回行，如 code/id）。
    ref_field: String,
    /// 物理字段名（如 MANDT），空则无。
    physical_field: String,
    /// 录入控件配置（原样透传 edit{}）。
    edit: Option<Value>,
    /// 编辑设置（原样透传 editSettings{}）。
    edit_settings: Option<Value>,
    /// 显示属性（原样透传 display{}，如下沉后的 decimalDigits/format）。
    display: Option<Value>,
}

/// file 缺失时自动解析的进程内缓存：`domain/app/module/dict` → file。
/// 避免每次 search 都扫描该模块下的所有 DCT 文件（文件小、数量有限，但仍省 IO）。
/// 开发期改了定义文件后缓存可能短暂过期，但 dictCode→file 的归属关系极少变。
static DICT_FILE_CACHE: std::sync::OnceLock<tokio::sync::RwLock<std::collections::HashMap<String, String>>> =
    std::sync::OnceLock::new();

fn dict_file_cache() -> &'static tokio::sync::RwLock<std::collections::HashMap<String, String>> {
    DICT_FILE_CACHE.get_or_init(|| tokio::sync::RwLock::new(std::collections::HashMap::new()))
}

/// 解析字典操作的 db_id：前端显式传 `db_id` header 时用它，缺失时回退到业务库（source_type=biz）。
/// 字典数据通常建在业务库（如 fico-db），而非默认的主控库（primary）。
/// 前端字典兜底数据源（cmx-dict-select 的 createRestDictDataSource）不带 db_id，
/// 这里经 get_biz_db_id() 自动路由到业务库，免去前端手填。
async fn resolve_db_id(headers: &HeaderMap) -> String {
    if let Some(v) = headers.get("db_id").and_then(|h| h.to_str().ok()) {
        let s = v.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    get_default_pg_db_manager().get_biz_db_id().await
}

/// 判断字典表项是否与目标标识匹配：同时认 dictCode（如 comp_unit）和 tableName（如 cf_comp_unit）。
/// 前端字典池统一用 tableName（物理表名），但 DctQuery.dict 也可能传 dictCode，故两者都比对。
pub(crate) fn dict_matches(t: &Value, target: &str) -> bool {
    let m = match t.get("dictMeta") {
        Some(m) => m,
        None => return false,
    };
    m.get("dictCode").and_then(|v| v.as_str()) == Some(target)
        || m.get("tableName").and_then(|v| v.as_str()) == Some(target)
}

/// file 缺失时：在 domain/app/module 下扫描 DCT 文件，找含 dictCode 的那份定义文件。
///
/// 选版本策略（与前端 `_pickDefaultDefinitions` 一致）：
///   1. 优先该 stem 组里 `isDefault=true` 的；多个取 version 最大；
///   2. 无 isDefault 则取该 stem 组 version 最大的；
///   3. 逐候选文件读 `dictionaryTables` 找 `dictMeta.dictCode == dict`，第一个命中的返回。
///
/// 缓存结果（键 `domain/app/module/dict`）。定义文件改动后若需立即生效，重启服务即可。
pub(crate) async fn resolve_dict_file(domain: &str, app: &str, module: &str, dict: &str) -> Result<String> {
    let cache_key = format!("{domain}/{app}/{module}/{dict}");
    if let Some(f) = dict_file_cache().read().await.get(&cache_key).cloned() {
        return Ok(f);
    }
    let items = cmx_portal::definitions::store::list_definitions(
        Some("DCT"),
        Some(domain),
        Some(app),
        Some(module),
    )
    .await?;
    // 提取 owned 摘要元组，避开对 items 的引用生命周期纠缠。
    // (stem, file, is_default, version)：stem 用于分组，其余用于选版本。
    let entries: Vec<(String, String, bool, u64)> = items
        .iter()
        .filter_map(|it| {
            let stem = it.get("stem").and_then(|v| v.as_str())?.to_string();
            let file = it.get("file").and_then(|v| v.as_str())?.to_string();
            let is_default = it.get("isDefault").and_then(|x| x.as_bool()).unwrap_or(false);
            let version = it.get("version").and_then(|x| x.as_u64()).unwrap_or(0);
            Some((stem, file, is_default, version))
        })
        .collect();
    // 按 stem 分组，每组选出代表（isDefault 优先，否则 version 最大）。
    let mut groups: std::collections::HashMap<String, Vec<(String, bool, u64)>> =
        std::collections::HashMap::new();
    for (stem, file, is_default, version) in &entries {
        groups
            .entry(stem.clone())
            .or_default()
            .push((file.clone(), *is_default, *version));
    }
    let pick = |arr: &[(String, bool, u64)]| -> Option<String> {
        // 优先 isDefault=true 的；无则全员；组内取 version 最大者的 file。
        let any_default = arr.iter().any(|(_, d, _)| *d);
        arr.iter()
            .filter(|(_, d, _)| if any_default { *d } else { true })
            .max_by_key(|(_, _, v)| *v)
            .map(|(f, _, _)| f.clone())
    };
    // 收集候选文件（每组代表优先），逐文件读 dictionaryTables 找 dictCode。
    let mut candidates: Vec<String> = Vec::new();
    for arr in groups.values() {
        if let Some(f) = pick(arr) {
            candidates.push(f);
        }
    }
    // 代表都没命中时，回退扫描该 stem 组其余版本（防 isDefault 版本恰好不含该 dict）。
    let mut fallback: Vec<String> = Vec::new();
    for (_, file, _, _) in &entries {
        if !candidates.contains(file) {
            fallback.push(file.clone());
        }
    }
    for f in candidates.iter().chain(fallback.iter()) {
        let doc_ref = cmx_portal::definitions::store::DefRef {
            domain: Some(domain.to_string()),
            application: Some(app.to_string()),
            app: Some(app.to_string()),
            module: Some(module.to_string()),
            file: Some(f.clone()),
            id: None,
            kind: None,
        };
        let doc = match cmx_portal::definitions::store::get_definition(&doc_ref).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        let hit = doc
            .get("dictionaryTables")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|t| dict_matches(t, dict)))
            .unwrap_or(false);
        if hit {
            dict_file_cache()
                .write()
                .await
                .insert(cache_key, f.clone());
            return Ok(f.clone());
        }
    }
    Err(api_err(&format!(
        "未在 {domain}/{app}/{module} 下找到含字典 {dict} 的 DCT 定义文件"
    )))
}

/// 读定义 + base，解析出指定 dictCode 的 DictView。
async fn resolve_dict(q: &DctQuery) -> Result<DictView> {
    // file 缺失时自动解析：在该 domain/app/module 下扫描含 dictCode 的 DCT 文件。
    // 前端运行时只持有 dictCode + domain/app/module（host 无 file 坐标），故 file 由后端兜底。
    let file = match &q.file {
        Some(f) if !f.is_empty() => f.clone(),
        _ => resolve_dict_file(&q.domain, &q.application, &q.module, &q.dict).await?,
    };
    let doc_ref = cmx_portal::definitions::store::DefRef {
        domain: Some(q.domain.clone()),
        application: Some(q.application.clone()),
        app: Some(q.application.clone()),
        module: Some(q.module.clone()),
        file: Some(file.clone()),
        id: None,
        kind: None,
    };
    let doc = cmx_portal::definitions::store::get_definition(&doc_ref).await?;
    let base = load_base(&doc).await;

    let tables = doc
        .get("dictionaryTables")
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_err("定义缺少 dictionaryTables"))?;

    let t = tables
        .iter()
        .find(|t| dict_matches(t, &q.dict))
        .ok_or_else(|| api_err(&format!("未找到字典 {}", q.dict)))?;

    let dm = t.get("dictMeta").cloned().unwrap_or_else(|| json!({}));
    let table_name = dm
        .get("tableName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| api_err("dictMeta 缺少 tableName"))?
        .to_string();

    // 合并列：own fields + 全部 *FieldSet 引用（与 compile_dct 对齐）。
    let mut columns: Vec<DictColumn> = Vec::new();
    // 合并后的原始字段（带 fieldLength/decimalDigits），供构建校验规范 TableSpec。
    let mut raw_fields: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push = |fields: &Vec<Value>,
                columns: &mut Vec<DictColumn>,
                raw_fields: &mut Vec<Value>,
                seen: &mut std::collections::HashSet<String>| {
        for f in fields {
            let name = match f.get("name").and_then(|v| v.as_str()) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => continue,
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            raw_fields.push(f.clone());
            let caption = f
                .get("caption")
                .and_then(|c| {
                    c.get("zh_CN")
                        .and_then(|v| v.as_str())
                        .or_else(|| c.as_str())
                })
                .unwrap_or(&name)
                .to_string();
            // 录入控件/编辑设置/显示属性/维度类型/字典引用/物理字段：原样透传，
            // 供前端 DCT→列模型转换时派生 cmx-dict-select 录入控件与字典回显。
            let edit = f.get("edit").filter(|v| v.is_object()).cloned();
            let edit_settings = f.get("editSettings").filter(|v| v.is_object()).cloned();
            let display = f.get("display").filter(|v| v.is_object()).cloned();
            columns.push(DictColumn {
                caption,
                data_type: f
                    .get("dataType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("VARCHAR")
                    .to_string(),
                is_pk: f
                    .get("isPrimaryKey")
                    .and_then(|v| v.as_i64())
                    .map(|n| n != 0)
                    .unwrap_or(false),
                nullable: f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true),
                dim_type: f
                    .get("dimType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ref_dict: f
                    .get("refDict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                display_field: f
                    .get("displayField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ref_field: f
                    .get("refField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                physical_field: f
                    .get("physicalField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                edit,
                edit_settings,
                display,
                name,
            });
        }
    };
    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        push(own, &mut columns, &mut raw_fields, &mut seen);
    }
    if let Some(obj) = t.as_object() {
        // 固定顺序 + 兜底（与 compile_dct 一致）。
        for key in [
            "baseFieldSet",
            "hierarchyFieldSet",
            "scopeFieldSet",
            "effectiveFieldSet",
            "disableFieldSet",
            "auditFieldSet",
            "systemFieldSet",
        ] {
            if let Some(set_name) = obj.get(key).and_then(|v| v.as_str())
                && let Some(fields) = base_fieldset(&base, set_name)
            {
                push(fields, &mut columns, &mut raw_fields, &mut seen);
            }
        }
    }

    // 主键：优先 isPrimaryKey 标记列；否则 idField（若存在于列中）；再否则 codeField。
    let id_field = dm
        .get("idField")
        .and_then(|v| v.as_str())
        .unwrap_or("id")
        .to_string();
    let code_field = dm
        .get("codeField")
        .and_then(|v| v.as_str())
        .unwrap_or("code")
        .to_string();
    let pk = columns
        .iter()
        .find(|c| c.is_pk)
        .map(|c| c.name.clone())
        .or_else(|| {
            columns
                .iter()
                .find(|c| c.name == id_field)
                .map(|c| c.name.clone())
        })
        .unwrap_or_else(|| code_field.clone());
    // 标记 pk 列（供元数据投影）。
    for c in columns.iter_mut() {
        if c.name == pk {
            c.is_pk = true;
        }
    }

    // 落库前列级校验规范：从合并后的原始字段构建 TableSpec，进程内缓存（键含版本，免失效）。
    let version = doc
        .get("dctMeta")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_u64())
        .or_else(|| doc.get("version").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let spec_key = cmx_biz::validation::spec_key(
        &q.domain, &q.application, &q.module, &file, &table_name, version,
    );
    let spec = match cmx_biz::validation::get_spec(&spec_key) {
        Some(s) => s,
        None => {
            let built = std::sync::Arc::new(cmx_biz::validation::build_table_spec(
                table_name.clone(),
                &pk,
                &raw_fields,
            ));
            cmx_biz::validation::put_spec(spec_key, built.clone());
            built
        }
    };

    Ok(DictView {
        dict_code: dm
            .get("dictCode")
            .and_then(|v| v.as_str())
            .unwrap_or(&q.dict)
            .to_string(),
        dict_name: dm
            .get("dictName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string(),
        self_hierarchy: dm
            .get("selfHierarchy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        parent_field: dm
            .get("parentField")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        label_field: dm
            .get("labelField")
            .and_then(|v| v.as_str())
            .unwrap_or("name")
            .to_string(),
        table_name,
        id_field,
        code_field,
        columns,
        pk,
        spec,
    })
}

/// 从 baseDctMetaRef.file 读 base 字段集定义（无则空对象）。
async fn load_base(doc: &Value) -> Value {
    let file = doc
        .get("baseDctMetaRef")
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str());
    let file = match file {
        Some(f) => f,
        None => return json!({}),
    };
    let base_ref = cmx_portal::definitions::store::DefRef {
        domain: Some("base".into()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    cmx_portal::definitions::store::get_definition(&base_ref)
        .await
        .unwrap_or_else(|_| json!({}))
}

fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}

// ============================================================================
// SQL 辅助：列名白名单校验（防注入）
// ============================================================================

/// 校验标识符是否为该字典的合法列（防 SQL 注入；只允许已知列）。
fn valid_col(view: &DictView, name: &str) -> bool {
    view.columns.iter().any(|c| c.name == name)
}

fn api_err(msg: &str) -> crate::Error {
    cmx_biz::BizError::business(msg.to_string()).into()
}

// ============================================================================
// 主键 ID 生成（后端首次存储铸号）
// ============================================================================

/// pk 列是否为「服务端生成的 bigint 主键」——即需要后端铸号的列。
///
/// 判据：主键列的 dataType 是整数类（BIGINT/INT/…）。字典若以 `code`(VARCHAR) 作 PK
/// （NoID 字典，如 cf_currency），业务 code 本就跨系统稳定，**不铸号**、原样保留。
fn pk_is_generated(view: &DictView) -> bool {
    view.columns
        .iter()
        .find(|c| c.name == view.pk)
        .map(|c| c.data_type.to_uppercase().contains("INT"))
        .unwrap_or(false)
}

/// 判断一个 JSON id 值是否为「前端临时 id」——即需要后端铸真号的占位。
///
/// 前端新增行的 id 可能是：① 缺失/null；② 字符串占位（CmxDataSet 的 `r{rand}`，或本方案约定的
/// `t{n}` 关联键）；③ 客户端 `maxId+1` 小整数（历史做法）。前两类必然是临时值。
/// 对整数：**不能**一律当真号，否则历史前端塞的 `maxId+1` 会绕过铸号又撞库——故整数一律视为需重铸，
/// 由 `remap` 用生成的真号替换，同时把旧值登记进映射供子行 parent_id 重指向。
fn is_temp_id(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()),
        // 纯数字字符串 / 数字：交给调用方按「是否服务端生成列」决定，这里只判「明显的临时形态」。
        _ => false,
    }
}

/// 为一批 inserted 行铸号并回填 parent_id 自引用（自分级字典）。
///
/// 返回 `idMap`：前端原始临时 id（字符串化）→ 新铸真 id。用途：
///   ① 把每行的 pk 列替换成真号；
///   ② 同批子行的 parent_id 若指向某个「同批新增父行的旧临时 id」，重指向到父的真号；
///   ③ 回传前端，让其把临时行的 id 换成真号（避免「新建后立即再改」错位）。
///
/// **仅对「临时 id」行铸号**（缺失/null/非纯数字串，见 [`is_temp_id`]）：已带真数字 id 的行原样保留
/// —— 这样 upsert 路径里「重存一条已存在行」不会被误判为新增而生成重复行。
/// 仅当 `pk_is_generated(view)` 为真时调用。纯内存改写，不碰库。
fn mint_ids_for_inserts(
    view: &DictView,
    rows: &mut [serde_json::Map<String, Value>],
) -> serde_json::Map<String, Value> {
    let mut id_map = serde_json::Map::new();
    // 第一遍：为「临时 id」行铸真号，登记 旧临时id→新真id。
    for row in rows.iter_mut() {
        let cur = row.get(&view.pk);
        if !is_temp_id(cur) {
            continue; // 已是真号（编辑/重存已存在行）→ 不重铸。
        }
        let old_key = id_to_key(cur);
        let new_id = cmx_utils::next_pk_id();
        row.insert(view.pk.clone(), json!(new_id));
        if let Some(k) = old_key {
            id_map.insert(k, json!(new_id));
        }
    }
    // 第二遍：自分级 parent_id 重指向（子行 parent_id == 某父行旧临时 id → 换成父的真号）。
    // 父指向「已存在的真号父行」时 parent_id 不在 id_map 中，原样保留。
    if let Some(pf) = &view.parent_field {
        for row in rows.iter_mut() {
            if let Some(pv) = row.get(pf).cloned()
                && let Some(k) = id_to_key(Some(&pv))
                && let Some(real) = id_map.get(&k)
            {
                row.insert(pf.clone(), real.clone());
            }
        }
    }
    id_map
}

/// id 值 → 稳定字符串键（数字/字符串统一）。null/空 → None。
fn id_to_key(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

// ============================================================================
// 1) GET /api/dct/meta —— 字典显示元数据
// ============================================================================

pub async fn dct_meta(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    _headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    debug!("{:<12} - dct_meta {}/{}", "HANDLER", q.module, q.dict);
    let view = resolve_dict(&q).await?;
    let cols: Vec<Value> = view
        .columns
        .iter()
        .map(|c| {
            let mut obj = json!({
                "name": c.name,
                "caption": c.caption,
                "dataType": c.data_type,
                "isPrimaryKey": c.is_pk,
                "nullable": c.nullable,
            });
            // 维度类型/字典引用/物理字段/录入控件/编辑设置/显示属性：有值才输出，
            // 供前端 DCT→列模型转换时派生 cmx-dict-select 控件与字典外键回显。
            if !c.dim_type.is_empty() {
                obj["dimType"] = Value::String(c.dim_type.clone());
            }
            if !c.ref_dict.is_empty() {
                obj["refDict"] = Value::String(c.ref_dict.clone());
            }
            if !c.display_field.is_empty() {
                obj["displayField"] = Value::String(c.display_field.clone());
            }
            if !c.ref_field.is_empty() {
                obj["refField"] = Value::String(c.ref_field.clone());
            }
            if !c.physical_field.is_empty() {
                obj["physicalField"] = Value::String(c.physical_field.clone());
            }
            if let Some(edit) = &c.edit {
                obj["edit"] = edit.clone();
            }
            if let Some(es) = &c.edit_settings {
                obj["editSettings"] = es.clone();
            }
            if let Some(d) = &c.display {
                obj["display"] = d.clone();
            }
            obj
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({
        "dictCode": view.dict_code,
        "dictName": view.dict_name,
        "tableName": view.table_name,
        "idField": view.id_field,
        "codeField": view.code_field,
        "labelField": view.label_field,
        "parentField": view.parent_field,
        "selfHierarchy": view.self_hierarchy,
        "pk": view.pk,
        "columns": cols,
    }))))
}

// ============================================================================
// 2) POST /api/dct/data/search —— 装载字典数据
// ============================================================================

/// search 请求体。
#[derive(Debug, Deserialize, Default)]
pub struct DctSearchBody {
    /// 自分级：按 parentField 过滤（None=不限；显式 null 表示根级）。
    #[serde(default)]
    pub parent_id: Option<Value>,
    /// 是否传了 parent_id 键（区分「不过滤」与「过滤 null 根级」）。
    #[serde(skip)]
    pub _has_parent: bool,
    /// 简单等值过滤：{col: value}。
    #[serde(default)]
    pub filters: Option<serde_json::Map<String, Value>>,
    /// 关键字（对 code/label 模糊）。
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

/// 由 view + 请求 body 构造 (data_sql, count_sql, params)。data/search 与 zmc-msgpack 端点共用。
fn build_search_sql(view: &DictView, raw: &Value) -> (String, String, Vec<Value>) {
    let col_list = view
        .columns
        .iter()
        .map(|c| format!("\"{}\"", c.name))
        .collect::<Vec<_>>()
        .join(", ");

    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    let mut n = 0usize;

    // parentId 过滤（自分级 children）：仅当定义有 parentField 且请求带 parentId 键。
    if let Some(pf) = &view.parent_field {
        if let Some(pv) = raw.get("parentId") {
            if pv.is_null() {
                wheres.push(format!("\"{}\" IS NULL", pf));
            } else {
                n += 1;
                wheres.push(format!("\"{}\" = ${}", pf, n));
                params.push(pv.clone());
            }
        }
    }

    // filters: {col: value}（列白名单校验）。
    if let Some(filters) = raw.get("filters").and_then(|v| v.as_object()) {
        for (k, v) in filters {
            if !valid_col(view, k) {
                continue;
            }
            if v.is_null() {
                wheres.push(format!("\"{}\" IS NULL", k));
            } else {
                n += 1;
                wheres.push(format!("\"{}\" = ${}", k, n));
                params.push(v.clone());
            }
        }
    }

    // q: 对 code/label 模糊。
    if let Some(kw) = raw.get("q").and_then(|v| v.as_str()) {
        let kw = kw.trim();
        if !kw.is_empty() {
            let c = &view.code_field;
            let l = &view.label_field;
            if valid_col(view, c) && valid_col(view, l) {
                n += 1;
                let p = n;
                wheres.push(format!(
                    "(\"{}\" ILIKE ${} OR \"{}\" ILIKE ${})",
                    c, p, l, p
                ));
                params.push(Value::String(format!("%{}%", kw)));
            }
        }
    }

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", wheres.join(" AND "))
    };

    // 排序：sort_no（若有）→ pk。
    let order = if valid_col(view, "sort_no") {
        format!(" ORDER BY \"sort_no\", \"{}\"", view.pk)
    } else {
        format!(" ORDER BY \"{}\"", view.pk)
    };

    let page = raw.get("page").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
    let page_size = raw
        .get("pageSize")
        .and_then(|v| v.as_i64())
        .unwrap_or(500)
        .clamp(1, 5000);
    let offset = (page - 1) * page_size;

    let data_sql = format!(
        "SELECT {} FROM \"{}\"{}{} LIMIT {} OFFSET {}",
        col_list, view.table_name, where_sql, order, page_size, offset
    );
    let count_sql = format!(
        "SELECT COUNT(*) AS cnt FROM \"{}\"{}",
        view.table_name, where_sql
    );
    (data_sql, count_sql, params)
}

/// JSON 值 → DataValue（zmc 查询参数绑定用；zmc 路径不走 JSON 自动 coerce，需显式）。
fn json_to_datavalue(v: &Value) -> cmx_core::model::cell::DataValue {
    use cmx_core::model::cell::DataValue;
    match v {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::Bool(*b),
        Value::Number(x) => {
            if let Some(i) = x.as_i64() {
                DataValue::Int(i)
            } else if let Some(f) = x.as_f64() {
                DataValue::Float(f)
            } else {
                DataValue::Null
            }
        }
        Value::String(s) => DataValue::String(s.clone()),
        other => DataValue::String(other.to_string()),
    }
}

pub async fn dct_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id(&headers).await;
    let view = resolve_dict(&q).await?;
    let raw = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    debug!(
        "{:<12} - dct_search {} table={}",
        "HANDLER", q.dict, view.table_name
    );

    let (sql, count_sql, params) = build_search_sql(&view, &raw);

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_json(
            &db_id,
            None,
            &sql,
            Value::Array(params.clone()),
            &view.dict_code,
        )
        .await
        .map_err(|e| api_err(&format!("字典查询失败: {e}")))?;
    let total_ds = mm
        .query_sql_with_json(&db_id, None, &count_sql, Value::Array(params), "cnt")
        .await
        .map_err(|e| api_err(&format!("字典计数失败: {e}")))?;

    // DataSet → rows JSON。
    let rows_val = serde_json::to_value(&ds).map_err(|e| api_err(&format!("序列化失败: {e}")))?;
    let rows = rows_val.get("rows").cloned().unwrap_or_else(|| json!([]));
    let total = serde_json::to_value(&total_ds)
        .ok()
        .and_then(|v| {
            v.get("rows")
                .and_then(|r| r.get(0))
                .and_then(|r0| r0.get("cnt"))
                .cloned()
        })
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let page = raw.get("page").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
    let page_size = raw
        .get("pageSize")
        .and_then(|v| v.as_i64())
        .unwrap_or(500)
        .clamp(1, 5000);
    Ok(Json(ApiResp::ok(json!({
        "rows": rows,
        "total": total,
        "page": page,
        "pageSize": page_size,
    }))))
}

// ============================================================================
// 2b) GET|POST /api/dct/data/tokio-zmc-msgpack —— 零拷贝装载：tokio-postgres + ZmcDataSet
//     + 列式 msgpack 二进制出口（对标 doc 的 tokio-zmc-msgpack）。
// ============================================================================

pub async fn dct_search_zmc_msgpack(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let db_id = resolve_db_id(&headers).await;
    let view = resolve_dict(&q).await?;
    let raw = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    debug!(
        "{:<12} - dct zmc-msgpack {} table={}",
        "HANDLER", q.dict, view.table_name
    );

    let (sql, _count_sql, params) = build_search_sql(&view, &raw);
    let dv_params: Vec<cmx_core::model::cell::DataValue> =
        params.iter().map(json_to_datavalue).collect();

    let mm = get_default_pg_db_manager();
    // 零拷贝：ZmcDataSet 持有原始 tokio-postgres Row，惰性列式二进制编码。
    let zmc = mm
        .query_sql_zmc_with_datavalues(&db_id, &sql, dv_params, &view.dict_code)
        .await
        .map_err(|e| api_err(&format!("字典零拷贝查询失败: {e}")))?;
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);

    let envelope = encode_envelope_ok(&buf);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-msgpack")],
        envelope,
    )
        .into_response())
}

/// 成功信封的 msgpack 字节：`{code:0, msg:"success", data:<列式包字节>}`（对标 doc）。
fn encode_envelope_ok(data_msgpack: &[u8]) -> Vec<u8> {
    use rmp::encode as mp;
    let mut buf = Vec::with_capacity(data_msgpack.len() + 32);
    mp::write_map_len(&mut buf, 3).unwrap();
    mp::write_str(&mut buf, "code").unwrap();
    mp::write_uint(&mut buf, 0).unwrap();
    mp::write_str(&mut buf, "msg").unwrap();
    mp::write_str(&mut buf, "success").unwrap();
    mp::write_str(&mut buf, "data").unwrap();
    buf.extend_from_slice(data_msgpack);
    buf
}

// ============================================================================
// 3) POST /api/dct/entries —— 回存（upsert，merge 语义）
// ============================================================================

pub async fn dct_upsert(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id(&headers).await;
    let view = resolve_dict(&q).await?;
    debug!(
        "{:<12} - dct_upsert {} table={}",
        "HANDLER", q.dict, view.table_name
    );

    // body：数组或单对象。
    let items: Vec<Value> = match body {
        Value::Array(a) => a,
        v => vec![v],
    };
    // 取出可改写的行对象。
    let mut rows: Vec<serde_json::Map<String, Value>> = items
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();

    // 主键为服务端生成的 bigint 列时：为「临时 id」行铸真号 + 回填自分级 parent_id。
    // 回传 idMap 供前端把临时行 id 换成真号。NoID(code PK)字典 pk_is_generated=false，跳过铸号。
    let id_map = if pk_is_generated(&view) {
        mint_ids_for_inserts(&view, &mut rows)
    } else {
        serde_json::Map::new()
    };

    // 落库前列级校验：类型/长度/精度/非空（NOT NULL 跳过服务端 backfill 列）。一次回报全部。
    let vopts = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: true,
    };
    let mut violations = Vec::new();
    for (i, obj) in rows.iter().enumerate() {
        violations.extend(cmx_biz::validation::validate_insert_row(
            &view.spec,
            obj,
            Some(i),
            &vopts,
        ));
    }
    if !violations.is_empty() {
        return Ok(Json(validation_fail_resp(&violations)));
    }

    let mm = get_default_pg_db_manager();
    let mut affected = 0u64;
    for obj in &rows {
        if let Some((sql, params)) = build_upsert_sql(&view, obj) {
            let n = mm
                .execute_sql_with_json(&db_id, None, &sql, Value::Array(params))
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
            affected += n;
        }
    }

    Ok(Json(ApiResp::ok(
        json!({ "count": affected, "idMap": id_map }),
    )))
}

/// 构造校验失败响应：`{code:422, msg, data:{violations:[...]}}`（结构化，前端逐行逐列高亮）。
fn validation_fail_resp(violations: &[cmx_biz::errcode::Violation]) -> ApiResp<Value> {
    ApiResp::fail_with_data(
        422,
        format!("数据校验未通过（{} 处）", violations.len()),
        json!({ "violations": violations }),
    )
}

/// DB 原始错误 → 已翻译的优雅错误（稳定错误码 + 中文），不再暴露 PG 英文原文。
fn api_err_db(raw: &str) -> crate::Error {
    cmx_biz::BizError::from_db_error(raw).into()
}

/// 校验一个 changeset 桶（inserted 整行 + updated 部分字段）。有违规则返回校验失败响应，否则 None。
fn dct_validate_bucket(view: &DictView, bucket: &Value) -> Option<ApiResp<Value>> {
    let vopts_insert = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: true,
    };
    let vopts_update = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: false,
    };
    let mut violations = Vec::new();

    if let Some(ins) = bucket.get("inserted").and_then(|v| v.as_array()) {
        for (i, row) in ins.iter().enumerate() {
            if let Some(obj) = row_fields(row) {
                violations.extend(cmx_biz::validation::validate_insert_row(
                    &view.spec,
                    &obj,
                    Some(i),
                    &vopts_insert,
                ));
            }
        }
    }
    if let Some(ups) = bucket.get("updated").and_then(|v| v.as_array()) {
        for (i, row) in ups.iter().enumerate() {
            if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
                violations.extend(cmx_biz::validation::validate_update_fields(
                    &view.spec,
                    fields,
                    Some(i),
                    &vopts_update,
                ));
            }
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(validation_fail_resp(&violations))
    }
}

/// 服务端始终托管、拒绝客户端绑定的只读列（避免 timestamptz 字符串序列化失败 + 防篡改）。
/// 这些列由 build_upsert_sql 的 backfill 用 now()/CURRENT_DATE 强填。
fn is_server_managed_col(name: &str) -> bool {
    matches!(name, "create_time" | "update_time")
}

/// 服务端会 backfill 的列——校验 NOT NULL 时跳过（row 未提供时服务端强填），但**用户提供了值仍校验**。
/// 与 build_upsert_sql 的 backfill 表一致。
const SERVER_FILLED_COLS: &[&str] = &[
    "create_by",
    "update_by",
    "sort_no",
    "status",
    "is_system",
    "is_leaf",
    "level_no",
    "effective_from",
    "full_path",
    "delete_flag",
];

/// 服务端**始终替换值**的列——完全跳过值校验（id 铸号、时间戳 backfill）。
const SERVER_REPLACED_COLS: &[&str] = &["id", "create_time", "update_time"];

/// 构造单行 upsert 的 (sql, params)。列白名单 + 服务端强填 NOT NULL 常见列。
/// dct_upsert 与 dct_save 的 inserted/updated 共用。
fn build_upsert_sql(
    view: &DictView,
    obj: &serde_json::Map<String, Value>,
) -> Option<(String, Vec<Value>)> {
    // 跳过非法列 + 服务端托管列（create_time/update_time 由 backfill 用 now() 填，不接受客户端值）。
    let cols: Vec<&String> = obj
        .keys()
        .filter(|k| valid_col(view, k) && !is_server_managed_col(k))
        .collect();
    if cols.is_empty() {
        return None;
    }
    let mut params: Vec<Value> = Vec::new();
    let mut col_names: Vec<String> = Vec::new();
    let mut placeholders: Vec<String> = Vec::new();
    let mut updates: Vec<String> = Vec::new();
    let mut i = 0usize;
    for c in &cols {
        col_names.push(format!("\"{}\"", c));
        // null 值用 SQL NULL 字面量，不占参数位 —— tokio-postgres 无法为「裸 NULL 参数」推断列
        // 类型（bigint 等），会报 "error serializing parameter"。用字面量让 PG 按列类型取 NULL。
        // 典型场景：自分级字典根级新建行 parent_id=null。
        if obj[*c].is_null() {
            placeholders.push("NULL".to_string());
        } else {
            i += 1;
            placeholders.push(format!("${}", i));
            params.push(obj[*c].clone());
        }
        if **c != view.pk {
            updates.push(format!("\"{}\" = EXCLUDED.\"{}\"", c, c));
        }
    }
    // 服务端强填 NOT NULL 无默认值的常见列（客户端未给时）：审计时间 + 状态/排序/系统标识 +
    // 自分级派生列。避免新建行因缺列被 PG 拒绝（db error）。用 SQL 字面量，不占参数位。
    let provided: std::collections::HashSet<&str> = cols.iter().map(|c| c.as_str()).collect();
    let backfill: &[(&str, &str, bool)] = &[
        ("create_time", "now()", false),
        ("update_time", "now()", true),
        ("sort_no", "0", false),
        ("status", "1", false),
        ("is_system", "0", false),
        ("is_leaf", "1", false),
        ("level_no", "1", false),
        ("effective_from", "CURRENT_DATE", false),
    ];
    for (name, lit, on_update) in backfill {
        if valid_col(view, name) && !provided.contains(name) {
            col_names.push(format!("\"{}\"", name));
            placeholders.push(lit.to_string());
            if *on_update {
                updates.push(format!("\"{}\" = {}", name, lit));
            }
        }
    }
    // full_path 缺失时用 code 值兜底（自分级根级；深层路径前端算）。复用 code 的参数值再绑一次。
    if valid_col(view, "full_path")
        && !provided.contains("full_path")
        && let Some(code_v) = obj.get(&view.code_field)
    {
        i += 1;
        col_names.push("\"full_path\"".to_string());
        placeholders.push(format!("${}", i));
        params.push(code_v.clone());
    }
    let update_clause = if updates.is_empty() {
        "NOTHING".to_string()
    } else {
        format!("UPDATE SET {}", updates.join(", "))
    };
    let sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES ({}) ON CONFLICT (\"{}\") DO {}",
        view.table_name,
        col_names.join(", "),
        placeholders.join(", "),
        view.pk,
        update_clause
    );
    Some((sql, params))
}

// ============================================================================
// 4) DELETE /api/dct/entries/{id} —— 删除一行
// ============================================================================

pub async fn dct_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id(&headers).await;
    let view = resolve_dict(&q).await?;
    debug!("{:<12} - dct_delete {} id={}", "HANDLER", q.dict, id);

    let sql = format!(
        "DELETE FROM \"{}\" WHERE \"{}\" = $1",
        view.table_name, view.pk
    );
    // pk 是整数还是字符串：按 pk 列类型决定 JSON 参数（execute_sql_with_json 按值类型绑定）。
    let pk_is_int = view
        .columns
        .iter()
        .find(|c| c.name == view.pk)
        .map(|c| {
            let dt = c.data_type.to_uppercase();
            dt.contains("INT")
        })
        .unwrap_or(false);
    let param = if pk_is_int {
        id.parse::<i64>()
            .map(|n| json!(n))
            .unwrap_or_else(|_| json!(id))
    } else {
        json!(id)
    };

    let mm = get_default_pg_db_manager();
    let n = mm
        .execute_sql_with_json(&db_id, None, &sql, json!([param]))
        .await
        .map_err(|e| api_err_db(&e.to_string()))?;

    Ok(Json(ApiResp::ok(json!({ "ok": n > 0, "deleted": n }))))
}

// ============================================================================
// 5) POST /api/dct/save —— 基于 changeset 的回存（对标 doc 的 ChangeSetCollector/DocSaver）。
//     body: { saveMode:"merge", changes: { <tableName|dict>: { inserted:[{id,fields}],
//             updated:[{id,fields,baseline}], deleted:[ids] } } }
//     事务内执行；updated 带 update_time baseline 做乐观锁（冲突→409）。
//     返回 { ok, mode, affected, updatedAt:[{id,updateTime}] }。
// ============================================================================

pub async fn dct_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let db_id = resolve_db_id(&headers).await;
    let view = resolve_dict(&q).await?;
    let save_mode = body
        .get("saveMode")
        .and_then(|v| v.as_str())
        .unwrap_or("merge")
        .to_string();
    debug!(
        "{:<12} - dct_save {} table={} mode={}",
        "HANDLER", q.dict, view.table_name, save_mode
    );

    // changes：按 path 分桶。字典是单表，只认与本 dict 的 tableName/dictCode 匹配的那个桶
    // （前端 ChangeSetCollector 的 path 是 root dataset id = dictCode 或 tableName）。
    let changes = body.get("changes").and_then(|v| v.as_object());
    let bucket = changes.and_then(|m| {
        m.get(&view.dict_code)
            .or_else(|| m.get(&view.table_name))
            // 单桶时直接取第一个（前端 root path 可能用别名）
            .or_else(|| m.values().next())
    });
    let bucket = match bucket {
        Some(b) => b,
        None => {
            return Ok(Json(ApiResp::ok(
                json!({ "ok": true, "mode": save_mode, "affected": 0, "updatedAt": [] }),
            ))
            .into_response());
        }
    };

    // 落库前列级校验（开事务前，一次回报全部）。inserted 走整行校验（含 NOT NULL，跳过
    // 服务端 backfill 列）；updated 只校验其 fields（不做整表 NOT NULL）。
    if let Some(resp) = dct_validate_bucket(&view, bucket) {
        return Ok(Json(resp).into_response());
    }

    let mm = get_default_pg_db_manager();
    let tx = mm.get_transaction_context();
    let txn_id = tx
        .begin(&db_id)
        .await
        .map_err(|e| api_err(&format!("开启事务失败: {e}")))?;

    let result = dct_save_apply(mm, &db_id, &txn_id, &view, bucket).await;

    match result {
        Ok((affected, updated_at, conflict, id_map)) => {
            if conflict {
                let _ = tx.rollback(&txn_id).await;
                // 乐观锁冲突：返回 409（对标 doc，前端识别 conflict 提示刷新）。
                return Ok((
                    axum::http::StatusCode::CONFLICT,
                    Json(json!({ "code": 409, "msg": "字典项已被他人修改，请刷新后重试" })),
                )
                    .into_response());
            }
            tx.commit(&txn_id)
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(Json(ApiResp::ok(json!({
                "ok": true,
                "mode": save_mode,
                "affected": affected,
                "updatedAt": updated_at,
                "idMap": id_map,
            })))
            .into_response())
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            Err(e)
        }
    }
}

/// 在事务内应用 changeset 的一个桶。返回 (affected, updatedAt, conflict, idMap)。
/// idMap：inserted 行的 临时id→新铸真id（供前端回填），NoID 字典为空。
async fn dct_save_apply(
    mm: &cmx_database_pg::DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, Vec<Value>, bool, serde_json::Map<String, Value>)> {
    let mut affected = 0u64;
    let mut updated_at: Vec<Value> = Vec::new();
    let mut id_map = serde_json::Map::new();

    // deleted：按 pk 删。
    if let Some(dels) = bucket.get("deleted").and_then(|v| v.as_array()) {
        for id in dels {
            let sql = format!(
                "DELETE FROM \"{}\" WHERE \"{}\" = $1",
                view.table_name, view.pk
            );
            let n = mm
                .execute_sql_with_json(db_id, Some(txn_id), &sql, json!([id]))
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
            affected += n;
        }
    }

    // inserted：先铺平成可改写行对象 → 服务端生成列则铸号 + 回填 parent_id → 整行 upsert。
    if let Some(ins) = bucket.get("inserted").and_then(|v| v.as_array()) {
        let mut rows: Vec<serde_json::Map<String, Value>> =
            ins.iter().filter_map(row_fields).collect();
        if pk_is_generated(view) {
            id_map = mint_ids_for_inserts(view, &mut rows);
        }
        for o in &rows {
            if let Some((sql, params)) = build_upsert_sql(view, o) {
                let n = mm
                    .execute_sql_with_json(db_id, Some(txn_id), &sql, Value::Array(params))
                    .await
                    .map_err(|e| api_err_db(&e.to_string()))?;
                affected += n;
            }
        }
    }

    // updated：带乐观锁基线（baseline=装载时 update_time）。有 baseline 且表有 update_time 列时，
    // UPDATE ... WHERE pk=$ AND update_time=baseline；影响 0 行 = 冲突。
    if let Some(ups) = bucket.get("updated").and_then(|v| v.as_array()) {
        for row in ups {
            let id = match row.get("id") {
                Some(v) if !v.is_null() => v.clone(),
                _ => continue,
            };
            let fields = match row.get("fields").and_then(|v| v.as_object()) {
                Some(f) if !f.is_empty() => f,
                _ => continue,
            };
            // 只更新白名单列（排除 pk 自身 + 服务端托管的时间列）。
            let mut set_parts: Vec<String> = Vec::new();
            let mut params: Vec<Value> = Vec::new();
            let mut i = 0usize;
            for (k, v) in fields {
                if !valid_col(view, k) || k == &view.pk || is_server_managed_col(k) {
                    continue;
                }
                i += 1;
                set_parts.push(format!("\"{}\" = ${}", k, i));
                params.push(v.clone());
            }
            if set_parts.is_empty() {
                continue;
            }
            // update_time 服务端刷新。
            if valid_col(view, "update_time") {
                set_parts.push("\"update_time\" = now()".to_string());
            }
            // pk 参数。
            i += 1;
            let pk_ph = i;
            params.push(id.clone());
            // 乐观锁：baseline 存在 + 有 update_time 列 → 加 AND update_time = baseline。
            let baseline = row.get("baseline").filter(|b| !b.is_null()).cloned();
            let lock_clause = if valid_col(view, "update_time") && baseline.is_some() {
                i += 1;
                params.push(baseline.unwrap());
                format!(" AND \"update_time\" = ${}", i)
            } else {
                String::new()
            };
            let sql = format!(
                "UPDATE \"{}\" SET {} WHERE \"{}\" = ${}{}",
                view.table_name,
                set_parts.join(", "),
                view.pk,
                pk_ph,
                lock_clause
            );
            let n = mm
                .execute_sql_with_json(db_id, Some(txn_id), &sql, Value::Array(params))
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
            if n == 0 && !lock_clause.is_empty() {
                // 乐观锁冲突（baseline 不匹配）。
                return Ok((affected, updated_at, true, id_map));
            }
            affected += n;
            // 回传新 update_time 供前端刷新基线。
            if valid_col(view, "update_time") {
                let q = format!(
                    "SELECT \"update_time\" AS ut FROM \"{}\" WHERE \"{}\" = $1",
                    view.table_name, view.pk
                );
                if let Ok(ds) = mm
                    .query_sql_with_json(db_id, Some(txn_id), &q, json!([id]), "ut")
                    .await
                    && let Ok(v) = serde_json::to_value(&ds)
                    && let Some(ut) = v
                        .get("rows")
                        .and_then(|r| r.get(0))
                        .and_then(|r0| r0.get("ut"))
                        .cloned()
                {
                    updated_at.push(json!({ "id": id, "updateTime": ut }));
                }
            }
        }
    }

    Ok((affected, updated_at, false, id_map))
}

/// changeset 行取 fields：兼容 {id,fields:{...}} 与裸 {...}（含 id）两种形态。
fn row_fields(row: &Value) -> Option<serde_json::Map<String, Value>> {
    if let Some(f) = row.get("fields").and_then(|v| v.as_object()) {
        let mut m = f.clone();
        // 把 id 并进去（inserted 的 id 是业务主键值时需要；前端合成 id 则被白名单过滤）。
        if let Some(idv) = row.get("id")
            && !m.contains_key("id")
        {
            m.insert("id".into(), idv.clone());
        }
        Some(m)
    } else {
        row.as_object().cloned()
    }
}
