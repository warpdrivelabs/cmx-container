//! DCT/DOC/RPT 定义 JSON → cmx-core TableDefine 编译器。
//!
//! 从 lib.rs 拆出：原"一、编译器"段 + "二、内省辅助"段。

use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, IndexKind, TableDefine};
use serde_json::{Value, json};
use tracing::warn;

use cmx_api_types::{Error, Result};

use cmx_utils::json::base_fieldset;

use crate::{db_err, VARCHAR_DEFAULT_LENGTH};

// ════════════════════════════════════════════════════════════════════════
//  一、编译器：DCT/DOC 定义 JSON → cmx-core TableDefine
// ════════════════════════════════════════════════════════════════════════

/// DCT dataType 词元 → FieldType（大小写不敏感）。
///
/// 兼容前端/迁移脚本中常见的大小写、缩写、领域别名（`CHAR`/`STRING`/`TEXT`/`CLOB`/`NUMBER`
/// 等）。未识别的词元默认走 `String`，避免一处拼写错误炸掉整个模块编译（行为同原实现）。
pub(crate) fn map_field_type(data_type: &str) -> FieldType {
    match data_type.to_ascii_uppercase().as_str() {
        // 字符串家族：所有 VARCHAR 变体都映射为 String，长度在调用方按 VARCHAR_DEFAULT_LENGTH 兜底
        "VARCHAR" | "CHAR" | "STRING" => FieldType::String,
        // 长文本：TEXT/CLOB 通常不设 length
        "TEXT" | "CLOB" => FieldType::Text,
        // 整数家族：PG 内部按是否需 > 2^31 自动选 int/bigint，但 cmx-core 只暴露 Int
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" | "LONG" => FieldType::Int,
        // 精度数：精度/标度在调用方通过 (fieldLength, decimalDigits) 注入
        "DECIMAL" | "NUMERIC" | "NUMBER" => FieldType::Decimal,
        // 浮点（前端不常见，但兼容历史定义）
        "FLOAT" | "DOUBLE" | "REAL" => FieldType::Float,
        // 日期
        "DATE" => FieldType::Date,
        // 时间戳（带时区与否由 PG 端 DDL 决定，cmx-core 不区分）
        "DATETIME" | "TIMESTAMP" => FieldType::DateTime,
        // 布尔
        "BOOL" | "BOOLEAN" => FieldType::Bool,
        // JSON 家族：JSON/JSONB 在 PG 端行为有差异，但 cmx-core 不区分
        "JSON" | "JSONB" => FieldType::Json,
        // UUID
        "UUID" => FieldType::Uuid,
        // 二进制
        "BINARY" | "BLOB" | "BYTEA" => FieldType::Binary,
        // 兜底：未知词元走 String（与原行为一致，避免一处拼写错误炸掉整个模块）
        _ => FieldType::String,
    }
}

/// 取字段中文标题（caption.zh_CN / caption 字符串 / 空）。
///
/// 优先级：`caption.zh_CN` > `caption.en` > `caption` 字符串。caption 缺失或为非字符串/非对象
/// 形态（如 `null`/数字）一律回退空串（与原行为一致），不报错。
fn field_caption(f: &Value) -> String {
    match f.get("caption") {
        // 形态 1：caption 是对象（i18n），优先 zh_CN，其次 en
        Some(Value::Object(o)) => o
            .get("zh_CN")
            .or_else(|| o.get("en"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // 形态 2：caption 是字符串（简写）
        Some(Value::String(s)) => s.clone(),
        // 形态 3：缺失 / null / 数字等 → 空串
        _ => String::new(),
    }
}

/// 表/汇总表标题（caption.zh_CN / caption 字符串 / tableAlias / name）。
///
/// 优先级：`tableAlias` > `caption`（同 field_caption）> `name`。三者全缺时回退到 `fallback`
/// （通常是 `tableName` 短 id，保证总返回非空）。
fn table_caption(t: &Value, fallback: &str) -> String {
    // 1) tableAlias 优先（部分表用它表示"友好名"）
    t.get("tableAlias")
        .and_then(|v| v.as_str())
        // 2) caption：i18n 对象或字符串都接受
        .or_else(|| match t.get("caption") {
            Some(Value::Object(o)) => o
                .get("zh_CN")
                .or_else(|| o.get("en"))
                .and_then(|v| v.as_str()),
            Some(Value::String(s)) => Some(s.as_str()),
            _ => None,
        })
        // 3) name（部分定义把它当显示名用）
        .or_else(|| t.get("name").and_then(|v| v.as_str()))
        // 4) 兜底
        .unwrap_or(fallback)
        .to_string()
}

/// SQL 字符串字面量定界：单引号包裹 + 内部单引号翻倍转义。
fn sql_quote_default(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 识别"高级表达式"形态的字符串默认值（原样透传，不做类型转换/加引号）：
/// - 以单引号开头：调用方已自行定界的字面量/复合表达式（如 `'{}'::jsonb`）
/// - `CURRENT_*` / `NOW()` / `GEN_*` / `UUID_*` / `NEXTVAL(...)`：常见 SQL 函数
/// - 含 `::` 类型转换
/// - 形如 `name(...)` 的函数调用：`(` 前必须是纯 SQL 标识符——挡住 `N/A (备用)` 这类
///   以括号结尾的普通文本（只看"含 ( 且以 ) 结尾"会误判透传导致 DDL 语法错误）
fn is_sql_default_expr(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with('\'') {
        return true;
    }
    let u = t.to_ascii_uppercase();
    if u.starts_with("CURRENT_") || u.starts_with("GEN_") || u.starts_with("UUID_") {
        return true;
    }
    if u == "NOW()" || u.starts_with("NOW(") || u.starts_with("NEXTVAL(") {
        return true;
    }
    if t.contains("::") {
        return true;
    }
    if t.contains('(')
        && t.ends_with(')')
        && let Some(pos) = t.find('(')
    {
        let head = &t[..pos];
        return !head.is_empty()
            && head
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    }
    false
}

/// 字段 JSON 的 `defaultValue`（任意 JSON 值）→ 按字段类型规范化的 SQL 默认值表达式。
///
/// 设计约定：
/// - 产出**最终 SQL 表达式**——DDL 层（cmx-metadata `render_default_value`）对以单引号
///   定界的值原样输出，因此带引号的字面量在本函数内完成定界，避免 DDL 层的内容启发式
///   误判（如 VARCHAR 列默认值 `"0"` 被裸输出成数字导致 PG 报错）；
/// - 数值/布尔按类型转裸字面量；字符串/日期/UUID 转 `'...'` 字面量；JSON 转紧凑 `'...'`
///   文本（PG 对 jsonb 列的字符串字面量默认值可隐式转换，无需显式 `::jsonb`）；
/// - 字符串值命中 [`is_sql_default_expr`]（`now()` / `'{}'::jsonb` / `nextval(...)` 等）时
///   原样透传——高级逃生舱，让定义文件能表达任意 SQL 表达式；
/// - JSON `null` / 与字段类型不匹配的值（如 INT 字段给了 `"abc"`）→ `None`（宽松容错：
///   warn 不中断编译，与本文件非法字段的处理风格一致）。
fn normalize_default_value(raw: &Value, ft: &FieldType) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    let mismatch = |expect: &str| -> Option<String> {
        warn!(value = %raw, expect, "defaultValue 与字段类型不匹配，忽略该默认值");
        None
    };
    match ft {
        // 整数：JSON 整数或整数字符串 → 裸数字字面量。小数一律拒绝（warn 容错）：
        // PG 对整型列的 DEFAULT 1.5 不做 numeric→int 隐式赋值转换，会直接报错
        // 中断整个部署。
        FieldType::Int => match raw {
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(i.to_string())
                } else if let Some(f) = n.as_f64() {
                    // 2.0 / 1e3 等「整值的小数语法表示」转整数；真小数拒绝
                    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
                        Some((f as i64).to_string())
                    } else {
                        mismatch("整数")
                    }
                } else {
                    mismatch("整数")
                }
            }
            Value::String(s) => {
                let t = s.trim();
                if t.parse::<i64>().is_ok() || t.parse::<u64>().is_ok() {
                    Some(t.to_string())
                } else {
                    mismatch("整数")
                }
            }
            _ => mismatch("整数"),
        },
        FieldType::Decimal | FieldType::Float => match raw {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) => {
                let t = s.trim();
                if t.parse::<f64>().is_ok() {
                    Some(t.to_string())
                } else {
                    mismatch("数值")
                }
            }
            _ => mismatch("数值"),
        },
        FieldType::Bool => match raw {
            Value::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.to_string()),
            Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Some("TRUE".to_string()),
                "false" | "0" | "no" => Some("FALSE".to_string()),
                _ => mismatch("布尔"),
            },
            _ => mismatch("布尔"),
        },
        // 字符串/长文本/日期/时间戳/UUID：字面量加引号定界；表达式（now() 等）原样
        FieldType::String | FieldType::Text | FieldType::Date | FieldType::DateTime | FieldType::Uuid => match raw {
            Value::String(s) => {
                let t = s.trim();
                if is_sql_default_expr(t) {
                    Some(t.to_string())
                } else {
                    Some(sql_quote_default(t))
                }
            }
            // 数字/布尔给到字符串型字段：按字符串内容定界（如 status VARCHAR DEFAULT '1'）
            Value::Number(n) => Some(sql_quote_default(&n.to_string())),
            Value::Bool(b) => Some(sql_quote_default(if *b { "true" } else { "false" })),
            _ => mismatch("字符串/日期"),
        },
        // JSON：对象/数组 → 紧凑序列化文本；字符串为合法 JSON 文本 → 定界；表达式原样
        FieldType::Json | FieldType::Array => match raw {
            Value::Object(_) | Value::Array(_) => Some(sql_quote_default(&raw.to_string())),
            Value::String(s) => {
                let t = s.trim();
                if is_sql_default_expr(t) {
                    Some(t.to_string())
                } else if serde_json::from_str::<Value>(t).is_ok() {
                    Some(sql_quote_default(t))
                } else {
                    mismatch("JSON")
                }
            }
            _ => mismatch("JSON"),
        },
        // 二进制等无法用 JSON 字面量表达的类型：不支持
        _ => mismatch("该字段类型"),
    }
}

/// 单个字段对象 → ColumnDefine。id_field 命中则标记主键。
///
/// 字段缺失/类型不匹配返回 `None`（由调用方按需跳过），不做失败中断：
/// - `name` 缺失或空串 → `None`（必须）
/// - `dataType` 缺失 → 默认 `VARCHAR`（与原行为一致，宽松容错）
/// - `nullable` 缺失 → 默认 `true`
/// - `fieldLength` / `decimalDigits` 缺失 → 走 VARCHAR 兜底 255 / Decimal 标度 0
/// - `defaultValue` 与字段类型不匹配 → 忽略默认值（warn），字段本身保留
///
/// 主键判定三路满足其一即视为 PK：
/// 1. `id_field` 非空且与字段名相等（约定式 PK）
/// 2. `isPrimaryKey` 是非 0 整数（部分老定义形态）
/// 3. `isPrimaryKey` 是 `true` 布尔
fn field_to_column(f: &Value, id_field: &str, ordinal: u32) -> Option<ColumnDefine> {
    // name 是必须项：缺失 / 空串 / 非字符串都视为"非法字段"，由调用方决定是否继续
    let name = f.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }
    // dataType 默认 VARCHAR（宽松容错：缺省视为短文本，不阻断编译）
    let data_type = f
        .get("dataType")
        .and_then(|v| v.as_str())
        .unwrap_or("VARCHAR");
    let ft = map_field_type(data_type);
    // nullable 默认 true（与 PG DDL 默认一致，简化前端定义）
    let nullable = f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
    // fieldLength：VARCHAR 用 length；DECIMAL 用 precision
    let field_len = f
        .get("fieldLength")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    // decimalDigits：DECIMAL 用 scale
    let dec = f
        .get("decimalDigits")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    // 主键判定（三路满足任一即视为 PK）：
    // 1) 显式 id_field 约定
    // 2) isPrimaryKey 是非 0 整数（兼容老形态）
    // 3) isPrimaryKey 是 true 布尔（现代形态）
    let is_pk = (!id_field.is_empty() && name == id_field)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_i64())
            .map(|n| n != 0)
            .unwrap_or(false)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    // 长度 / 精度：VARCHAR 用 length；DECIMAL 用 precision(=fieldLength)+scale(=decimalDigits)。
    // VARCHAR 未指定 fieldLength 时默认 255（避免被建表逻辑当成 TEXT，导致与期望不一致时无法 ALTER 修正）。
    let (length, precision, scale) = match ft {
        FieldType::String => (
            Some(field_len.unwrap_or(VARCHAR_DEFAULT_LENGTH)),
            None,
            None,
        ),
        FieldType::Decimal => (None, field_len, dec.or(Some(0))),
        _ => (None, None, None),
    };

    // 默认值：defaultValue（兼容下划线 default_value）按字段类型规范化为最终 SQL 表达式；
    // null / 类型不匹配 → None（宽松容错，warn 不中断编译）。
    let default_value = f
        .get("defaultValue")
        .or_else(|| f.get("default_value"))
        .and_then(|v| normalize_default_value(v, &ft));

    Some(ColumnDefine {
        name,
        label: field_caption(f),
        field_type: ft,
        is_primary_key: is_pk,
        // PK 列强制 NOT NULL（业务约束：PK 不能为 NULL）
        is_nullable: if is_pk { false } else { nullable },
        default_value,
        i18n: false,
        length,
        precision,
        scale,
        db_type: None,
        ordinal: Some(ordinal),
        create_time: None,
        update_time: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    })
}

/// 从 JSON 提取非空字符串列名序列（`uniqueKeys` 子数组 / `indexes` 条目的 `columns` 键共用）。
///
/// 非数组 / 全非字符串 / 列名全空 → `None`（调用方跳过该索引，避免建空索引）。
fn index_columns(v: Option<&Value>) -> Option<Vec<String>> {
    let cnames: Vec<String> = v?
        .as_array()?
        .iter()
        .filter_map(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if cnames.is_empty() {
        None
    } else {
        Some(cnames)
    }
}

/// 返回列序列中第一个不在合法列集里的列名（索引列存在性校验用）。
fn first_missing_column<'a>(
    cnames: &'a [String],
    valid: &std::collections::HashSet<&str>,
) -> Option<&'a str> {
    cnames.iter().map(|c| c.as_str()).find(|c| !valid.contains(c))
}

/// 表级索引收集：`uniqueKeys`（唯一索引）+ `indexes`（普通索引）→ `IndexDefine` 列表。
///
/// 两类键约定：
/// - `uniqueKeys`: 每个元素为一组联合唯一约束，**双形态**——纯列数组 `[col, ...]`
///   （存量）或对象 `{ name?, columns: [col, ...] }`（支持自定义名）→ `IndexKind::Unique`；
///   未提供 `name` 时按列内容哈希自动命名（见 [`auto_index_name`]）；
/// - `indexes`: `[{ name?, columns: [col, ...] }, ...]` 对象数组 → `IndexKind::Normal`，
///   `name` 可选、缺省同按列内容哈希自动命名；`columns` 顺序敏感（复合索引最左前缀语义）。
///
/// 校验（宽松容错，warn 不中断编译，与本文件非法字段的处理风格一致）：
/// - **列存在性**：`columns` 参数须是合并 base 字段集之后的最终列集；条目引用不存在的列 →
///   warn 并跳过整条——悬空引用会让 `CREATE [UNIQUE] INDEX` 直接 SQL 报错阻断整个部署；
/// - **冗余告警**：普通索引与某条唯一索引列序列完全相同 → warn（仍生成，PG 允许，仅冗余提示）；
/// - **超长告警**：自定义索引名超过 PG 标识符上限 63 字节 → warn（仍生成；PG 静默截断会导致
///   每次部署名字对不上，反复 DROP/CREATE）。
pub(crate) fn collect_indexes(
    t: &Value,
    table_name: &str,
    columns: &[ColumnDefine],
) -> Vec<IndexDefine> {
    let valid: std::collections::HashSet<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    let mut indexes: Vec<IndexDefine> = Vec::new();

    // ── uniqueKeys → 唯一索引 ──
    if let Some(uks) = t.get("uniqueKeys").and_then(|v| v.as_array()) {
        for uk in uks.iter() {
            // 双形态：纯列数组（存量）/ { name?, columns } 对象（支持自定义名）。
            let (custom_name, cols_val) = match uk {
                Value::Object(_) => (
                    uk.get("name").and_then(|v| v.as_str()),
                    uk.get("columns"),
                ),
                _ => (None, Some(uk)),
            };
            let Some(cnames) = index_columns(cols_val) else { continue };
            let name = custom_name
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| auto_index_name("uk", table_name, &cnames));
            if let Some(miss) = first_missing_column(&cnames, &valid) {
                warn!(table = table_name, index = %name, missing = miss,
                    "uniqueKeys 引用不存在的列，跳过该唯一索引");
                continue;
            }
            if let Some(dup) = indexes.iter().find(|x| x.name == name) {
                warn!(table = table_name, index = %name, dup_columns = ?dup.columns,
                    "自动索引名重复（列序列相同或哈希碰撞），跳过重复条目");
                continue;
            }
            warn_long_index_name(&name, table_name);
            indexes.push(IndexDefine {
                name,
                columns: cnames,
                kind: IndexKind::Unique,
                valid: true,
            });
        }
    }

    // ── indexes → 普通索引 ──
    if let Some(idxs) = t.get("indexes").and_then(|v| v.as_array()) {
        for ix in idxs.iter() {
            let Some(cnames) = index_columns(ix.get("columns")) else { continue };
            let name = ix
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| auto_index_name("idx", table_name, &cnames));
            if let Some(miss) = first_missing_column(&cnames, &valid) {
                warn!(table = table_name, index = %name, missing = miss,
                    "indexes 引用不存在的列，跳过该索引");
                continue;
            }
            // 冗余提示：与唯一索引列序列相同（仍生成，仅告警）
            if indexes.iter().any(|u| u.kind == IndexKind::Unique && u.columns == cnames) {
                warn!(table = table_name, index = %name,
                    "普通索引与唯一索引列序列相同，冗余（仍生成）");
            }
            // 同名去重（自定义名或自动哈希名与已有条目重名 → CREATE 会撞 already exists）
            if let Some(dup) = indexes.iter().find(|x| x.name == name) {
                warn!(table = table_name, index = %name, dup_columns = ?dup.columns,
                    "索引名与已有条目重复，跳过（PG 索引名 schema 级唯一）");
                continue;
            }
            // 自定义名超长提示（用户可自行改名）；自动名已由 auto_index_name 保证合法。
            warn_long_index_name(&name, table_name);
            indexes.push(IndexDefine {
                name,
                columns: cnames,
                kind: IndexKind::Normal,
                valid: true,
            });
        }
    }
    indexes
}

/// 生成自动索引名：`{prefix}_{table}_{hash6}`，哈希取**列序列**（逗号连接）的
/// FNV-1a 摘要——**不按下标**。列不变则名不变：
/// - 删除/移动/新增其它条目不影响本条目名字，无「下标前移」漂移；
/// - 删除条目后，DB 中同名旧索引的内容不再出现在定义里 → diff 正常产出 DROP
///   清理，不留孤儿索引；
/// - 确定性：同表同列每次编译产出同名，内省还原与设计期稳定对齐。
///
/// 超 PG 标识符上限 63 字节时截断表名，且哈希输入混入完整表名（防跨表截断撞名）：
/// `{prefix}_{截断表}_{hash6(table:columns)}`。
///
/// 一致性：前端（portal-definition-manager `_autoIndexName`）用同一算法展示
/// 自动名，改本规则须同步前端。
fn auto_index_name(prefix: &str, table: &str, columns: &[String]) -> String {
    let cols = columns.join(",");
    let full = format!("{prefix}_{table}_{:06x}", fnv1a32(&cols) & 0xff_ffff);
    if full.len() <= 63 {
        return full;
    }
    // 预算：{prefix} + 2 个下划线 + 6 位哈希 ≤ 63（str::len 即 UTF-8 字节数）
    let t_avail = 63usize.saturating_sub(prefix.len() + 2 + 6);
    let trunc = truncate_utf8(table, t_avail);
    let hash_input = format!("{table}:{cols}");
    format!("{prefix}_{trunc}_{:06x}", fnv1a32(&hash_input) & 0xff_ffff)
}

/// 按字节预算截断字符串，不切断多字节字符（回退到字符边界）。
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// FNV-1a 32 位哈希（对字符串的 UTF-8 字节）。前端用 `Math.imul` + `>>> 0`
/// 实现同算法，两端结果一致。
fn fnv1a32(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(16777619);
    }
    h
}

/// 自定义索引名超长提示：PG 标识符上限 63 字节（NAMEDATALEN-1），超长会被静默
/// 截断，造成内省还原的名字与设计期名对不上 → 每次部署反复 DROP/CREATE。
/// 仅 warn 不阻断（用户可自行缩短名字）。
fn warn_long_index_name(name: &str, table_name: &str) {
    // str::len() 即字节数（PG 标识符上限按字节计，中文等非 ASCII 名更易超限）
    let len = name.len();
    if len > 63 {
        warn!(table = table_name, index = %name, bytes = len,
            "索引名超过 PG 标识符上限 63 字节，将被截断导致部署名不匹配（建议缩短）");
    }
}

/// 将一组字段 JSON 数组追加到 columns（去重 + 自增 ordinal）。
///
/// 内部循环：每个字段尝试 `field_to_column` 转换，转换成功且列名未在 `seen` 中才追加。
/// 序号 `ord` 每次成功转换后自增（保证 ColumnDefine.ordinal 与"列在表中的位置"对齐）。
///
/// 设计要点：
/// - 去重：保证同名列不重复加入（覆盖逻辑 = 第一个出现的胜出，与原行为一致）
/// - 序号连续：哪怕中途字段被跳过，ord 仍会按"成功转换"次数自增，与"列在表中的视觉位置"对齐
fn push_field_set(
    fields: &[Value],
    id_field: &str,
    columns: &mut Vec<ColumnDefine>,
    seen: &mut std::collections::HashSet<String>,
    ord: &mut u32,
) {
    for f in fields {
        // 序号在尝试转换前 +1（保证即使跳过非法字段也不会让后续列的 ordinal 回退）
        *ord += 1;
        if let Some(c) = field_to_column(f, id_field, *ord)
            // seen.insert 返回 true 表示新插入（未重复），false 表示已存在（跳过）
            && seen.insert(c.name.clone())
        {
            columns.push(c);
        }
    }
}

/// 从已收集的 columns 构造 TableDefine（统一 15 字段初始化）。
///
/// 收敛"构造 TableDefine 的 15+ 个字段都必须显式写"的负担，所有调用方（`compile_dct` /
/// `compile_doc` / `compile_rpt`）共用同一构造入口，缺省字段在此统一填默认值，避免散落
/// 写多份带来的字段集漂移。
///
/// 参数：
/// - `table_name`：物理表名（PG 端 identifier）
/// - `display_name`：显示名（前端友好）
/// - `comment`：表注释（对应 PG `COMMENT ON TABLE`）
/// - `primary_keys`：主键列名列表（多列联合 PK 也支持）
/// - `indexes`：唯一索引列表
/// - `columns`：列定义列表
fn finish_table(
    table_name: String,
    display_name: String,
    comment: Option<String>,
    primary_keys: Vec<String>,
    indexes: Vec<IndexDefine>,
    columns: Vec<ColumnDefine>,
) -> TableDefine {
    TableDefine {
        table_name,
        display_name,
        columns,
        primary_keys,
        indexes,
        version: 1,
        create_time: None,
        update_time: None,
        i18n: false,
        comment,
        schema: None,
        tablespace: None,
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: Default::default(),
    }
}

/// DCT 定义 doc + 其 base fieldset doc → Vec<TableDefine>（每个 dictionaryTable 一张表）。
///
/// # 编译流程
///
/// 遍历 `dictionaryTables[]`，对每张表：
/// 1. 抽 `dictMeta.{tableName, idField, dictName, remark}` → 物理表名 / 主键约定 / 显示名 / 表注释
/// 2. 合并字段来源（本表 fields + 7 个内建字段集 + 任意 *FieldSet 兜底），按列名去重
/// 3. 收集 `is_primary_key=true` 的列 → `primary_keys`
/// 4. 抽 `uniqueKeys` → 唯一索引
/// 5. 走 `finish_table` 统一构造 TableDefine
///
/// # 字段集合并顺序（保证列序稳定）
///
/// 本表 fields → baseFieldSet → hierarchyFieldSet → scopeFieldSet → effectiveFieldSet →
/// disableFieldSet → auditFieldSet → systemFieldSet → 任意其它 *FieldSet 兜底。
/// `hierarchyFieldSet` 提供自分级字典的 `parent_id` / `full_path` / `level_no` / `is_leaf`。
///
/// # 容错
///
/// - `tableName` 缺失 / 空串 → 跳过该表
/// - `fields` 缺失 / 非数组 → 视为空数组
/// - 任意 `*FieldSet` 引用值缺失 / 非字符串 / 在 base 中查不到 → 静默跳过
/// - 单字段 `field_to_column` 返回 `None` → 跳过该字段
pub(crate) fn compile_dct(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("dictionaryTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    for t in tables {
        // 抽 dictMeta；缺失时用空对象兜底（dictMeta 是必填但前端偶有省略）
        let dm = t.get("dictMeta").cloned().unwrap_or(json!({}));
        let table_name = dm
            .get("tableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // tableName 缺失直接跳过该表（无 tableName 无法建表）
        if table_name.is_empty() {
            continue;
        }
        // idField 约定主键（与 isPrimaryKey 显式标注并行生效）
        let id_field = dm.get("idField").and_then(|v| v.as_str()).unwrap_or("id");
        // 显示名：dictName 缺省回退到 table_name（保证总非空）
        let display = dm
            .get("dictName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string();
        // 表注释：对应 PG COMMENT ON TABLE
        let comment = dm
            .get("remark")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 合并字段来源：本表 + 三个引用字段集（跳过 null / 缺失）。
        let mut columns: Vec<ColumnDefine> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ord: u32 = 0;
        // 1) 本表自有字段（最优先：定义中"我这张表有什么"）
        if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
            push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
        }
        // 2) 合并全部 *FieldSet 引用（base/hierarchy/audit/effective/disable/scope/system…）。
        //    固定顺序保证列序稳定；hierarchyFieldSet 提供自分级字典的 parent_id/full_path/level_no/is_leaf。
        for set_key in [
            "baseFieldSet",
            "hierarchyFieldSet",
            "scopeFieldSet",
            "effectiveFieldSet",
            "disableFieldSet",
            "auditFieldSet",
            "systemFieldSet",
        ] {
            if let Some(set_name) = t.get(set_key).and_then(|v| v.as_str())
                && let Some(fields) = base_fieldset(base, set_name)
            {
                push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
            }
        }
        // 3) 兜底：捕获上面未列出的任何 `*FieldSet` 键（前向兼容新增字段集）。
        if let Some(obj) = t.as_object() {
            for (k, v) in obj {
                if k.ends_with("FieldSet")
                    && !matches!(
                        k.as_str(),
                        "baseFieldSet"
                            | "hierarchyFieldSet"
                            | "scopeFieldSet"
                            | "effectiveFieldSet"
                            | "disableFieldSet"
                            | "auditFieldSet"
                            | "systemFieldSet"
                    )
                    && let Some(set_name) = v.as_str()
                    && let Some(fields) = base_fieldset(base, set_name)
                {
                    push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
                }
            }
        }

        // 收集主键：扫描所有列，挑出 is_primary_key=true 的（兼容多列联合 PK）
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();

        let indexes = collect_indexes(t, &table_name, &columns);
        out.push(finish_table(
            table_name,
            display,
            comment,
            primary_keys,
            indexes,
            columns,
        ));
    }
    out
}

/// 编译一张 DOC 表（或一张 sum/summary 表）→ TableDefine。
///
/// DOC 与 DCT 编译路径共用同套 `field_to_column` / `finish_table` 工具，但字段合并规则不同：
/// - 字段来源：仅本表 `fields` + `documentFieldSets[]` 引用的 base 字段集
/// - 不抽 `dictMeta`（DOC 用顶层 `tableName` / `name` / `id`）
/// - DOC 主键约定为 `id`（与 DCT 不同），sum/summaries 表也沿用
///
/// # 三段 fallback
///
/// DOC 表名可能写在 `tableName` / `name` / `id` 三个字段任一处（不同模块风格不同）：
/// 优先 `tableName` > `name` > `id`。三者全缺 / 全空 → `None`（调用方跳过该表）。
fn compile_doc_table(t: &Value, base: &Value) -> Option<TableDefine> {
    // DOC 表名 fallback 链：tableName → name → id（部分老定义用 id 作表名）
    let table_name = t
        .get("tableName")
        .or_else(|| t.get("name"))
        .or_else(|| t.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    if table_name.is_empty() {
        return None;
    }
    // 显示名：tableAlias > caption > name > table_name（fallback）
    let display = table_caption(t, &table_name);
    // 表注释：对应 PG COMMENT ON TABLE
    let comment = t
        .get("remark")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut columns: Vec<ColumnDefine> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ord: u32 = 0;
    // DOC 主键约定：id（若存在）。sum/summaries 表也沿用该约定。
    let id_field = "id";
    // 本表 fields
    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
    }
    // documentFieldSets: [ "voucherCommonFields", ... ] 引用 base。汇总表通常不配，
    // 但保留同样展开能力，便于后续把通用审计字段抽到 base。
    if let Some(sets) = t.get("documentFieldSets").and_then(|v| v.as_array()) {
        for s in sets {
            if let Some(set_name) = s.as_str()
                && let Some(fields) = base_fieldset(base, set_name)
            {
                push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
            }
        }
    }

    // 收集主键（同 DCT：扫描所有列）
    let primary_keys: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    Some(finish_table(
        table_name.clone(),
        display,
        comment,
        primary_keys,
        collect_indexes(t, &table_name, &columns),
        columns,
    ))
}

/// DOC 定义 doc → Vec<TableDefine>（每个 voucherTable 一张表）。
///
/// DOC 列 = 本表 fields + documentFieldSets 引用的 base 字段集；每层表下的
/// summaries/sum 汇总表也按同一 TableDefine 链路编译，以复用创建与升级执行器。
///
/// # 编译顺序
///
/// 1. 遍历 `voucherTables[]`，对每张主表走 `compile_doc_table`
/// 2. 对每张主表的 `summaries[]` 与 `sum[]` 走 `compile_doc_table`
/// 3. 用 `seen_tables` 去重（防御性：极少数定义会把主表/汇总表命名重复）
pub(crate) fn compile_doc(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("voucherTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    // seen_tables：防御性去重（极少数定义可能在主表/汇总表间出现同名）
    let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in tables {
        // 1) 主表自身
        if let Some(def) = compile_doc_table(t, base)
            && seen_tables.insert(def.table_name.clone())
        {
            out.push(def);
        }
        // 2) 该主表下的汇总表（summaries + sum，两种命名都支持）
        for key in ["summaries", "sum"] {
            if let Some(summaries) = t.get(key).and_then(|v| v.as_array()) {
                for summary in summaries {
                    if let Some(def) = compile_doc_table(summary, base)
                        && seen_tables.insert(def.table_name.clone())
                    {
                        out.push(def);
                    }
                }
            }
        }
    }
    out
}

/// RPT 报表定义 → Vec<TableDefine>。报表落地的三张 cr_* 物理表
/// （cr_report_instance / cr_cell_value / cr_report_snapshot）是全部报表模板共享的
/// 基础设施，其表结构声明在 base_rpt_meta 的 storageTables 中，一次建出、幂等升级。
/// 报表模板本身（grid/cells/datasets）是运行期概念，不产生 DDL。
/// 列 = storageTables[i].fields + 其 auditFieldSet 引用的 base 字段集，复用 field_to_column。
///
/// # 与 DCT/DOC 的差异
///
/// - 入口是 `base`（不是 `doc`）：所有报表都共用同一组 storageTables，与模板正交
/// - 字段合并：本表 fields + 任意 `*FieldSet`（前向兼容，目前主要 auditFieldSet）
/// - 报表模板本身（`_doc`）不参与编译，参数名加下划线表示"故意不用"
pub(crate) fn compile_rpt(_doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match base.get("storageTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    // 防御性去重：base 自身的 storageTables 理论上无重复，但保证一次建表清单唯一
    let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in tables {
        let table_name = t
            .get("tableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // 跳过空名 / 重复名
        if table_name.is_empty() || !seen_tables.insert(table_name.clone()) {
            continue;
        }
        // idField 默认 "id"（与 DCT 一致）
        let id_field = t.get("idField").and_then(|v| v.as_str()).unwrap_or("id");
        // 显示名：displayName 缺省回退到 table_name
        let display = t
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string();
        // 表注释
        let comment = t
            .get("remark")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut columns: Vec<ColumnDefine> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ord: u32 = 0;
        // 本表 fields
        if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
            push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
        }
        // 合并全部 *FieldSet 引用（当前仅 auditFieldSet；前向兼容任意 *FieldSet 键）。
        if let Some(obj) = t.as_object() {
            for (k, v) in obj {
                if k.ends_with("FieldSet")
                    && let Some(set_name) = v.as_str()
                    && let Some(fields) = base_fieldset(base, set_name)
                {
                    push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
                }
            }
        }

        // 收集主键
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();

        let indexes = collect_indexes(t, &table_name, &columns);
        out.push(finish_table(
            table_name,
            display,
            comment,
            primary_keys,
            indexes,
            columns,
        ));
    }
    out
}

// ════════════════════════════════════════════════════════════════════════
//  二、内省辅助：读定义文件（复用 definitions store）
// ════════════════════════════════════════════════════════════════════════

/// 读某定义文件全文（domain/application/module/file）。
///
/// 走 `cmx_model_meta::definitions::store::get_definition`：先在内存缓存查，未命中再走文件系统
/// （`data/meta/definitions/<domain>/<app>/<module>/<file>`），最终反序列化为 `Value`。
///
/// 错误以 `Error::BadRequest` 抛出（含 file 名便于排查）。
pub(crate) async fn read_def(domain: &str, app: &str, module: &str, file: &str) -> Result<Value> {
    let r = cmx_model_meta::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        // app 别名（与 application 同义；store 同时支持两种 key）
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        // id / kind 不参与本次查找
        id: None,
        kind: None,
    };
    cmx_model_meta::definitions::store::get_definition(&r)
        .await
        .map_err(|e| Error::BadRequest(format!("读取定义失败 {file}: {e}")))
}

/// 读 base 字段集文件（domain=base）。
///
/// base 是与业务域并列的"基础字段集"域，所有 DCT/DOC/RPT 定义都引用它来获得通用列
/// （如 code/name/status/审计字段等）。路径恒为 `data/meta/definitions/base/<file>`。
async fn read_base(file: &str) -> Result<Value> {
    let r = cmx_model_meta::definitions::store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    cmx_model_meta::definitions::store::get_definition(&r)
        .await
        .map_err(db_err("读取 base 字段集失败"))
}

/// 编译一个定义（kind=DCT/DOC/RPT）→ (TableDefine 列表, 源 JSON)。
///
/// 统一入口：根据 `kind` 选择 base 引用 key（`baseDctMetaRef` / `baseDocMetaRef` /
/// `baseRptMetaRef`）与默认 base 文件名，然后调对应编译器。
///
/// # base 引用规则
///
/// - `kind == "DOC"` → key=`baseDocMetaRef`，default=`base_doc_meta_v1.json`
/// - `kind == "RPT"` → key=`baseRptMetaRef`，default=`base_rpt_meta_v1.json`
/// - 其他（含 "DCT"） → key=`baseDctMetaRef`，default=`base_dct_meta_v1.json`
///
/// base 文件缺失时降级为空字段集（`{"fieldSets": {}}`），避免单个 base 故障阻断整个模块编译。
/// 但会打 `warn!` 日志，便于排查"定义依赖了未提供的 base"的情况。
///
/// # 返回值
///
/// - `Vec<TableDefine>`：编译出的表定义（每张表 1 个 TableDefine）
/// - `Value`：原始定义 JSON（部署路径会用它做 source_json 留档 + 取 moduleMeta.version）
pub(crate) async fn compile_definition(
    kind: &str,
    domain: &str,
    app: &str,
    module: &str,
    file: &str,
) -> Result<(Vec<TableDefine>, Value)> {
    // 1) 读定义文件全文
    let doc = read_def(domain, app, module, file).await?;
    // 2) 按 kind 选 base 引用 key + 默认 base 文件名
    let base_ref_key = match kind {
        "DOC" => "baseDocMetaRef",
        "RPT" => "baseRptMetaRef",
        _ => "baseDctMetaRef",
    };
    let default_base = match kind {
        "DOC" => "base_doc_meta_v1.json",
        "RPT" => "base_rpt_meta_v1.json",
        _ => "base_dct_meta_v1.json",
    };
    // 3) 读 base（定义里可显式覆盖默认；缺失回退默认）
    let base_file = doc
        .get(base_ref_key)
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_base)
        .to_string();
    // 4) base 失败时降级为空字段集（保留 warn 日志，便于排查"定义依赖了未提供的 base"）
    let base = match read_base(&base_file).await {
        Ok(v) => v,
        Err(e) => {
            // base 文件缺失时降级为空字段集（某些定义不依赖 base），但必须留日志；
            // 传输/解析错误同样降级，避免单个 base 文件故障阻断整个模块编译。
            warn!(base_file = %base_file, error = %e, "读取 base 字段集失败，降级为空字段集");
            json!({ "fieldSets": {} })
        }
    };
    // 5) 调对应编译器
    let defs = match kind {
        "DOC" => compile_doc(&doc, &base),
        "RPT" => compile_rpt(&doc, &base),
        _ => compile_dct(&doc, &base),
    };
    Ok((defs, doc))
}

#[cfg(test)]
mod default_value_tests {
    use super::*;
    use serde_json::json;

    fn norm(v: Value, ft: FieldType) -> Option<String> {
        normalize_default_value(&v, &ft)
    }

    #[test]
    fn int_defaults() {
        assert_eq!(norm(json!(0), FieldType::Int), Some("0".into()));
        assert_eq!(norm(json!(-5), FieldType::Int), Some("-5".into()));
        assert_eq!(norm(json!("100"), FieldType::Int), Some("100".into()));
        // 类型不匹配 / null → None（宽松容错）
        assert_eq!(norm(json!("abc"), FieldType::Int), None);
        assert_eq!(norm(json!(true), FieldType::Int), None);
        assert_eq!(norm(Value::Null, FieldType::Int), None);
    }

    #[test]
    fn int_defaults_reject_fractions() {
        // 小数一律拒绝：PG 对整型列 DEFAULT 1.5 无 numeric→int 隐式赋值转换，
        // 放行会直接报错中断整个部署。
        assert_eq!(norm(json!(1.5), FieldType::Int), None);
        assert_eq!(norm(json!("1.5"), FieldType::Int), None);
        assert_eq!(norm(json!(-0.5), FieldType::Int), None);
        // 「整值的小数语法表示」（2.0 / 1e3，serde_json 走 f64）转整数放行
        assert_eq!(norm(json!(2.0), FieldType::Int), Some("2".into()));
        assert_eq!(norm(json!(1e3), FieldType::Int), Some("1000".into()));
    }

    #[test]
    fn decimal_defaults() {
        assert_eq!(norm(json!(1.5), FieldType::Decimal), Some("1.5".into()));
        assert_eq!(norm(json!("3.14"), FieldType::Float), Some("3.14".into()));
        assert_eq!(norm(json!("nan_is_not_num"), FieldType::Decimal), None);
    }

    #[test]
    fn bool_defaults() {
        assert_eq!(norm(json!(true), FieldType::Bool), Some("TRUE".into()));
        assert_eq!(norm(json!(false), FieldType::Bool), Some("FALSE".into()));
        assert_eq!(norm(json!("1"), FieldType::Bool), Some("TRUE".into()));
        assert_eq!(norm(json!("No"), FieldType::Bool), Some("FALSE".into()));
        assert_eq!(norm(json!("off"), FieldType::Bool), None);
    }

    #[test]
    fn string_defaults_type_aware() {
        // 核心场景：字符串型字段的数字/TRUE 样内容必须定界为字符串字面量，
        // 而不是被 DDL 层内容启发式裸输出成数字/布尔
        assert_eq!(norm(json!("0"), FieldType::String), Some("'0'".into()));
        assert_eq!(norm(json!("true"), FieldType::String), Some("'true'".into()));
        assert_eq!(norm(json!(1), FieldType::String), Some("'1'".into()));
        // 单引号转义
        assert_eq!(norm(json!("it's"), FieldType::Text), Some("'it''s'".into()));
        // 含括号的普通文本也要定界（DDL 层启发式会当函数原样输出导致语法错）
        assert_eq!(norm(json!("N/A (备用)"), FieldType::String), Some("'N/A (备用)'".into()));
    }

    #[test]
    fn date_datetime_defaults() {
        assert_eq!(norm(json!("2023-01-01"), FieldType::Date), Some("'2023-01-01'".into()));
        assert_eq!(
            norm(json!("2023-01-01 10:00:00"), FieldType::DateTime),
            Some("'2023-01-01 10:00:00'".into())
        );
        // 函数表达式原样
        assert_eq!(norm(json!("now()"), FieldType::DateTime), Some("now()".into()));
        assert_eq!(
            norm(json!("CURRENT_TIMESTAMP"), FieldType::DateTime),
            Some("CURRENT_TIMESTAMP".into())
        );
    }

    #[test]
    fn json_defaults() {
        assert_eq!(norm(json!({}), FieldType::Json), Some("'{}'".into()));
        assert_eq!(norm(json!([]), FieldType::Json), Some("'[]'".into()));
        assert_eq!(norm(json!({"a":1}), FieldType::Json), Some("'{\"a\":1}'".into()));
        assert_eq!(norm(json!("{}"), FieldType::Json), Some("'{}'".into()));
        // 已定界复合表达式原样透传
        assert_eq!(norm(json!("'{}'::jsonb"), FieldType::Json), Some("'{}'::jsonb".into()));
        // 非 JSON 文本 → None
        assert_eq!(norm(json!("not json"), FieldType::Json), None);
    }

    #[test]
    fn field_to_column_passes_default() {
        // 端到端：字段 JSON defaultValue → ColumnDefine.default_value（最终 SQL 表达式）
        let c1 = field_to_column(&json!({ "name": "sort_no", "dataType": "INT", "defaultValue": 0 }), "id", 1)
            .expect("合法字段");
        assert_eq!(c1.default_value, Some("0".to_string()));

        let c2 = field_to_column(
            &json!({ "name": "status", "dataType": "VARCHAR", "fieldLength": 16, "defaultValue": "1" }),
            "id",
            2,
        )
        .expect("合法字段");
        assert_eq!(c2.default_value, Some("'1'".to_string()));

        // 兼容下划线键名
        let c3 = field_to_column(
            &json!({ "name": "cfg", "dataType": "JSONB", "default_value": {} }),
            "id",
            3,
        )
        .expect("合法字段");
        assert_eq!(c3.default_value, Some("'{}'".to_string()));

        // 类型不匹配：默认值被忽略，字段本身保留
        let c4 = field_to_column(&json!({ "name": "age", "dataType": "INT", "defaultValue": "abc" }), "id", 4)
            .expect("合法字段");
        assert_eq!(c4.default_value, None);
    }
}

#[cfg(test)]
mod indexes_tests {
    use super::*;

    fn col(name: &str) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: name.to_string(),
            field_type: FieldType::String,
            is_primary_key: false,
            is_nullable: true,
            default_value: None,
            i18n: false,
            length: None,
            precision: None,
            scale: None,
            db_type: None,
            ordinal: None,
            create_time: None,
            update_time: None,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: Default::default(),
        }
    }

    /// 合法列集（模拟合并 base 字段集后的最终列集）
    fn valid_cols() -> Vec<ColumnDefine> {
        ["code", "tax_no", "name", "updated_at"]
            .iter()
            .map(|n| col(n))
            .collect()
    }

    #[test]
    fn unique_and_normal_collected() {
        let t = json!({
            "uniqueKeys": [["code"], ["tax_no", "name"]],
            "indexes": [
                { "columns": ["name"] },
                { "name": "idx_x", "columns": ["code", "updated_at"] }
            ]
        });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 4, "两类索引合并: {idxs:?}");
        // uniqueKeys → Unique，自动名 = uk_<table>_<列序列哈希>（不按下标）
        assert_eq!(idxs[0].name, "uk_t1_316cf4"); // ["code"]
        assert_eq!(idxs[0].kind, IndexKind::Unique);
        assert_eq!(idxs[0].columns, vec!["code".to_string()]);
        assert_eq!(idxs[1].name, "uk_t1_8a8d1b"); // ["tax_no","name"]
        assert_eq!(idxs[1].columns, vec!["tax_no".to_string(), "name".to_string()]);
        // indexes → Normal；name 缺省自动命名，顺序敏感保留
        assert_eq!(idxs[2].name, "idx_t1_39bde6"); // ["name"]
        assert_eq!(idxs[2].kind, IndexKind::Normal);
        assert_eq!(idxs[2].columns, vec!["name".to_string()]);
        assert_eq!(idxs[3].name, "idx_x");
        assert_eq!(idxs[3].columns, vec!["code".to_string(), "updated_at".to_string()]);
    }

    #[test]
    fn normal_index_name_trimmed() {
        // name 为空白串 → 视为缺省自动命名
        let t = json!({ "indexes": [{ "name": "   ", "columns": ["code"] }] });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 1);
        assert_eq!(idxs[0].name, "idx_t1_316cf4");
    }

    #[test]
    fn unique_key_object_form_with_custom_name() {
        // 对象形态 { name?, columns }：自定义名优先；空名/缺名退回自动命名；与纯数组混用兼容
        let t = json!({
            "uniqueKeys": [
                { "name": "uk_supplier_code", "columns": ["code"] },
                { "name": "   ", "columns": ["tax_no"] },
                { "columns": ["name"] },
                ["updated_at"]
            ]
        });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 4);
        assert_eq!(idxs[0].name, "uk_supplier_code");
        assert_eq!(idxs[0].kind, IndexKind::Unique);
        assert_eq!(idxs[1].name, "uk_t1_3f7c44"); // 空白名 → 自动名（tax_no）
        assert_eq!(idxs[2].name, "uk_t1_39bde6"); // 缺名 → 自动名（name）
        assert_eq!(idxs[3].name, "uk_t1_4b6560"); // 纯数组存量形态 → 自动名（updated_at）
    }

    #[test]
    fn auto_name_stable_regardless_of_position() {
        // 核心性质：自动名由列内容决定——条目位置变化（前移）名字不变。
        // 下标命名会在删除中间条目后前移漂移，造成 DB 孤儿索引；哈希命名无此问题。
        let cols = vec!["tax_no".to_string()];
        let a = auto_index_name("idx", "t1", &cols);
        let b = auto_index_name("idx", "t1", &cols);
        assert_eq!(a, b, "同表同列 → 确定性同名");
        assert_eq!(a, "idx_t1_3f7c44");
        // 列序列不同 → 名不同（顺序也参与哈希：join 顺序敏感）
        assert_ne!(auto_index_name("idx", "t1", &["a".to_string(), "b".to_string()]),
                   auto_index_name("idx", "t1", &["b".to_string(), "a".to_string()]));
        // 同列序列 unique 与 normal（冗余并存场景）前缀区分
        assert_ne!(auto_index_name("uk", "t1", &cols), auto_index_name("idx", "t1", &cols));
    }

    #[test]
    fn auto_name_truncated_within_63_bytes() {
        // 60 字节长表名：idx_<table>_<hash6> 超限 → 截断表名并混入表名哈希
        let table = "a".repeat(60);
        let t = json!({ "uniqueKeys": [["code"], ["tax_no"]] });
        let idxs = collect_indexes(&t, &table, &valid_cols());
        assert_eq!(idxs.len(), 2);
        for ix in idxs.iter() {
            assert!(
                ix.name.len() <= 63,
                "自动名必须 ≤63 字节: {} ({})",
                ix.name,
                ix.name.len()
            );
            assert!(ix.name.starts_with("uk_"), "保留类型前缀: {}", ix.name);
        }
        // 确定性：同表两次生成完全一致
        let again = collect_indexes(&t, &table, &valid_cols());
        assert_eq!(idxs[0].name, again[0].name);
        assert_ne!(idxs[0].name, idxs[1].name, "列不同 → 名不同");
    }

    #[test]
    fn auto_name_truncation_avoids_cross_table_collision() {
        // 两个 60 字节表名仅末位不同：截断后共享前缀；超长形态哈希混入完整表名 → 不撞。
        let t1 = format!("{}x", "a".repeat(59));
        let t2 = format!("{}y", "a".repeat(59));
        let cols = vec!["code".to_string()];
        let n1 = auto_index_name("uk", &t1, &cols);
        let n2 = auto_index_name("uk", &t2, &cols);
        assert_ne!(n1, n2, "截断 + 表名哈希须防跨表撞名: {n1} vs {n2}");
        assert!(n1.len() <= 63 && n2.len() <= 63);
    }

    #[test]
    fn duplicate_auto_name_skipped() {
        // 同表两条相同列序列 → 同名自动索引 → 第二条跳过（CREATE 撞 already exists）
        let t = json!({ "uniqueKeys": [["code"], ["code"]] });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 1, "重复列序列条目被跳过: {idxs:?}");
    }

    #[test]
    fn auto_name_short_table_format() {
        // 短表名：{prefix}_{table}_{列哈希6}
        assert_eq!(
            auto_index_name("uk", "cm_supplier", &["code".to_string()]),
            "uk_cm_supplier_316cf4"
        );
    }

    #[test]
    fn missing_column_skipped() {
        // 悬空列引用（unique 与 normal 各一）→ 跳过整条，防 CREATE INDEX SQL 报错阻断部署
        let t = json!({
            "uniqueKeys": [["code"], ["ghost"]],
            "indexes": [{ "columns": ["nope"] }, { "columns": ["name"] }]
        });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 2, "悬空引用条目被跳过: {idxs:?}");
        assert!(idxs.iter().all(|i| !i.columns.iter().any(|c| c == "ghost" || c == "nope")));
    }

    #[test]
    fn redundant_normal_with_unique_still_generated() {
        // 普通索引与唯一索引列序列相同 → 冗余告警但仍生成（PG 允许）
        let t = json!({
            "uniqueKeys": [["code"]],
            "indexes": [{ "columns": ["code"] }]
        });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 2, "冗余仅告警不裁剪: {idxs:?}");
        assert_eq!(idxs[0].kind, IndexKind::Unique);
        assert_eq!(idxs[1].kind, IndexKind::Normal);
    }

    #[test]
    fn old_file_without_indexes_key_compat() {
        // 老定义只有 uniqueKeys：正常收集，自动名按列哈希命名
        let t = json!({ "uniqueKeys": [["code"]] });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 1);
        assert_eq!(idxs[0].kind, IndexKind::Unique);
        assert_eq!(idxs[0].name, "uk_t1_316cf4");
    }

    #[test]
    fn empty_entries_skipped() {
        let t = json!({
            "uniqueKeys": [[], ["code"]],
            "indexes": [{ "columns": [] }, { "columns": ["tax_no"] }]
        });
        let idxs = collect_indexes(&t, "t1", &valid_cols());
        assert_eq!(idxs.len(), 2, "空列序列条目跳过: {idxs:?}");
    }

    #[test]
    fn compile_dct_end_to_end_indexes() {
        let doc = json!({
            "dictionaryTables": [{
                "dictMeta": { "tableName": "cm_test", "idField": "id", "dictName": "测试" },
                "fields": [
                    { "name": "id", "dataType": "BIGINT", "isPrimaryKey": 1 },
                    { "name": "code", "dataType": "VARCHAR" },
                    { "name": "status", "dataType": "INT" }
                ],
                "uniqueKeys": [["code"]],
                "indexes": [{ "columns": ["status"] }]
            }]
        });
        let base = json!({ "fieldSets": {} });
        let defs = compile_dct(&doc, &base);
        assert_eq!(defs.len(), 1);
        let idxs = &defs[0].indexes;
        assert_eq!(idxs.len(), 2, "DCT 端到端两类索引: {idxs:?}");
        assert!(idxs.iter().any(|i| i.kind == IndexKind::Unique && i.columns == vec!["code".to_string()]));
        assert!(idxs.iter().any(|i| i.kind == IndexKind::Normal && i.columns == vec!["status".to_string()]));
    }

    #[test]
    fn compile_doc_with_summary_indexes() {
        // DOC 主表与汇总表都走 compile_doc_table → indexes/uniqueKeys 同样生效
        let doc = json!({
            "voucherTables": [{
                "tableName": "cv_main",
                "fields": [{ "name": "id", "dataType": "BIGINT", "isPrimaryKey": 1 }, { "name": "biz_no", "dataType": "VARCHAR" }],
                "indexes": [{ "columns": ["biz_no"] }],
                "summaries": [{
                    "tableName": "cv_sum",
                    "fields": [{ "name": "id", "dataType": "BIGINT", "isPrimaryKey": 1 }, { "name": "k", "dataType": "VARCHAR" }],
                    "uniqueKeys": [["k"]]
                }]
            }]
        });
        let base = json!({ "fieldSets": {} });
        let defs = compile_doc(&doc, &base);
        assert_eq!(defs.len(), 2, "主表 + 汇总表: {defs:?}");
        let main = defs.iter().find(|d| d.table_name == "cv_main").expect("主表");
        assert!(main.indexes.iter().any(|i| i.kind == IndexKind::Normal && i.columns == vec!["biz_no".to_string()]));
        let sum = defs.iter().find(|d| d.table_name == "cv_sum").expect("汇总表");
        assert!(sum.indexes.iter().any(|i| i.kind == IndexKind::Unique && i.columns == vec!["k".to_string()]));
    }
}
