//! 字典模块共享工具（消除 repo/tree/multi/write 四处的 `field_str` 重复）。

use serde_json::Value;

/// 取行的某字段字符串值（数字/布尔也转字符串，缺失或对象/数组返回空串）。
///
/// 供 repo（过滤比对）/ tree（父子组装）/ multi（join 取值）/ write（id 取值）共用,
/// 保证「字段如何归一为字符串」口径一致。
pub fn field_str(row: &Value, field: &str) -> String {
    match row.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}
