//! JSON 处理工具模块。
//!
//! 提供数据库 JSONB 列读取时常见的「字符串或对象」归一化等辅助函数。

use serde_json::Value;

/// 将 JSONB 列读取出的 `serde_json::Value` 归一化为 JSON 对象形式。
///
/// 数据库驱动在读取 JSONB 列时，可能返回:
/// - JSON 对象/数组（已是结构化的 `Value::Object` / `Value::Array`）
/// - 文本字符串（`Value::String`，内容是 JSON 文本的序列化形式）
///
/// 本函数对字符串形式做一次反序列化，得到真正的 JSON 结构；
/// 非字符串值原样返回。解析失败时返回 `Value::Null`，避免抛错阻断流程。
///
/// # Arguments
/// * `value` - 数据库返回的原始 `serde_json::Value`
///
/// # Returns
/// 归一化后的 JSON 值（字符串被解析为对象/数组；其他值原样返回）。
///
/// # Example
/// ```
/// use cmx_utils::json::coerce_to_object;
/// use serde_json::json;
///
/// // 字符串形式 → 解析为对象
/// let s = json!("{\"a\":1}");
/// assert_eq!(coerce_to_object(s), json!({"a": 1}));
///
/// // 已是对象 → 原样返回
/// let o = json!({"a": 1});
/// assert_eq!(coerce_to_object(o), json!({"a": 1}));
/// ```
pub fn coerce_to_object(value: Value) -> Value {
    if value.is_string() {
        serde_json::from_str::<Value>(value.as_str().unwrap_or("null")).unwrap_or(Value::Null)
    } else {
        value
    }
}

/// 从定义 JSON 的 `fieldSets` 里按名取某字段集的 `fields` 数组引用。
///
/// dct-model / doc-model / model-deploy 共用（消除 3 份复刻）。
/// `base` 是 base 定义 JSON（含 `fieldSets` 对象），`set_name` 是字段集名。
pub fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}
