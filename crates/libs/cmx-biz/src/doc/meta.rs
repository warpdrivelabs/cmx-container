//! DocMetaView — 业务单据定义（DOC）的强类型只读投影（方案 §3.8）
//!
//! 把 `definitions::store` 读入的弱类型 `serde_json::Value`（DOC 单据定义 + base 字段集）
//! 解析为强类型层级模型：层序 / relations(父子键) / 各层物理 Schema。
//!
//! 设计原则（方案 §3.8）：
//! - 只强类型化「层级骨架」（层序、父子键、表名、各层字段名/类型）；
//! - 字段「血肉」（30+ 属性）仍以按需读取为主，只取建 Schema 必需的 name/dataType。
//! - 不改存储格式，纯 serde 投影 + 一次解析。
//!
//! DocLoader / DocSaver 消费本视图，避免满地 `Value.get("...")`。

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;

use cmx_core::model::cell::{Field, FieldType};
use cmx_core::model::data::dataset::Schema;

use crate::{BizError, Result};

/// 一层（一张物理表）的视图。
#[derive(Debug, Clone)]
pub struct LayerView {
    /// schema 节点 id（= 逻辑层 id，如 `cv_batch`）
    pub id: String,
    /// 物理表名（= `voucherTables[i].tableName`）
    pub table_name: String,
    /// 层级标签（L1/L2/...）
    pub level: String,
    /// 该层完整列名（本表 fields + documentFieldSets 展开，去重，有序）
    pub columns: Vec<ColumnView>,
    /// 该层物理 Schema（Arc 共享，供装载零拷贝复用）
    pub schema: Arc<Schema>,
}

/// 一列的最小视图（只取建表/装载/回存必需的属性）。
#[derive(Debug, Clone)]
pub struct ColumnView {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

/// 父子关系（来自 `voucherSchema.relations`）。
#[derive(Debug, Clone)]
pub struct RelationView {
    pub parent: String,
    pub child: String,
    /// 父键（默认 "id"）
    pub parent_key: String,
    /// 子键（"upper_id" 或命名外键如 "header_id"）
    pub child_key: String,
}

/// 单据定义的强类型投影。
#[derive(Debug, Clone)]
pub struct DocMetaView {
    pub module_code: String,
    pub version: u64,
    /// 层序（自顶向下，L1..Ln）：schema id 列表
    pub layer_order: Vec<String>,
    /// 各层视图（与 layer_order 对齐，按 id 可查）
    pub layers: Vec<LayerView>,
    /// 父子关系
    pub relations: Vec<RelationView>,
    /// 校验规则（原始数组透传，§14.2）：[{ code, expr, message, severity, level }]
    pub validation_rules: Vec<serde_json::Value>,
    /// 状态机（原始透传，§14.1）：{ stateField, states:[{code,editable}], transitions:[...] }
    pub status_flow: Option<serde_json::Value>,
    /// 版本化开关（原始透传，§6A）：moduleMeta.versioning
    pub versioning: Option<serde_json::Value>,
}

impl DocMetaView {
    /// 某状态是否可编辑（§14.1）；无状态机或状态未声明时默认可编辑。
    pub fn is_state_editable(&self, state: &str) -> bool {
        let Some(sf) = &self.status_flow else {
            return true;
        };
        let Some(states) = sf.get("states").and_then(|v| v.as_array()) else {
            return true;
        };
        for s in states {
            if s.get("code").and_then(|v| v.as_str()) == Some(state) {
                return s.get("editable").and_then(|v| v.as_bool()).unwrap_or(true);
            }
        }
        true
    }

    /// 状态字段名（§14.1），如 "doc_status"。
    pub fn state_field(&self) -> Option<&str> {
        self.status_flow
            .as_ref()
            .and_then(|sf| sf.get("stateField"))
            .and_then(|v| v.as_str())
    }

    /// 版本化是否开启（§6A）。
    pub fn versioning_enabled(&self) -> bool {
        self.versioning
            .as_ref()
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
    /// 从 DOC 定义 doc + 其 base 字段集 base 解析。
    ///
    /// - `doc`：单据定义 JSON（含 moduleMeta / voucherSchema / voucherTables）
    /// - `base`：base 字段集 JSON（含 `fieldSets`），供 documentFieldSets 展开；无则传 `Value::Null`
    pub fn parse(doc: &Value, base: &Value) -> Result<Self> {
        let module_meta = doc.get("moduleMeta");
        let module_code = module_meta
            .and_then(|m| m.get("moduleCode"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = module_meta
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1);

        // 层序：voucherSchema.schema 是 [[{id,level,...}]] 嵌套数组，每个内层数组取首个节点 id
        let vs = doc
            .get("voucherSchema")
            .ok_or_else(|| BizError::business("单据定义缺少 voucherSchema"))?;
        let layer_order = parse_layer_order(vs);

        // relations
        let relations = parse_relations(vs);

        // voucherTables → 各层视图
        let tables = doc
            .get("voucherTables")
            .and_then(|v| v.as_array())
            .ok_or_else(|| BizError::business("单据定义缺少 voucherTables"))?;
        let mut layers = Vec::with_capacity(tables.len());
        for t in tables {
            layers.push(parse_layer(t, base)?);
        }

        Ok(DocMetaView {
            module_code,
            version,
            layer_order,
            layers,
            relations,
            validation_rules: doc
                .get("validationRules")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            status_flow: doc.get("voucherStatusFlow").cloned(),
            versioning: module_meta.and_then(|m| m.get("versioning")).cloned(),
        })
    }

    /// 按 schema id 或表名找层。
    pub fn layer(&self, id_or_table: &str) -> Option<&LayerView> {
        self.layers
            .iter()
            .find(|l| l.id == id_or_table || l.table_name == id_or_table)
    }

    /// 根层（层序首个）。
    pub fn root_layer(&self) -> Option<&LayerView> {
        self.layer_order.first().and_then(|id| self.layer(id))
    }

    /// 取某父层到子层的关系（按 parent id）。
    pub fn child_relations(&self, parent_id: &str) -> Vec<&RelationView> {
        self.relations
            .iter()
            .filter(|r| r.parent == parent_id)
            .collect()
    }
}

// ─────────────────────── 解析辅助 ───────────────────────

fn parse_layer_order(vs: &Value) -> Vec<String> {
    let mut order = Vec::new();
    if let Some(schema) = vs.get("schema").and_then(|v| v.as_array()) {
        for level_group in schema {
            // 每个 level_group 是一个数组 [{id,...}]，取首个节点 id
            let node = level_group
                .as_array()
                .and_then(|a| a.first())
                .or(Some(level_group)); // 容错：也接受直接对象
            if let Some(id) = node.and_then(|n| n.get("id")).and_then(|v| v.as_str()) {
                order.push(id.to_string());
            }
        }
    }
    order
}

fn parse_relations(vs: &Value) -> Vec<RelationView> {
    let mut out = Vec::new();
    if let Some(rels) = vs.get("relations").and_then(|v| v.as_array()) {
        for r in rels {
            let parent = r.get("parent").and_then(|v| v.as_str());
            let child = r.get("child").and_then(|v| v.as_str());
            if let (Some(parent), Some(child)) = (parent, child) {
                out.push(RelationView {
                    parent: parent.to_string(),
                    child: child.to_string(),
                    parent_key: r
                        .get("parentKey")
                        .and_then(|v| v.as_str())
                        .unwrap_or("id")
                        .to_string(),
                    child_key: r
                        .get("childKey")
                        .and_then(|v| v.as_str())
                        .unwrap_or("upper_id")
                        .to_string(),
                });
            }
        }
    }
    out
}

fn parse_layer(t: &Value, base: &Value) -> Result<LayerView> {
    let table_name = t
        .get("tableName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BizError::business("voucherTables 项缺少 tableName"))?
        .to_string();
    // schema id 约定 = tableName（cmxfico 里 schema.id 与 tableName 同名）
    let id = table_name.clone();
    let level = t
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 列 = 本表 fields + documentFieldSets 展开（去重）
    let mut columns: Vec<ColumnView> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        for f in own {
            if let Some(c) = parse_column(f) {
                if seen.insert(c.name.clone()) {
                    columns.push(c);
                }
            }
        }
    }
    if let Some(sets) = t.get("documentFieldSets").and_then(|v| v.as_array()) {
        for s in sets {
            if let Some(set_name) = s.as_str() {
                if let Some(fields) = base_fieldset(base, set_name) {
                    for f in fields {
                        if let Some(c) = parse_column(f) {
                            if seen.insert(c.name.clone()) {
                                columns.push(c);
                            }
                        }
                    }
                }
            }
        }
    }

    // 建 Schema（字段名 → FieldType）
    let fields: Vec<Field> = columns
        .iter()
        .map(|c| Field {
            name: c.name.clone(),
            field_type: map_field_type(&c.data_type),
            label: String::new(),
        })
        .collect();
    let schema = Arc::new(
        Schema::new(id.clone(), fields)
            .map_err(|e| BizError::business(format!("层 {table_name} Schema 构建失败: {e}")))?,
    );

    Ok(LayerView {
        id,
        table_name,
        level,
        columns,
        schema,
    })
}

fn parse_column(f: &Value) -> Option<ColumnView> {
    let name = f.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }
    let data_type = f
        .get("dataType")
        .and_then(|v| v.as_str())
        .unwrap_or("VARCHAR")
        .to_string();
    let nullable = f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
    // 主键：isPrimaryKey==1 或字段名为 "id"
    let is_primary_key = f
        .get("isPrimaryKey")
        .and_then(|v| v.as_i64())
        .map(|n| n == 1)
        .unwrap_or(false)
        || name == "id";
    Some(ColumnView {
        name,
        data_type,
        nullable,
        is_primary_key,
    })
}

/// 从 base 的 `fieldSets` 里取某字段集的 fields 数组。
fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}

/// 业务单据 dataType 字符串 → cmx-core FieldType（对齐 model_center.rs 的映射）。
fn map_field_type(data_type: &str) -> FieldType {
    match data_type.to_ascii_uppercase().as_str() {
        "VARCHAR" | "CHAR" | "TEXT" | "STRING" => FieldType::String,
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" => FieldType::Int,
        "DECIMAL" | "NUMERIC" => FieldType::Decimal,
        "FLOAT" | "DOUBLE" | "REAL" => FieldType::Float,
        "DATE" => FieldType::Date,
        "DATETIME" | "TIMESTAMP" | "TIMESTAMPTZ" => FieldType::DateTime,
        "BOOL" | "BOOLEAN" => FieldType::Bool,
        "JSON" | "JSONB" => FieldType::Json,
        "UUID" => FieldType::Uuid,
        "BYTEA" | "BINARY" | "BLOB" => FieldType::Binary,
        _ => FieldType::String,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_doc() -> Value {
        json!({
            "moduleMeta": { "moduleCode": "GL", "metaKind": "DOC", "version": 1 },
            "voucherSchema": {
                "schema": [
                    [ { "id": "cv_batch",  "level": "L1", "levelName": "凭证批" } ],
                    [ { "id": "cv_header", "level": "L2", "levelName": "凭证头" } ],
                    [ { "id": "cv_line",   "level": "L3", "levelName": "科目行" } ]
                ],
                "relations": [
                    { "parent": "cv_batch",  "child": "cv_header", "parentKey": "id", "childKey": "upper_id" },
                    { "parent": "cv_header", "child": "cv_line",   "parentKey": "id", "childKey": "upper_id" }
                ]
            },
            "voucherTables": [
                { "level": "L1", "tableName": "cv_batch",
                  "fields": [ { "name": "doc_no", "dataType": "VARCHAR", "nullable": true } ],
                  "documentFieldSets": [ "documentLevelFields" ] },
                { "level": "L2", "tableName": "cv_header",
                  "fields": [ { "name": "total_dr", "dataType": "DECIMAL", "decimalDigits": 2 } ],
                  "documentFieldSets": [ "documentLevelFields" ] },
                { "level": "L3", "tableName": "cv_line",
                  "fields": [ { "name": "amount", "dataType": "DECIMAL" } ],
                  "documentFieldSets": [ "documentLevelFields" ] }
            ]
        })
    }

    fn sample_base() -> Value {
        json!({
            "fieldSets": {
                "documentLevelFields": {
                    "fields": [
                        { "name": "id",       "dataType": "BIGINT", "nullable": false, "isPrimaryKey": 1 },
                        { "name": "upper_id", "dataType": "BIGINT", "nullable": true },
                        { "name": "line_no",  "dataType": "INT",    "nullable": false }
                    ]
                }
            }
        })
    }

    #[test]
    fn parses_layer_order_and_relations() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        assert_eq!(v.module_code, "GL");
        assert_eq!(v.layer_order, vec!["cv_batch", "cv_header", "cv_line"]);
        assert_eq!(v.relations.len(), 2);
        assert_eq!(v.relations[0].child_key, "upper_id");
        assert_eq!(v.relations[0].parent_key, "id");
        assert_eq!(v.root_layer().unwrap().table_name, "cv_batch");
    }

    #[test]
    fn merges_base_fieldsets_and_builds_schema() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        let batch = v.layer("cv_batch").unwrap();
        // 列 = doc_no(本表) + id/upper_id/line_no(base)
        let names: Vec<&str> = batch.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"doc_no"));
        assert!(names.contains(&"id"));
        assert!(names.contains(&"upper_id"));
        assert!(names.contains(&"line_no"));
        // id 是主键
        assert!(batch.columns.iter().any(|c| c.name == "id" && c.is_primary_key));
        // Schema 建成，字段数一致
        assert_eq!(batch.schema.field_count(), batch.columns.len());
        // id 列类型为 Int（BIGINT→Int）
        assert_eq!(batch.schema.get_index("id").is_some(), true);
    }

    #[test]
    fn child_relations_lookup() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        let ch = v.child_relations("cv_batch");
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].child, "cv_header");
    }

    #[test]
    fn state_machine_and_versioning_helpers() {
        let mut doc = sample_doc();
        doc["moduleMeta"]["versioning"] = json!({ "enabled": true });
        doc["voucherStatusFlow"] = json!({
            "stateField": "doc_status",
            "states": [
                { "code": "draft", "editable": true },
                { "code": "posted", "editable": false }
            ]
        });
        let v = DocMetaView::parse(&doc, &sample_base()).unwrap();
        assert_eq!(v.state_field(), Some("doc_status"));
        assert!(v.is_state_editable("draft"));
        assert!(!v.is_state_editable("posted"));       // 过账不可编辑（§14.1 铁律）
        assert!(v.is_state_editable("unknown"));        // 未声明默认可编辑
        assert!(v.versioning_enabled());
        assert_eq!(v.validation_rules.len(), 0);        // sample 无 validationRules
    }
}
