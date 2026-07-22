//! 落库前列级校验：列规范 + 进程内缓存 + 校验器。
//!
//! 规范来源 = **定义 JSON**（`dataType`/`fieldLength`/`decimalDigits`/`nullable`），与建表 DDL
//! （`model_center.rs::field_to_column`）**同源同规则**，故「按定义校验」等价于「与真实表列一致」。
//!
//! 规则复刻（与 `field_to_column` 一致）：
//!   - `FieldType::String`  → `length = fieldLength`（VARCHAR(n) 的 n）
//!   - `FieldType::Decimal` → `precision = fieldLength`，`scale = decimalDigits`（NUMERIC(p,s)）
//!   - 其它类型无长度/精度约束
//!
//! 性能：`TableSpec` 按 `坐标@version` 进程内缓存（`OnceLock<Mutex<HashMap>>`，照抄 doc/cache.rs），
//! 版本变即键变、免失效；热路径零查库零读盘。
//!
//! 校验在 **JSON `Value` 层、SQL 绑定前**执行，一次回报全部 [`Violation`]（不遇错即停）。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{Map, Value};

use cmx_core::model::meta::table::FieldType;

use crate::errcode::{CmxErrCode, Violation};

// ============================================================================
// 列规范
// ============================================================================

/// 单列校验规范（`ColumnDefine` 的校验子集 + caption 供提示）。
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    /// DB 列名。
    pub name: String,
    /// 列中文名（caption.zh_CN）。
    pub caption: String,
    /// 归一后的字段类型。
    pub field_type: FieldType,
    /// 原始 dataType 词元（如 TINYINT/INT/BIGINT，用于整数范围判定）。
    pub raw_type: String,
    /// 字符串最大长度（VARCHAR(n) 的 n）；仅 String 类型有值。
    pub length: Option<u32>,
    /// DECIMAL 总精度；仅 Decimal 类型有值。
    pub precision: Option<u32>,
    /// DECIMAL 小数位；仅 Decimal 类型有值。
    pub scale: Option<u32>,
    /// 是否可空。
    pub nullable: bool,
    /// 是否主键。
    pub is_primary_key: bool,
}

/// 一张表的全部列规范，按列名索引。
#[derive(Debug, Clone)]
pub struct TableSpec {
    /// 表名 / 字典 code。
    pub table: String,
    /// 列名 → 规范。
    pub columns: HashMap<String, ColumnSpec>,
    /// 保序列名（用于 NOT NULL 遍历稳定）。
    pub order: Vec<String>,
}

impl TableSpec {
    /// 取某列规范。
    pub fn column(&self, name: &str) -> Option<&ColumnSpec> {
        self.columns.get(name)
    }
}

// ============================================================================
// dataType 词元 → FieldType（复刻 model_center.rs::map_field_type）
// ============================================================================

/// DCT/DOC dataType 词元 → FieldType（大小写不敏感）。与建表侧对齐。
pub fn map_field_type(data_type: &str) -> FieldType {
    match data_type.to_ascii_uppercase().as_str() {
        "VARCHAR" | "CHAR" | "STRING" => FieldType::String,
        "TEXT" | "CLOB" => FieldType::Text,
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" | "LONG" => FieldType::Int,
        "DECIMAL" | "NUMERIC" | "NUMBER" => FieldType::Decimal,
        "FLOAT" | "DOUBLE" | "REAL" => FieldType::Float,
        "DATE" => FieldType::Date,
        "DATETIME" | "TIMESTAMP" => FieldType::DateTime,
        "BOOL" | "BOOLEAN" => FieldType::Bool,
        "JSON" | "JSONB" => FieldType::Json,
        "UUID" => FieldType::Uuid,
        "BINARY" | "BLOB" | "BYTEA" => FieldType::Binary,
        _ => FieldType::String,
    }
}

/// 单个字段对象（定义 JSON） → ColumnSpec。复刻 field_to_column 的长度/精度分配。
/// `id_field` 命中或 `isPrimaryKey` 标记 → 主键（主键强制 not-null）。
pub fn field_to_spec(f: &Value, id_field: &str) -> Option<ColumnSpec> {
    let name = f.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }
    let raw_type = f
        .get("dataType")
        .and_then(|v| v.as_str())
        .unwrap_or("VARCHAR")
        .to_string();
    let ft = map_field_type(&raw_type);
    let nullable = f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
    let field_len = f
        .get("fieldLength")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let dec = f
        .get("decimalDigits")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let is_pk = (!id_field.is_empty() && name == id_field)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_i64())
            .map(|n| n != 0)
            .unwrap_or(false)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    // 与 field_to_column 一致：String 用 length；Decimal 用 precision(=fieldLength)+scale(=decimalDigits)。
    let (length, precision, scale) = match ft {
        FieldType::String => (field_len, None, None),
        FieldType::Decimal => (None, field_len, dec.or(Some(0))),
        _ => (None, None, None),
    };

    let caption = field_caption(f);

    Some(ColumnSpec {
        name,
        caption,
        field_type: ft,
        raw_type,
        length,
        precision,
        scale,
        nullable: if is_pk { false } else { nullable },
        is_primary_key: is_pk,
    })
}

/// 取字段中文标题（caption.zh_CN / caption 字符串 / 列名）。
fn field_caption(f: &Value) -> String {
    match f.get("caption") {
        Some(Value::Object(o)) => o
            .get("zh_CN")
            .or_else(|| o.get("en"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => f
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// 从一组字段对象构建 TableSpec（供 DCT/DOC 组装合并后的列集调用）。
/// `fields` 已是合并去重后的列（own + *FieldSet）；重复列名后者忽略。
pub fn build_table_spec(table: impl Into<String>, id_field: &str, fields: &[Value]) -> TableSpec {
    let table = table.into();
    let mut columns = HashMap::new();
    let mut order = Vec::new();
    for f in fields {
        if let Some(spec) = field_to_spec(f, id_field)
            && !columns.contains_key(&spec.name)
        {
            order.push(spec.name.clone());
            columns.insert(spec.name.clone(), spec);
        }
    }
    TableSpec {
        table,
        columns,
        order,
    }
}

// ============================================================================
// 进程内缓存（照抄 doc/cache.rs 范式，无外部依赖）
// ============================================================================

static SPEC_CACHE: OnceLock<Mutex<HashMap<String, Arc<TableSpec>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Arc<TableSpec>>> {
    SPEC_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 构造缓存键：坐标 + 表 + 版本（版本变即键变，天然免失效）。
pub fn spec_key(
    domain: &str,
    app: &str,
    module: &str,
    file: &str,
    table: &str,
    version: u64,
) -> String {
    format!("{domain}/{app}/{module}/{file}/{table}@{version}")
}

/// 取缓存。
pub fn get_spec(key: &str) -> Option<Arc<TableSpec>> {
    cache().lock().ok()?.get(key).cloned()
}

/// 存缓存。
pub fn put_spec(key: String, spec: Arc<TableSpec>) {
    if let Ok(mut m) = cache().lock() {
        m.insert(key, spec);
    }
}

/// 逐出某键（定义变更后调用）。
pub fn invalidate(key: &str) {
    if let Ok(mut m) = cache().lock() {
        m.remove(key);
    }
}

/// 清空（测试/运维）。
pub fn clear() {
    if let Ok(mut m) = cache().lock() {
        m.clear();
    }
}

// ============================================================================
// 校验器
// ============================================================================

/// 校验选项。
#[derive(Debug, Clone, Default)]
pub struct ValidateOptions<'a> {
    /// 服务端 backfill 列（audit/status/sort_no/full_path 等）——**仅**影响 NOT NULL：
    /// 这些列 row 未提供时不报「非空」（落库时服务端强填）。**但用户若提供了值，仍照常校验类型/长度**。
    pub server_filled: &'a [&'a str],
    /// 服务端**始终替换值**的列（id 铸号、create_time/update_time 时间戳）——这些列的值是前端占位，
    /// 落库时被真值覆盖，故**完全跳过值校验**（如 id="t1" 不当整数类型错报）。也隐含跳过 NOT NULL。
    pub server_replaced: &'a [&'a str],
    /// 是否校验「未知列」（row 里有 spec 不存在的列）。默认 false（宽松，容忍 UI 附加字段）。
    pub check_unknown: bool,
    /// 是否检查 NOT NULL（insert 用 true；update 通常只改部分列，用 false）。
    pub check_not_null: bool,
}

/// 校验一行（insert 语义）：类型 / 长度 / 精度 / 范围 / 日期 / NOT NULL。
///
/// `row_idx`：本行在提交中的索引（用于 Violation 定位）。返回**全部** violations。
pub fn validate_insert_row(
    spec: &TableSpec,
    row: &Map<String, Value>,
    row_idx: Option<usize>,
    opts: &ValidateOptions,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let replaced: HashSet<&str> = opts.server_replaced.iter().copied().collect();

    // 逐个已提供的列校验值。
    for (col, val) in row {
        // 服务端始终替换值的列（id 临时占位、时间戳）跳过值校验——其值落库时被真值覆盖，
        // 校验它反而误报（如 id="t1" 不是整数）。backfill 列（server_filled）不在此列：
        // 用户若提供了值仍要校验类型/长度。
        if replaced.contains(col.as_str()) {
            continue;
        }
        let Some(cs) = spec.column(col) else {
            if opts.check_unknown && !col.starts_with('_') {
                out.push(Violation::new(
                    CmxErrCode::UnknownColumn,
                    &spec.table,
                    Some(col.clone()),
                    None,
                    row_idx,
                    &[("column", col.clone()), ("table", spec.table.clone())],
                ));
            }
            continue;
        };
        validate_value(cs, val, &spec.table, row_idx, &mut out);
    }

    // NOT NULL：遍历 spec 里 非空 + 非主键 + 非 backfill/替换 列，若 row 未提供或显式 null → 报错。
    if opts.check_not_null {
        let filled: HashSet<&str> = opts
            .server_filled
            .iter()
            .chain(opts.server_replaced.iter())
            .copied()
            .collect();
        for col in &spec.order {
            let cs = &spec.columns[col];
            if cs.nullable || cs.is_primary_key || filled.contains(cs.name.as_str()) {
                continue;
            }
            let missing = match row.get(col) {
                None => true,
                Some(Value::Null) => true,
                Some(Value::String(s)) if s.is_empty() => true, // 空串视作未填（业务常见）
                _ => false,
            };
            if missing {
                out.push(Violation::new(
                    CmxErrCode::NotNullViolation,
                    &spec.table,
                    Some(cs.name.clone()),
                    Some(cs.caption.clone()),
                    row_idx,
                    &[("caption", disp(&cs.caption, &cs.name))],
                ));
            }
        }
    }

    out
}

/// 校验一行的部分列（update 语义）：只校验 row 里出现的列，不做 NOT NULL 整表检查。
pub fn validate_update_fields(
    spec: &TableSpec,
    fields: &Map<String, Value>,
    row_idx: Option<usize>,
    opts: &ValidateOptions,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let replaced: HashSet<&str> = opts.server_replaced.iter().copied().collect();
    for (col, val) in fields {
        // 服务端始终替换值的列跳过（同 insert）。
        if replaced.contains(col.as_str()) {
            continue;
        }
        let Some(cs) = spec.column(col) else {
            if opts.check_unknown && !col.starts_with('_') {
                out.push(Violation::new(
                    CmxErrCode::UnknownColumn,
                    &spec.table,
                    Some(col.clone()),
                    None,
                    row_idx,
                    &[("column", col.clone()), ("table", spec.table.clone())],
                ));
            }
            continue;
        };
        // update 时若显式把非空列设为 null，也应拦。
        if !cs.nullable && !cs.is_primary_key && val.is_null() {
            out.push(Violation::new(
                CmxErrCode::NotNullViolation,
                &spec.table,
                Some(cs.name.clone()),
                Some(cs.caption.clone()),
                row_idx,
                &[("caption", disp(&cs.caption, &cs.name))],
            ));
            continue;
        }
        validate_value(cs, val, &spec.table, row_idx, &mut out);
    }
    out
}

/// 单值校验：类型 / 长度 / 精度 / 范围 / 日期。null 值跳过（NOT NULL 另判）。
fn validate_value(
    cs: &ColumnSpec,
    val: &Value,
    table: &str,
    row_idx: Option<usize>,
    out: &mut Vec<Violation>,
) {
    if val.is_null() {
        return;
    }
    let cap = disp(&cs.caption, &cs.name);
    match cs.field_type {
        FieldType::String | FieldType::Text => {
            // 允许数字/布尔被前端当字符串传，统一取其字符串形态度量长度。
            let s = value_as_str(val);
            if let (Some(max), Some(s)) = (cs.length, s.as_ref()) {
                let len = s.chars().count() as u32;
                if len > max {
                    out.push(Violation::new(
                        CmxErrCode::ValueTooLong,
                        table,
                        Some(cs.name.clone()),
                        Some(cs.caption.clone()),
                        row_idx,
                        &[
                            ("caption", cap.clone()),
                            ("max", max.to_string()),
                            ("actual", len.to_string()),
                        ],
                    ));
                }
            }
        }
        FieldType::Int => {
            // 接受 JSON 数字，或纯数字字符串（前端常把 id/数值当字符串传）。
            let n = value_as_i128(val);
            match n {
                Some(n) => {
                    if let Some((lo, hi)) = int_range(&cs.raw_type)
                        && (n < lo || n > hi)
                    {
                        out.push(Violation::new(
                            CmxErrCode::NumericOutOfRange,
                            table,
                            Some(cs.name.clone()),
                            Some(cs.caption.clone()),
                            row_idx,
                            &[
                                ("caption", cap.clone()),
                                ("type", cs.raw_type.clone()),
                                ("actual", n.to_string()),
                            ],
                        ));
                    }
                }
                None => out.push(Violation::new(
                    CmxErrCode::TypeMismatch,
                    table,
                    Some(cs.name.clone()),
                    Some(cs.caption.clone()),
                    row_idx,
                    &[
                        ("caption", cap.clone()),
                        ("type", "整数".into()),
                        ("actual", value_preview(val)),
                    ],
                )),
            }
        }
        FieldType::Float => {
            if value_as_f64(val).is_none() {
                out.push(Violation::new(
                    CmxErrCode::TypeMismatch,
                    table,
                    Some(cs.name.clone()),
                    Some(cs.caption.clone()),
                    row_idx,
                    &[
                        ("caption", cap.clone()),
                        ("type", "小数".into()),
                        ("actual", value_preview(val)),
                    ],
                ));
            }
        }
        FieldType::Decimal => match decimal_digits(val) {
            Some((int_digits, frac_digits)) => {
                if let (Some(prec), Some(scale)) = (cs.precision, cs.scale) {
                    let max_int = prec.saturating_sub(scale);
                    if frac_digits > scale {
                        out.push(Violation::new(
                            CmxErrCode::DecimalScaleExceeded,
                            table,
                            Some(cs.name.clone()),
                            Some(cs.caption.clone()),
                            row_idx,
                            &[
                                ("caption", cap.clone()),
                                ("max", format!("{scale} 位小数")),
                                ("actual", format!("{frac_digits} 位小数")),
                            ],
                        ));
                    }
                    if int_digits > max_int {
                        out.push(Violation::new(
                            CmxErrCode::NumericOutOfRange,
                            table,
                            Some(cs.name.clone()),
                            Some(cs.caption.clone()),
                            row_idx,
                            &[
                                ("caption", cap.clone()),
                                ("type", format!("整数位 ≤ {max_int}")),
                                ("actual", format!("{int_digits} 位整数")),
                            ],
                        ));
                    }
                }
            }
            None => out.push(Violation::new(
                CmxErrCode::TypeMismatch,
                table,
                Some(cs.name.clone()),
                Some(cs.caption.clone()),
                row_idx,
                &[
                    ("caption", cap.clone()),
                    ("type", "数值".into()),
                    ("actual", value_preview(val)),
                ],
            )),
        },
        FieldType::Date => {
            if let Some(s) = val.as_str()
                && chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").is_err()
            {
                out.push(Violation::new(
                    CmxErrCode::InvalidDate,
                    table,
                    Some(cs.name.clone()),
                    Some(cs.caption.clone()),
                    row_idx,
                    &[("caption", cap.clone()), ("actual", value_preview(val))],
                ));
            }
        }
        FieldType::DateTime => {
            if let Some(s) = val.as_str()
                && !parseable_datetime(s.trim())
            {
                out.push(Violation::new(
                    CmxErrCode::InvalidDate,
                    table,
                    Some(cs.name.clone()),
                    Some(cs.caption.clone()),
                    row_idx,
                    &[("caption", cap.clone()), ("actual", value_preview(val))],
                ));
            }
        }
        // Bool/Json/Uuid/Binary/Array/Unknown：暂不强校验（宽松，DB 层兜底）。
        _ => {}
    }
}

// ── 值形态工具 ──────────────────────────────────────────────────────────

fn disp(caption: &str, name: &str) -> String {
    if caption.is_empty() {
        name.to_string()
    } else {
        caption.to_string()
    }
}

/// 取值的字符串形态（用于长度度量）。数字/布尔转成其文本表示。
fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn value_as_i128(v: &Value) -> Option<i128> {
    match v {
        Value::Number(n) => n.as_i64().map(|i| i as i128),
        Value::String(s) => s.trim().parse::<i128>().ok(),
        _ => None,
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// 取数值的（整数位数, 小数位数）。接受 JSON 数字或数字串。非数值返回 None。
fn decimal_digits(v: &Value) -> Option<(u32, u32)> {
    let s = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    let s = s.strip_prefix('-').unwrap_or(&s);
    if s.is_empty() {
        return None;
    }
    // 校验是数字形态
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    // 整数位：去前导 0（但 "0" 记 0 位有效整数——用于 0.xx，int digits 视为 0）。
    let int_trim = int_part.trim_start_matches('0');
    let int_digits = int_trim.len() as u32;
    let frac_digits = frac_part.trim_end_matches('0').len() as u32;
    Some((int_digits, frac_digits))
}

fn value_preview(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > 32 {
        format!("{}…", s.chars().take(32).collect::<String>())
    } else {
        s
    }
}

/// 整数类型词元 → (min, max)。未知按 BIGINT。
fn int_range(raw: &str) -> Option<(i128, i128)> {
    match raw.to_ascii_uppercase().as_str() {
        "TINYINT" => Some((-128, 127)),
        "SMALLINT" => Some((-32_768, 32_767)),
        "INT" | "INTEGER" => Some((i32::MIN as i128, i32::MAX as i128)),
        "BIGINT" | "LONG" => Some((i64::MIN as i128, i64::MAX as i128)),
        _ => None,
    }
}

fn parseable_datetime(s: &str) -> bool {
    if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
        return true;
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if chrono::NaiveDateTime::parse_from_str(s, fmt).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec() -> TableSpec {
        let fields = vec![
            json!({"name":"id","dataType":"BIGINT","isPrimaryKey":1,"caption":{"zh_CN":"主键"}}),
            json!({"name":"code","dataType":"VARCHAR","fieldLength":64,"nullable":false,"caption":{"zh_CN":"编码"}}),
            json!({"name":"name","dataType":"VARCHAR","fieldLength":128,"nullable":false,"caption":{"zh_CN":"名称"}}),
            json!({"name":"sort_no","dataType":"INT","nullable":true,"caption":{"zh_CN":"排序"}}),
            json!({"name":"flag","dataType":"TINYINT","nullable":true,"caption":{"zh_CN":"标志"}}),
            json!({"name":"amount","dataType":"DECIMAL","fieldLength":10,"decimalDigits":2,"nullable":true,"caption":{"zh_CN":"金额"}}),
            json!({"name":"biz_date","dataType":"DATE","nullable":true,"caption":{"zh_CN":"业务日期"}}),
        ];
        build_table_spec("cf_test", "id", &fields)
    }

    fn opts() -> ValidateOptions<'static> {
        ValidateOptions {
            server_filled: &[],
            server_replaced: &[],
            check_unknown: false,
            check_not_null: true,
        }
    }

    #[test]
    fn spec_captures_length_and_precision() {
        let s = spec();
        assert_eq!(s.column("code").unwrap().length, Some(64));
        assert_eq!(s.column("amount").unwrap().precision, Some(10));
        assert_eq!(s.column("amount").unwrap().scale, Some(2));
        assert!(s.column("id").unwrap().is_primary_key);
    }

    #[test]
    fn valid_row_passes() {
        let row = json!({"code":"1001","name":"库存现金","sort_no":1,"amount":"100.50","biz_date":"2026-07-12"})
            .as_object().unwrap().clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(v.is_empty(), "应无违规，实际: {:?}", v);
    }

    #[test]
    fn varchar_too_long() {
        let long = "x".repeat(65);
        let row = json!({"code":long,"name":"n"}).as_object().unwrap().clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "VALUE_TOO_LONG");
        assert!(v[0].message.contains("64"));
        assert!(v[0].message.contains("65"));
    }

    #[test]
    fn not_null_missing() {
        // 缺 name（非空非主键）→ 报 NOT_NULL。
        let row = json!({"code":"1001"}).as_object().unwrap().clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(
            v.iter()
                .any(|x| x.code == "NOT_NULL_VIOLATION" && x.column.as_deref() == Some("name"))
        );
    }

    #[test]
    fn not_null_skips_server_filled() {
        // name 若在 server_filled 里则不报（模拟服务端 backfill）。
        let row = json!({"code":"1001"}).as_object().unwrap().clone();
        let o = ValidateOptions {
            server_filled: &["name"],
            server_replaced: &[],
            check_unknown: false,
            check_not_null: true,
        };
        let v = validate_insert_row(&spec(), &row, Some(0), &o);
        assert!(!v.iter().any(|x| x.column.as_deref() == Some("name")));
    }

    #[test]
    fn int_type_mismatch() {
        let row = json!({"code":"c","name":"n","sort_no":"abc"})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(
            v.iter()
                .any(|x| x.code == "TYPE_MISMATCH" && x.column.as_deref() == Some("sort_no"))
        );
    }

    #[test]
    fn tinyint_out_of_range() {
        let row = json!({"code":"c","name":"n","flag":999})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(
            v.iter()
                .any(|x| x.code == "NUMERIC_OUT_OF_RANGE" && x.column.as_deref() == Some("flag"))
        );
    }

    #[test]
    fn decimal_scale_exceeded() {
        let row = json!({"code":"c","name":"n","amount":"1.234"})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(v.iter().any(|x| x.code == "DECIMAL_SCALE_EXCEEDED"));
    }

    #[test]
    fn decimal_int_part_too_big() {
        // precision=10 scale=2 → 整数位最多 8。给 9 位整数。
        let row = json!({"code":"c","name":"n","amount":"123456789.00"})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(
            v.iter()
                .any(|x| x.code == "NUMERIC_OUT_OF_RANGE" && x.column.as_deref() == Some("amount"))
        );
    }

    #[test]
    fn invalid_date() {
        let row = json!({"code":"c","name":"n","biz_date":"not-a-date"})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(v.iter().any(|x| x.code == "INVALID_DATE"));
    }

    #[test]
    fn collects_all_violations() {
        // 同时超长 + 类型错 → 一次回报 2 条。
        let long = "x".repeat(65);
        let row = json!({"code":long,"name":"n","sort_no":"bad"})
            .as_object()
            .unwrap()
            .clone();
        let v = validate_insert_row(&spec(), &row, Some(0), &opts());
        assert!(v.len() >= 2);
    }

    #[test]
    fn update_fields_no_not_null_full_check() {
        // update 只校验给的列，不做整表 NOT NULL。只给超长 code。
        let long = "x".repeat(65);
        let fields = json!({"code":long}).as_object().unwrap().clone();
        let o = ValidateOptions::default();
        let v = validate_update_fields(&spec(), &fields, Some(0), &o);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].code, "VALUE_TOO_LONG");
    }

    #[test]
    fn cache_roundtrip() {
        clear();
        let key = spec_key("fi", "cmxfico", "gl", "f.json", "cf_test", 1);
        assert!(get_spec(&key).is_none());
        put_spec(key.clone(), Arc::new(spec()));
        assert!(get_spec(&key).is_some());
        invalidate(&key);
        assert!(get_spec(&key).is_none());
    }
}
