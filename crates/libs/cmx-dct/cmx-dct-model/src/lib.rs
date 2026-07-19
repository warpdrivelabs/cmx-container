//! cmx-dct-model —— 数据字典（DCT）模块的语义中立层（DB-free）。
//!
//! - `DctQuery` / `DctSearchBody`：请求 DTO（坐标 + 查询体）。
//! - `DictView` / `DictColumn`：从定义 JSON 解析出的字典表强类型视图（由 cmx-dct-store-pg
//!   的 `resolve_dict` 构造，供 SQL 构造 + 元数据投影用）。
//! - 纯逻辑：列白名单校验、主键铸号判定 / 临时 id 识别 / 自分级 parent_id 重指向、
//!   search / upsert 的参数化 SQL 构造。全部无 DB 依赖。

use serde::Deserialize;
use serde_json::{json, Value};

use cmx_core::model::cell::DataValue;

// ============================================================================
// 请求参数
// ============================================================================

/// `/api/dct/*` 共用坐标：定位定义文件 + 其中哪张字典表。
///
/// `file` 可选：缺失时由 `resolve_dict_file`（cmx-dct-store-pg）在 domain/app/module 下自动
/// 扫描含该 dictCode 的 DCT 文件（优先 isDefault、回退 version 最大）。这样前端运行时只需传
/// domain/app/module/dict 四元（运行时 host 无 file 坐标）。
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

// ============================================================================
// 字典表视图（由 cmx-dct-store-pg::resolve_dict 从定义 JSON 构造）
// ============================================================================

/// 解析出的字典表视图（供 SQL 构造 + 元数据投影用）。
pub struct DictView {
    pub dict_code: String,
    pub dict_name: String,
    pub table_name: String,
    pub id_field: String,
    pub code_field: String,
    pub label_field: String,
    pub parent_field: Option<String>,
    pub self_hierarchy: bool,
    /// 合并后的列（own fields + 全部 *FieldSet 引用），去重保序。
    pub columns: Vec<DictColumn>,
    /// 主键列名（有 id 用 id；无 id 用 code）。
    pub pk: String,
    /// 落库前列级校验规范（进程内缓存，含类型/长度/精度/nullable）。
    pub spec: std::sync::Arc<cmx_biz::validation::TableSpec>,
}

#[derive(Clone)]
pub struct DictColumn {
    pub name: String,
    pub caption: String,
    pub data_type: String,
    pub is_pk: bool,
    pub nullable: bool,
    /// 维度类型（attribute|dimension），供前端列模型分组/排序。
    pub dim_type: String,
    /// 引用字典编码（如 comp_unit）。空 = 非字典列。
    pub ref_dict: String,
    /// 显示字段（字典回显用，如 name）。
    pub display_field: String,
    /// 写回字段（字典选值写回行，如 code/id）。
    pub ref_field: String,
    /// 物理字段名（如 MANDT），空则无。
    pub physical_field: String,
    /// 录入控件配置（原样透传 edit{}）。
    pub edit: Option<Value>,
    /// 编辑设置（原样透传 editSettings{}）。
    pub edit_settings: Option<Value>,
    /// 显示属性（原样透传 display{}，如下沉后的 decimalDigits/format）。
    pub display: Option<Value>,
}

// ============================================================================
// base 字段集读取辅助
// ============================================================================

/// 从 base 定义里取某个字段集的 `fields` 数组。
pub fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}

// ============================================================================
// SQL 辅助：列名白名单校验（防注入）
// ============================================================================

/// 校验标识符是否为该字典的合法列（防 SQL 注入；只允许已知列）。
pub fn valid_col(view: &DictView, name: &str) -> bool {
    view.columns.iter().any(|c| c.name == name)
}

// ============================================================================
// 主键 ID 生成（后端首次存储铸号）
// ============================================================================

/// pk 列是否为「服务端生成的 bigint 主键」——即需要后端铸号的列。
///
/// 判据：主键列的 dataType 是整数类（BIGINT/INT/…）。字典若以 `code`(VARCHAR) 作 PK
/// （NoID 字典，如 cf_currency），业务 code 本就跨系统稳定，**不铸号**、原样保留。
pub fn pk_is_generated(view: &DictView) -> bool {
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
pub fn is_temp_id(v: Option<&Value>) -> bool {
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
pub fn mint_ids_for_inserts(
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
pub fn id_to_key(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

// ============================================================================
// search SQL 构造
// ============================================================================

/// 由 view + 请求 body 构造 (data_sql, count_sql, params)。data/search 与 zmc-msgpack 端点共用。
pub fn build_search_sql(view: &DictView, raw: &Value) -> (String, String, Vec<Value>) {
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
    if let Some(pf) = &view.parent_field
        && let Some(pv) = raw.get("parentId")
    {
        if pv.is_null() {
            wheres.push(format!("\"{}\" IS NULL", pf));
        } else {
            n += 1;
            wheres.push(format!("\"{}\" = ${}", pf, n));
            params.push(pv.clone());
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
pub fn json_to_datavalue(v: &Value) -> DataValue {
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

// ============================================================================
// upsert SQL 构造 + 服务端托管列
// ============================================================================

/// 服务端托管列——upsert 时跳过客户端值（create_time/update_time 由 backfill 用 now() 填）。
pub fn is_server_managed_col(name: &str) -> bool {
    matches!(name, "create_time" | "update_time")
}

/// 服务端会 backfill 的列——校验 NOT NULL 时跳过（row 未提供时服务端强填），但**用户提供了值仍校验**。
/// 与 build_upsert_sql 的 backfill 表一致。
pub const SERVER_FILLED_COLS: &[&str] = &[
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
pub const SERVER_REPLACED_COLS: &[&str] = &["id", "create_time", "update_time"];

/// 构造单行 upsert 的 (sql, params)。列白名单 + 服务端强填 NOT NULL 常见列。
/// dct_upsert 与 dct_save 的 inserted/updated 共用。
pub fn build_upsert_sql(
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

/// changeset 行取 fields：兼容 {id,fields:{...}} 与裸 {...}（含 id）两种形态。
pub fn row_fields(row: &Value) -> Option<serde_json::Map<String, Value>> {
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
