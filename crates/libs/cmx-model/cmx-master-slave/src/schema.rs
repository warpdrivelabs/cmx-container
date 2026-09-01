//! 层级定义（中立视图）—— 对齐前端 `schema`（点分路径树）+ `relations` + `aggregations`。
//!
//! 由 DOC/DCT 的定义 JSON 解析而来，但协调器不认业务：只认路径、关系、汇总规则。

use crate::error::{MsError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 拓扑形状：异构 path-tree（A，DOC）或同构自引用（B，DCT）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    /// 形状 A：异构，每层一表，父子经跨表 FK（DOC `upper_id`）。
    PathTree,
    /// 形状 B：同构，单表自引用（DCT `parent_id`）。
    SelfRef { parent_field: String },
}

/// 服务端派生列名（形状 B 用）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedCols {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_no: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_leaf: Option<String>,
}

impl DerivedCols {
    pub fn any(&self) -> bool {
        self.full_path.is_some() || self.level_no.is_some() || self.is_leaf.is_some()
    }
}

/// 一层的定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerDef {
    /// 完整点分路径，如 `head.items.taxes`（A）或 `dict`（B）。
    pub path: String,
    /// 物理表名（协调器只透传，绝不硬编码）。
    pub table: String,
    /// 主键列名。
    #[serde(default = "default_pk")]
    pub pk: String,
    /// 指向父的 FK 列名。根层为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_key: Option<String>,
    /// 排序列，如 `line_no`（DOC）/ `sort_no`（DCT）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_key: Option<String>,
    /// 派生列名（形状 B）。
    #[serde(default)]
    pub derived: DerivedCols,
    /// 承接汇总结果的列名清单（可空）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agg_fields: Vec<String>,
}

fn default_pk() -> String {
    "id".to_string()
}

/// 父子关系（对齐前端 RelationDef / DOC RelationView）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationDef {
    pub parent: String,
    pub child: String,
    #[serde(default = "default_pk")]
    pub parent_key: String,
    #[serde(default = "default_child_key")]
    pub child_key: String,
}

fn default_child_key() -> String {
    "upper_id".to_string()
}

/// 汇总规则（与前端 `AggregationRule`、统一建模方案 `AggRule` 逐字对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggRule {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(rename = "toField")]
    pub to_field: String,
    pub agg: AggFn,
    #[serde(default)]
    pub scope: Scope,
}

/// 聚合函数（语义 = 前端 `AGG_FUNCS`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggFn {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

/// 汇总作用域（对齐前端 `_resolveSourcesForTarget`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Siblings,
    All,
}

/// 层级定义总集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierSchema {
    pub shape: Shape,
    pub layers: Vec<LayerDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aggregations: Vec<AggRule>,
}

impl HierSchema {
    /// 从 JSON 定义解析（协调器只吃已解析的中立视图）。
    pub fn from_json(v: &serde_json::Value) -> Result<Self> {
        let s: HierSchema =
            serde_json::from_value(v.clone()).map_err(|e| MsError::InvalidSchema(e.to_string()))?;
        s.validate()?;
        Ok(s)
    }

    /// 校验 schema 自洽：路径唯一、关系/汇总引用的路径都存在、汇总规则合法。
    pub fn validate(&self) -> Result<()> {
        let mut seen = BTreeMap::new();
        for l in &self.layers {
            if seen.insert(l.path.clone(), ()).is_some() {
                return Err(MsError::DuplicatePath(l.path.clone()));
            }
        }
        let known = |p: &str| seen.contains_key(p);
        for r in &self.relations {
            if !known(&r.parent) {
                return Err(MsError::UnknownPath(r.parent.clone()));
            }
            if !known(&r.child) {
                return Err(MsError::UnknownPath(r.child.clone()));
            }
        }
        for a in &self.aggregations {
            if a.to_field.is_empty() || a.from.is_empty() || a.to.is_empty() {
                return Err(MsError::InvalidRule(format!("requires from/to/toField: {:?}", a)));
            }
            if !known(&a.from) {
                return Err(MsError::UnknownPath(a.from.clone()));
            }
            if !known(&a.to) {
                return Err(MsError::UnknownPath(a.to.clone()));
            }
            if a.agg != AggFn::Count && a.field.is_none() {
                return Err(MsError::InvalidRule(format!(
                    "agg {:?} requires a field: {}->{}",
                    a.agg, a.from, a.to
                )));
            }
        }
        Ok(())
    }

    pub fn layer(&self, path: &str) -> Option<&LayerDef> {
        self.layers.iter().find(|l| l.path == path)
    }

    pub fn roots(&self) -> Vec<&LayerDef> {
        self.layers.iter().filter(|l| !l.path.contains('.')).collect()
    }

    /// 层的拓扑序（父在前）：按 path 段数升序。
    pub fn layer_order(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.layers.iter().map(|l| l.path.clone()).collect();
        paths.sort_by_key(|p| p.split('.').count());
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn voucher() -> serde_json::Value {
        json!({
            "shape": { "kind": "path_tree" },
            "layers": [
                { "path": "head", "table": "cv_header", "pk": "id" },
                { "path": "head.items", "table": "cv_acc_line", "child_key": "upper_id", "order_key": "line_no" },
                { "path": "head.items.taxes", "table": "cv_aux_line", "child_key": "upper_id" }
            ],
            "relations": [
                { "parent": "head", "child": "head.items", "child_key": "upper_id" }
            ],
            "aggregations": [
                { "from": "head.items", "to": "head", "field": "debit", "toField": "totalDebit", "agg": "sum" }
            ]
        })
    }

    #[test]
    fn parses_voucher() {
        let s = HierSchema::from_json(&voucher()).unwrap();
        assert_eq!(s.shape, Shape::PathTree);
        assert_eq!(s.layers.len(), 3);
        assert_eq!(s.layer("head.items").unwrap().table, "cv_acc_line");
        assert_eq!(s.layer_order(), vec!["head", "head.items", "head.items.taxes"]);
    }

    #[test]
    fn rejects_duplicate_path() {
        let mut v = voucher();
        v["layers"][1]["path"] = json!("head");
        assert!(matches!(HierSchema::from_json(&v), Err(MsError::DuplicatePath(_))));
    }

    #[test]
    fn rejects_unknown_agg_path() {
        let mut v = voucher();
        v["aggregations"][0]["to"] = json!("nope");
        assert!(matches!(HierSchema::from_json(&v), Err(MsError::UnknownPath(_))));
    }

    #[test]
    fn self_ref_parses() {
        let v = json!({
            "shape": { "kind": "self_ref", "parent_field": "parent_id" },
            "layers": [{ "path": "dict", "table": "cf_gl_account",
                "child_key": "parent_id", "order_key": "sort_no",
                "derived": { "full_path": "full_path", "level_no": "level_no", "is_leaf": "is_leaf" } }]
        });
        let s = HierSchema::from_json(&v).unwrap();
        assert!(matches!(s.shape, Shape::SelfRef { .. }));
        assert!(s.layer("dict").unwrap().derived.any());
    }
}
