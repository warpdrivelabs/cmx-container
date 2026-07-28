//! cmx-dct-model —— 数据字典（DCT）模块的语义中立层（DB-free）。
//!
//! - `DctQuery`：请求坐标 DTO（定位定义文件 + 其中哪张字典表）。
//! - `DictView` / `DictColumn`：从定义 JSON 解析出的字典表强类型视图（由 cmx-dct-store-pg
//!   的 `resolve_dict` 构造，供 SQL 构造 + 元数据投影用）。
//! - 纯逻辑：列白名单校验、主键铸号判定 / 临时 id 识别 / 自分级 parent_id 重指向、
//!   search / upsert 的参数化 SQL 构造。全部无 DB 依赖。

use serde::Deserialize;
use serde_json::{Value, json};

use cmx_core::model::cell::{DataValue, SqlTypeMarker};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

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
    /// 是否返回字段的扁平属性（width/visible/pattern/enumValues/required/intDigits/decimalDigits
    /// 等）到 columns[].extra。默认 false（仅基本列信息，向后兼容）；字典数据维护页等需要
    /// 完整字段属性做编辑/校验/布局的场景传 true。
    /// query key 即 `with_props`（serde 无 rename）。
    #[serde(default)]
    pub with_props: bool,
}

// ============================================================================
// 字典表视图（由 cmx-dct-store-pg::resolve_dict 从定义 JSON 构造）
// ============================================================================

/// 解析出的字典表视图（供 SQL 构造 + 元数据投影用）。
#[derive(Clone)]
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
    /// 字段定义里的扁平属性（width/frozen/visible/required/align/intDigits/decimalDigits/
    /// pattern/enumValues/defaultValue/agg/unique/maxlength/min/max 等），原样收集。
    /// 仅在 `DctQuery.with_props=true` 时填充，避免基本场景的 meta payload 膨胀。
    /// handler 投影时把键铺到列对象顶层（与字段定义 JSON 存储形态一致，前端可直接展开）。
    pub extra: Option<Value>,
}

// ============================================================================
// 元数据投影（供 /dct/meta 投影列对象）
// ============================================================================

/// 把 `DictColumn` 投影成 `/dct/meta` 下发的列对象（JSON）。
///
/// 固定键（必有）：name / caption / dataType / isPrimaryKey / nullable。
/// 条件键（有值才输出）：dimType / refDict / displayField / refField / physicalField /
///   edit / editSettings / display。
/// 扁平属性（with_props=true 时收集到 extra）：铺到列对象顶层，与字段定义 JSON 存储形态一致，
/// 供前端 buildColumnModel 直接展开挂到 CmxColumn（构造器的「完整继承」机制自动收纳未建模键）。
pub fn project_meta_column(c: &DictColumn) -> Value {
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
    // 扁平属性（width/visible/pattern/enumValues/required/intDigits/decimalDigits 等）：
    // with_props=true 时由 store 收集到 extra，此处铺到列对象顶层。
    if let Some(extra) = &c.extra
        && let Some(m) = extra.as_object()
    {
        for (k, v) in m {
            obj[k] = v.clone();
        }
    }
    obj
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

/// 按列类型把 JSON 值转成 DataValue（供 execute_sql_with_datavalues 绑定）。
///
/// 核心解决的问题（参见 cmx-sql-execution 技能）：
/// - **NULL 必须带类型**：tokio-postgres 严格类型校验，`None::<String>` 绑 BIGINT/DATE 列
///   会 WrongType。这里按列 `data_type` 派发到对应的 `NullTyped(SqlTypeMarker)`，
///   让绑定层用正确类型的 `None::<T>`。
/// - **整型列字符串数字 coerce**：前端 grid 编辑器回传的值可能是字符串形态数字（如 `"1"`），
///   整型列（INT/BIGINT/TINYINT）下转 `DataValue::Int`，避开 WrongType。
/// - **TIMESTAMP/DATETIME/DATE 列字符串 coerce**：前端回传的乐观锁 baseline（`update_time`）
///   通常是 ISO8601/RFC3339 字符串（如 `"2026-07-24T03:17:42.078808+00:00"`），`json_to_datavalue`
///   只会包成 `DataValue::String`，直接绑到 TIMESTAMP 列会 WrongType。这里按列类型尝试解析
///   为 `DataValue::DateTime` / `DataValue::Date`，对齐 PG 协议层 `PgDateTime`/`NaiveDate`。
pub fn to_dv_by_col(view: &DictView, col_name: &str, v: &Value) -> DataValue {
    let dt = view
        .columns
        .iter()
        .find(|c| c.name == col_name)
        .map(|c| c.data_type.to_uppercase())
        .unwrap_or_default();
    if v.is_null() {
        // 按列类型派发 NullTyped（绑定层据此用 None::<T>）
        let marker = sql_type_marker_of(&dt);
        return DataValue::NullTyped(marker);
    }
    // 非 null：整型列的字符串数字转 Int
    if dt.contains("INT") {
        if let Some(s) = v.as_str()
            && let Ok(n) = s.parse::<i64>()
        {
            return DataValue::Int(n);
        }
        if let Some(n) = v.as_i64() {
            return DataValue::Int(n);
        }
    }
    // 时间/日期列的字符串 coerce：前端 update_time baseline / 日期字段常以字符串回传
    // （如 "2026-07-24T03:17:42.078808+00:00"、"2026-07-24"）。解析失败仍回退到
    // json_to_datavalue（由绑定层报 WrongType，避免静默错误）。
    if (dt.contains("DATETIME") || dt.contains("TIMESTAMP"))
        && let Some(s) = v.as_str()
        && let Some(dv) = parse_datetime_str(s)
    {
        return dv;
    }
    if dt == "DATE"
        && let Some(s) = v.as_str()
        && let Some(dv) = parse_date_str(s)
    {
        return dv;
    }
    // 其余按 JSON 值类型自然映射（Number→Int/Float、String→String、Bool→Bool）
    json_to_datavalue(v)
}

/// 解析时间字符串为 DataValue::DateTime。
///
/// 兼容：
/// - RFC3339（含时区）：`2026-07-24T03:17:42.078808+00:00` / `...Z`（前端 baseline 形态）
/// - Naive datetime（无时区）：`2026-07-24 03:17:42[.fff]`（按 UTC 墙钟解释）
fn parse_datetime_str(s: &str) -> Option<DataValue> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(DataValue::DateTime(dt.with_timezone(&Utc)));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
    {
        return Some(DataValue::DateTime(
            DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
        ));
    }
    None
}

/// 解析日期字符串为 DataValue::Date。
fn parse_date_str(s: &str) -> Option<DataValue> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .map(DataValue::Date)
}

/// 取列的 SqlTypeMarker（从物理 dataType 字符串派发）。
pub fn sql_type_marker_of(dt: &str) -> SqlTypeMarker {
    let dt = dt.to_uppercase();
    if dt.contains("INT") {
        SqlTypeMarker::Int
    } else if dt.contains("DECIMAL") || dt.contains("NUMERIC") {
        SqlTypeMarker::Decimal
    } else if dt.contains("DATETIME") || dt.contains("TIMESTAMP") {
        SqlTypeMarker::Timestamp
    } else if dt == "DATE" {
        SqlTypeMarker::Date
    } else if dt == "BOOLEAN" || dt == "BOOL" {
        SqlTypeMarker::Bool
    } else if dt.contains("UUID") {
        SqlTypeMarker::Uuid
    } else if dt.contains("JSON") {
        SqlTypeMarker::Json
    } else if dt.contains("BYTEA") || dt.contains("BINARY") {
        SqlTypeMarker::Binary
    } else {
        // VARCHAR/TEXT/未知 → Text（None::<String> 兼容）
        SqlTypeMarker::Text
    }
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

/// 从请求 body 解析分页参数。
///
/// page：默认 1，最小 1。page_size：默认 500，范围 [1, 5000]。
/// `build_search_sql`（构造 LIMIT/OFFSET）与 `search`（回传响应）共用，避免重复计算。
pub fn parse_paging(raw: &Value) -> (i64, i64) {
    let page = raw.get("page").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
    let page_size = raw
        .get("pageSize")
        .and_then(|v| v.as_i64())
        .unwrap_or(500)
        .clamp(1, 5000);
    (page, page_size)
}

/// 由 view + 请求 body 构造 (data_sql, count_sql, params)。data/search 与 zmc-msgpack 端点共用。
///
/// params 直接产出 `Vec<DataValue>`（按列名走 [`to_dv_by_col`] 派发），与 save 路径统一：
/// 整型列字符串数字 coerce、TIMESTAMP/DATETIME/DATE 列字符串 coerce、NULL 带类型。
/// 调用方无需再做 `json_to_datavalue` 转换。
pub fn build_search_sql(view: &DictView, raw: &Value) -> (String, String, Vec<DataValue>) {
    let col_list = view
        .columns
        .iter()
        .map(|c| format!("\"{}\"", c.name))
        .collect::<Vec<_>>()
        .join(", ");

    let mut wheres: Vec<String> = Vec::new();
    let mut params: Vec<DataValue> = Vec::new();
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
            params.push(to_dv_by_col(view, pf, pv));
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
                params.push(to_dv_by_col(view, k, v));
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
                // code/label 恒为 VARCHAR，ILIKE 模糊串直接用 String。
                params.push(DataValue::String(format!("%{}%", kw)));
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

    let (page, page_size) = parse_paging(raw);
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
///
/// **不含** `effective_from`：元数据 `dictionaryEffectiveFields` 明确标 `required: true`，
/// 是业务必填字段而非服务端兜底。若放本表，校验层会跳过 NOT NULL 检查，客户端显式传 null 时
/// `build_upsert_sql_dv` 会按 null 走参数绑定而非 backfill，触发数据库 NOT NULL 违反。修复方案
/// 见 [cmx-dct-store-pg#validate_bucket] → `validate_insert_row` 的语义：必填字段必须由客户端提供。
pub const SERVER_FILLED_COLS: &[&str] = &[
    "create_by",
    "update_by",
    "sort_no",
    "status",
    "is_system",
    "is_leaf",
    "level_no",
    "full_path",
    "delete_flag",
];

/// 服务端**始终替换值**的列——完全跳过值校验（id 铸号、时间戳 backfill）。
pub const SERVER_REPLACED_COLS: &[&str] = &["id", "create_time", "update_time"];

/// 构造单行 upsert 的 (sql, params)。列白名单 + 服务端强填 NOT NULL 常见列。
/// DataValue 版（推荐，供 execute_sql_with_datavalues）。
///
/// 与旧 `build_upsert_sql`（已移除）的差异：
/// - **null 用占位符 + NullTyped**（不再用 SQL NULL 字面量跳过参数位）：按列类型派发
///   `NullTyped(SqlTypeMarker)`，让绑定层用正确类型的 `None::<T>`，避开 tokio-postgres
///   对裸 NULL 参数无法推断列类型的问题（参见 cmx-sql-execution 技能）。
/// - **整型列字符串数字 coerce**：`to_dv_by_col` 把 `"1"` 转 `DataValue::Int(1)`。
/// - params 类型为 `Vec<DataValue>`，配合 `execute_sql_with_datavalues` 绑定。
pub fn build_upsert_sql_dv(
    view: &DictView,
    obj: &serde_json::Map<String, Value>,
) -> Option<(String, Vec<DataValue>)> {
    let cols: Vec<&String> = obj
        .keys()
        .filter(|k| valid_col(view, k) && !is_server_managed_col(k))
        .collect();
    if cols.is_empty() {
        return None;
    }
    let mut params: Vec<DataValue> = Vec::new();
    let mut col_names: Vec<String> = Vec::new();
    let mut placeholders: Vec<String> = Vec::new();
    let mut updates: Vec<String> = Vec::new();
    let mut i = 0usize;
    for c in &cols {
        col_names.push(format!("\"{}\"", c));
        // null 也用占位符，配合 NullTyped 参数（按列类型推断 NULL 类型）
        i += 1;
        placeholders.push(format!("${}", i));
        params.push(to_dv_by_col(view, c, &obj[*c]));
        if **c != view.pk {
            updates.push(format!("\"{}\" = EXCLUDED.\"{}\"", c, c));
        }
    }
    // 服务端强填 NOT NULL 无默认值的常见列（客户端未给时）。用 SQL 字面量，不占参数位。
    let provided: std::collections::HashSet<&str> = cols.iter().map(|c| c.as_str()).collect();
    // 服务端 backfill 列（与 build_batch_insert_sql / SERVER_FILLED_COLS 一致）
    //
    // **不含** `effective_from`：元数据 `required: true` 必填，必须由客户端显式提供。
    // 若列入 backfill，客户端传 null 会走参数绑定而非 CURRENT_DATE 兜底，触发数据库 NOT NULL 违反。
    let backfill: &[(&str, &str, bool)] = &[
        ("create_time", "now()", false),
        ("update_time", "now()", true),
        ("sort_no", "0", false),
        ("status", "1", false),
        ("is_system", "0", false),
        ("is_leaf", "1", false),
        ("level_no", "1", false),
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
    // full_path 缺失时用 code 值兜底。复用 code 的 DataValue 再绑一次。
    if valid_col(view, "full_path")
        && !provided.contains("full_path")
        && let Some(code_v) = obj.get(&view.code_field)
    {
        i += 1;
        col_names.push("\"full_path\"".to_string());
        placeholders.push(format!("${}", i));
        params.push(to_dv_by_col(view, &view.code_field, code_v));
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

/// 构造按 pk 删除单行的 SQL：`DELETE FROM "table" WHERE "pk" = $1`。
///
/// `delete` 函数（DELETE /dct/entries/{id}）与 `save_apply` 的 deleted 分支共用。
/// pk 参数绑定（按 pk 列类型，整型列字符串 id 转 Int）由调用方用 to_dv_by_col 构造。
pub fn build_delete_sql(view: &DictView) -> String {
    format!(
        "DELETE FROM \"{}\" WHERE \"{}\" = $1",
        view.table_name, view.pk
    )
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

// ============================================================================
// 子模块：批量导入导出 SQL 构造（DB-free 纯逻辑）
// ============================================================================
mod bulk;
pub use bulk::*;
