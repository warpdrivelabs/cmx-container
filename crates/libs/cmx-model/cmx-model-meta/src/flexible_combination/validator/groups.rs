//! 分组校验：递归遍历分组成员，检查字段引用存在性与重复引用。

use std::collections::HashSet;

use serde_json::Value;

use super::{Diag, GROUP_AGG_KEYS, GROUP_AGG_POSITIONS, field_id};

/// 校验分组：递归遍历分组成员，检查字段引用存在性与重复引用。
pub(super) fn validate_groups(
    groups: Option<&Value>,
    fields: &[Value],
    r_path: &str,
    d: &mut Diag,
) {
    let Some(groups) = groups.filter(|v| !v.is_null()) else {
        return;
    };
    if !groups.is_array() {
        d.error(
            &format!("{r_path}.detail.groups"),
            "GROUPS_ARRAY_REQUIRED",
            "groups 必须是数组",
        );
        return;
    }
    // 上面已校验为数组并提前返回，此处安全解构。
    let groups = groups
        .as_array()
        .expect("invariant: groups checked is_array above");
    // 收集合法字段 id 集合
    let field_codes: HashSet<String> = fields
        .iter()
        .map(field_id)
        .filter(|s| !s.is_empty())
        .collect();
    let mut seen: HashSet<String> = HashSet::new();

    // 递归遍历分组树
    fn walk(
        node: &Value,
        path: &str,
        field_codes: &HashSet<String>,
        seen: &mut HashSet<String>,
        d: &mut Diag,
    ) {
        if !node.is_object() {
            d.error(path, "GROUP_OBJECT_REQUIRED", "分组节点必须是对象");
            return;
        }
        let members = match node.get("members").and_then(|v| v.as_array()) {
            Some(m) => m,
            None => {
                d.error(
                    &format!("{path}.members"),
                    "GROUP_MEMBERS_REQUIRED",
                    "分组缺少 members 数组",
                );
                return;
            }
        };
        // 分组节点自身属性校验（aggregate/aggregatePosition）
        validate_group_props(node, path, d);
        for (index, member) in members.iter().enumerate() {
            let m_path = format!("{path}.members.{index}");
            match member {
                // 字符串成员：按字段 id 校验存在性与重复引用
                Value::String(s) => {
                    if !field_codes.contains(s) {
                        d.error(
                            &m_path,
                            "GROUP_FIELD_UNKNOWN",
                            format!("分组引用了不存在的字段 {s}"),
                        );
                    } else if seen.contains(s) {
                        d.warn(
                            &m_path,
                            "GROUP_FIELD_DUPLICATE",
                            format!("字段 {s} 被多个分组引用，将只使用首次出现"),
                        );
                    } else {
                        seen.insert(s.clone());
                    }
                }
                // 对象成员：递归子分组
                _ => walk(member, &m_path, field_codes, seen, d),
            }
        }
    }
    for (i, g) in groups.iter().enumerate() {
        walk(
            g,
            &format!("{r_path}.detail.groups.{i}"),
            &field_codes,
            &mut seen,
            d,
        );
    }
}

/// 校验分组节点属性：aggregate 键值合法性 + aggregatePosition 取值。
fn validate_group_props(node: &Value, path: &str, d: &mut Diag) {
    if let Some(agg) = node.get("aggregate").filter(|v| !v.is_null()) {
        if !agg.is_object() {
            d.error(
                &format!("{path}.aggregate"),
                "GROUP_AGGREGATE_INVALID",
                "aggregate 必须是对象，如 { sum:true }",
            );
        } else if let Some(agg_obj) = agg.as_object() {
            for (k, v) in agg_obj {
                // 未知聚合键（警告级）
                if !GROUP_AGG_KEYS.contains(&k.as_str()) {
                    d.warn(
                        &format!("{path}.aggregate.{k}"),
                        "GROUP_AGGREGATE_KEY_UNKNOWN",
                        format!("未知聚合类型：{k}（支持 sum/avg/max/min/count）"),
                    );
                } else if !v.is_boolean() {
                    // 聚合键值须为布尔
                    d.error(
                        &format!("{path}.aggregate.{k}"),
                        "GROUP_AGGREGATE_VALUE_INVALID",
                        format!("aggregate.{k} 必须是布尔值"),
                    );
                }
            }
        }
    }
    // aggregatePosition 须为 before/after
    if let Some(pos) = node.get("aggregatePosition").and_then(|v| v.as_str())
        && !GROUP_AGG_POSITIONS.contains(&pos)
    {
        d.error(
            &format!("{path}.aggregatePosition"),
            "GROUP_AGGREGATE_POSITION_INVALID",
            format!("aggregatePosition 必须是 before/after，实际：{pos}"),
        );
    }
}
