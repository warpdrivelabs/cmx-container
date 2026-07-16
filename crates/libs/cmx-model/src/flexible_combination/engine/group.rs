//! 分组节点构建（CmxColumnGroup）。
//!
//! 复刻 Node `flexible-combination-engine.js` 的 `_buildGroupNode`（递归构造 CmxColumnGroup.toJSON）。

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

/// CmxColumnGroup 节点构建（递归）。groups members 可为字符串(列id)或嵌套组对象。
///
/// 成员为字符串时按列 id 从 `by_id` 取列并标记 used；为对象时递归构建子分组。
/// 空分组（无有效成员）返回 `None`。
pub(super) fn build_group_node(
    node: &Value,
    by_id: &HashMap<String, Value>,
    used: &mut HashSet<String>,
    counter: &mut usize,
) -> Option<Value> {
    let mut props = node.as_object().cloned().unwrap_or_default();
    props.remove("members");
    *counter += 1;
    let gid = props
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            props
                .get("caption")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| format!("__cp_g_{counter}"));
    let caption = props
        .get("caption")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut members: Vec<Value> = Vec::new();
    if let Some(ms) = node.get("members").and_then(|v| v.as_array()) {
        for m in ms {
            match m {
                Value::String(s) => {
                    if used.contains(s) {
                        continue;
                    }
                    if let Some(col) = by_id.get(s) {
                        members.push(col.clone());
                        used.insert(s.clone());
                    }
                }
                Value::Object(_) => {
                    if let Some(child) = build_group_node(m, by_id, used, counter) {
                        members.push(child);
                    }
                }
                _ => {}
            }
        }
    }
    if members.is_empty() {
        return None;
    }
    // CmxColumnGroup.toJSON 形状：__type + id/caption + aggregate(默认全 false) + aggregatePosition + members + 透传键。
    let aggregate = {
        let mut agg = Map::new();
        for k in ["sum", "avg", "max", "min", "count"] {
            agg.insert(k.to_string(), json!(false));
        }
        if let Some(o) = node.get("aggregate").and_then(|v| v.as_object()) {
            for (k, v) in o {
                agg.insert(k.clone(), v.clone());
            }
        }
        Value::Object(agg)
    };
    let aggregate_position = node
        .get("aggregatePosition")
        .cloned()
        .unwrap_or_else(|| json!("after"));

    let mut out = Map::new();
    out.insert("__type".to_string(), json!("CmxColumnGroup"));
    out.insert("id".to_string(), json!(gid));
    out.insert("caption".to_string(), json!(caption));
    out.insert("aggregate".to_string(), aggregate);
    out.insert("aggregatePosition".to_string(), aggregate_position);
    out.insert("members".to_string(), Value::Array(members));
    // 透传额外键（非 KNOWN）：props 已去掉 members，含作者写的其它分组属性
    const GROUP_KNOWN: &[&str] = &["id", "caption", "aggregate", "aggregatePosition", "members"];
    for (k, v) in &props {
        if GROUP_KNOWN.contains(&k.as_str()) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Some(Value::Object(out))
}
