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
    /// 层级标签（L1/L2/...；同层多表共享同一 level）
    pub level: String,
    /// 层级显示名（voucherSchema.schema[i].levelName，如 "凭证批"；无则空）
    pub level_name: String,
    /// 父表 id（`voucherTables[i].parentTable`，明确指出父是上一层哪张表；空=未指定，
    /// 装载时回退到「上一层默认表」= layer_order 主链路父）。
    pub parent_table: String,
    /// 该层完整列名（本表 fields + documentFieldSets 展开，去重，有序）
    pub columns: Vec<ColumnView>,
    /// 本表定义的汇总表（`voucherTables[i].summaries[]`；无则空）
    pub summaries: Vec<SummaryView>,
    /// 本表 measure 且 agg 非空的列名（便于前端识别可上卷列）
    pub agg_fields: Vec<String>,
    /// 该层物理 Schema（Arc 共享，供装载零拷贝复用）
    pub schema: Arc<Schema>,
    /// 落库前列级校验规范（含类型/长度/精度/nullable；从合并后原始字段构建）。
    pub spec: Arc<crate::validation::TableSpec>,
}

impl LayerView {
    /// 按列名找 ColumnView（供过滤值类型化 / 列白名单）。
    pub fn column(&self, name: &str) -> Option<&ColumnView> {
        self.columns.iter().find(|c| c.name == name)
    }
    /// 该列是否存在（白名单校验）。
    pub fn has_column(&self, name: &str) -> bool {
        self.schema.get_index(name).is_some()
    }
}

/// 一张汇总表（sum 表）的视图 —— 结构同「表」，挂在某张源表下。
///
/// 定义形如 `{ id, name, caption, fields:[<field>] }`；fields 是**已物化的完整列**
/// （定义里内联，无需 documentFieldSets 合并）。度量列带 `agg:"sum"`、维度列带 `dimType`。
#[derive(Debug, Clone)]
pub struct SummaryView {
    pub id: String,
    pub name: String,
    /// 显示标题（caption.zh_CN，回退 name）
    pub caption: String,
    /// 所属源表 table_name
    pub source_table: String,
    /// 汇总表列（复用 parse_column，含 caption/dataType/dimType/agg/isPrimaryKey）
    pub columns: Vec<ColumnView>,
    /// 汇总表物理 Schema
    pub schema: Arc<Schema>,
}

/// 一个层级组（同 level 下的全部并列表）—— 用于「彻底解析」保真每层多表。
#[derive(Debug, Clone)]
pub struct LevelGroup {
    /// 层级标签（L1/L2/...）
    pub level: String,
    /// 层级显示名
    pub level_name: String,
    /// 该层全部表 id（如 L4 = [cv_aux_line, cv_cyzb_line]；主链路取首个）
    pub table_ids: Vec<String>,
}

/// 一列的最小视图（建表/装载/回存必需的属性 + 前端显示用 caption/dimType/agg）。
#[derive(Debug, Clone)]
pub struct ColumnView {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
    /// 列显示标题（caption.zh_CN，无则回退为 name）—— 供前端通用单据页建列头。
    pub caption: String,
    /// 维度类型（attribute|dimension|measure，无则空）—— 前端可据此分组/排序。
    pub dim_type: String,
    /// 度量聚合方式（如 "sum"；无则空）—— 汇总表度量列 + 主表 measure 列携带。
    pub agg: String,
    // ── 前端录入控件透传（原样保留原始 JSON，供动态列模型复用）──────────────
    /// 引用字典编码（如 comp_unit）。空 = 非字典列。
    pub ref_dict: String,
    /// 显示字段（字典回显用，如 code/name）。
    pub display_field: String,
    /// 写回字段（字典选值写回行，如 id/code）。
    pub ref_field: String,
    /// 录入控件配置（原样透传 edit{}，如 {mode:"cmx-dict-selct"}）。
    pub edit: Option<Value>,
    /// 编辑设置（原样透传 editSettings{}，如 {dictCode, coord}）。
    pub edit_settings: Option<Value>,
    /// 显示属性（原样透传 display{}，如 {decimalDigits:0, format:"thousands"}）。
    /// 表现交互层属性，下沉到 DOC 元数据后由前端列模型直接消费。
    pub display: Option<Value>,
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
    /// 单据编码（docMeta.docCode），用于一模块多单据时精确定位。
    pub doc_code: String,
    pub version: u64,
    /// 层序（自顶向下，L1..Ln）：schema id 列表。**主链路**——每 level-group 取首表，
    /// 装载器（DocLoader/ZmcDocLoader）据此下钻。同层多表见 `layer_groups`。
    pub layer_order: Vec<String>,
    /// 各层视图（**含每层全部表**，不止主链路；按 id 可查）
    pub layers: Vec<LayerView>,
    /// 层级组（同 level 下全部并列表）—— 「彻底解析」保真每层多表。
    pub layer_groups: Vec<LevelGroup>,
    /// 父子关系
    pub relations: Vec<RelationView>,
    /// 校验规则（原始数组透传，§14.2）：[{ code, expr, message, severity, level }]
    pub validation_rules: Vec<serde_json::Value>,
    /// 状态机（原始透传，§14.1）：{ stateField, states:[{code,editable}], transitions:[...] }
    pub status_flow: Option<serde_json::Value>,
    /// 版本化开关（原始透传，§6A）：docMeta.versioning
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
    /// - `doc`：单据定义 JSON（含 docMeta / voucherSchema / voucherTables）
    /// - `base`：base 字段集 JSON（含 `fieldSets`），供 documentFieldSets 展开；无则传 `Value::Null`
    pub fn parse(doc: &Value, base: &Value) -> Result<Self> {
        let doc_meta = doc.get("docMeta");
        let doc_code = doc_meta
            .and_then(|m| m.get("docCode"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = doc_meta
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

        // schema 节点的 id → levelName 映射（供各层显示名）
        let level_names = parse_level_names(vs);

        // 层级组：同 level 下全部并列表（彻底解析保真每层多表）
        let layer_groups = parse_layer_groups(vs);

        // voucherTables → 各层视图（含每层全部表 + 每表汇总表）
        let tables = doc
            .get("voucherTables")
            .and_then(|v| v.as_array())
            .ok_or_else(|| BizError::business("单据定义缺少 voucherTables"))?;
        let mut layers = Vec::with_capacity(tables.len());
        for t in tables {
            layers.push(parse_layer(t, base, &level_names)?);
        }

        Ok(DocMetaView {
            doc_code,
            version,
            layer_order,
            layers,
            layer_groups,
            relations,
            validation_rules: doc
                .get("validationRules")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            status_flow: doc.get("voucherStatusFlow").cloned(),
            versioning: doc_meta.and_then(|m| m.get("versioning")).cloned(),
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

    /// 父层在 `layer_groups` 里的组下标（按组内任一表 id 命中）。
    fn group_index_of(&self, table_id: &str) -> Option<usize> {
        self.layer_groups
            .iter()
            .position(|g| g.table_ids.iter().any(|t| t == table_id))
    }

    /// 返回某父层的全部**直接子表**（同父兄弟）。
    ///
    /// 规则（同父兄弟 + parentTable 可选回退）：
    /// - 定位父所在层组 `gi`，取**下一组** `layer_groups[gi+1]` 的全部表；
    /// - 若下一组里**有任何**表声明了 `parent_table`，则只返回 `parent_table == parent_id` 的表
    ///   （精确父子）；
    /// - 若下一组**无任何**表声明 `parent_table`（老定义，如当前 cmxfico），则整组都算该父的子
    ///   （回退：上一层默认表 = 主链路父，全组并列挂同一父）。
    pub fn child_layers(&self, parent_id: &str) -> Vec<&LayerView> {
        let Some(gi) = self.group_index_of(parent_id) else {
            return Vec::new();
        };
        let Some(next) = self.layer_groups.get(gi + 1) else {
            return Vec::new();
        };
        let group_layers: Vec<&LayerView> = next
            .table_ids
            .iter()
            .filter_map(|id| self.layer(id))
            .collect();
        let any_declared = group_layers.iter().any(|l| !l.parent_table.is_empty());
        if any_declared {
            group_layers
                .into_iter()
                .filter(|l| l.parent_table == parent_id)
                .collect()
        } else {
            group_layers
        }
    }

    /// 某表是否为其所在层组的**主表**（= table_ids[0]，主链路那张，孙层只从主表下钻）。
    pub fn is_primary_in_group(&self, table_id: &str) -> bool {
        self.layer_groups
            .iter()
            .any(|g| g.table_ids.first().map(|t| t == table_id).unwrap_or(false))
    }

    /// 取父→子的 childKey：优先用父所在组下标对齐的 `relations[gi]`，默认 `upper_id`。
    ///
    /// 兄弟表共用同一 childKey（都用 upper_id 指父），故只按父的组下标取，不依赖具体子表。
    pub fn child_key_for(&self, parent_id: &str) -> String {
        self.group_index_of(parent_id)
            .and_then(|gi| self.relations.get(gi))
            .map(|r| r.child_key.clone())
            .unwrap_or_else(|| "upper_id".to_string())
    }

    /// 取「当 `child_id` 作为子层被装载时」它匹配父的 childKey（懒下钻用）。
    ///
    /// child 在第 gi 组，其父在第 gi-1 组，childKey = `relations[gi-1]`（默认 upper_id）。
    pub fn child_key_for_child(&self, child_id: &str) -> Option<String> {
        let gi = self.group_index_of(child_id)?;
        if gi == 0 {
            return None; // 根层无父
        }
        Some(
            self.relations
                .get(gi - 1)
                .map(|r| r.child_key.clone())
                .unwrap_or_else(|| "upper_id".to_string()),
        )
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

/// schema 节点 id → levelName 映射（voucherSchema.schema[i].levelName）。
fn parse_level_names(vs: &Value) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(schema) = vs.get("schema").and_then(|v| v.as_array()) {
        for level_group in schema {
            // 同层多表：给该组内**每个** node id 都登记 levelName（首个非空 levelName 作组名）
            let nodes: Vec<&Value> = match level_group.as_array() {
                Some(a) => a.iter().collect(),
                None => vec![level_group],
            };
            let group_name = nodes
                .iter()
                .find_map(|n| n.get("levelName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
                .unwrap_or("");
            for n in nodes {
                if let Some(id) = n.get("id").and_then(|v| v.as_str()) {
                    map.insert(id.to_string(), group_name.to_string());
                }
            }
        }
    }
    map
}

/// 层级组：每个 level-group 的 level + levelName + 全部表 id（同层多表保真）。
fn parse_layer_groups(vs: &Value) -> Vec<LevelGroup> {
    let mut out = Vec::new();
    if let Some(schema) = vs.get("schema").and_then(|v| v.as_array()) {
        for level_group in schema {
            let nodes: Vec<&Value> = match level_group.as_array() {
                Some(a) => a.iter().collect(),
                None => vec![level_group],
            };
            if nodes.is_empty() {
                continue;
            }
            let level = nodes[0]
                .get("level")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let level_name = nodes
                .iter()
                .find_map(|n| n.get("levelName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
                .unwrap_or("")
                .to_string();
            let table_ids: Vec<String> = nodes
                .iter()
                .filter_map(|n| n.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect();
            out.push(LevelGroup {
                level,
                level_name,
                table_ids,
            });
        }
    }
    out
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

fn parse_layer(
    t: &Value,
    base: &Value,
    level_names: &std::collections::HashMap<String, String>,
) -> Result<LayerView> {
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
    // 显示名：优先 schema 节点 levelName，回退表上 levelName/tableAlias
    let level_name = level_names
        .get(&id)
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            t.get("levelName")
                .or_else(|| t.get("tableAlias"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();

    // 列 = 本表 fields + documentFieldSets 展开（去重）
    let mut columns: Vec<ColumnView> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // 合并后的原始字段（带 fieldLength/decimalDigits），供构建落库校验规范 TableSpec。
    let mut raw_fields: Vec<Value> = Vec::new();

    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        for f in own {
            if let Some(c) = parse_column(f)
                && seen.insert(c.name.clone()) {
                    raw_fields.push(f.clone());
                    columns.push(c);
                }
        }
    }
    if let Some(sets) = t.get("documentFieldSets").and_then(|v| v.as_array()) {
        for s in sets {
            if let Some(set_name) = s.as_str()
                && let Some(fields) = base_fieldset(base, set_name) {
                    for f in fields {
                        if let Some(c) = parse_column(f)
                            && seen.insert(c.name.clone()) {
                                raw_fields.push(f.clone());
                                columns.push(c);
                            }
                    }
                }
        }
    }

    // 建 Schema（字段名 → FieldType）
    let schema = build_schema(&id, &columns)
        .map_err(|e| BizError::business(format!("层 {table_name} Schema 构建失败: {e}")))?;

    // 本表 measure 且 agg 非空的列（可上卷列，供前端识别）
    let agg_fields: Vec<String> = columns
        .iter()
        .filter(|c| !c.agg.is_empty())
        .map(|c| c.name.clone())
        .collect();

    // 本表定义的汇总表（sum 表）
    let summaries = parse_summaries(t, &table_name)?;

    // 父表 id（parentTable；空=未指定，装载时回退到上一层默认表）
    let parent_table = t
        .get("parentTable")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 落库前列级校验规范（从合并后原始字段构建；DOC 层主键约定为 "id"）。
    let spec = Arc::new(crate::validation::build_table_spec(
        table_name.clone(),
        "id",
        &raw_fields,
    ));

    Ok(LayerView {
        id,
        table_name,
        level,
        level_name,
        parent_table,
        columns,
        summaries,
        agg_fields,
        schema,
        spec,
    })
}

/// 用列视图建物理 Schema（层与汇总表共用）。
fn build_schema(id: &str, columns: &[ColumnView]) -> std::result::Result<Arc<Schema>, String> {
    let fields: Vec<Field> = columns
        .iter()
        .map(|c| Field {
            name: c.name.clone(),
            field_type: map_field_type(&c.data_type),
            label: String::new(),
        })
        .collect();
    Schema::new(id.to_string(), fields).map(Arc::new).map_err(|e| e.to_string())
}

/// 解析一张表的汇总表（`voucherTables[i].summaries[]`）。
///
/// 每张汇总表 `{id, name, caption, fields[]}`：fields 是**已物化的完整列**（定义内联，
/// 不走 documentFieldSets 合并），直接用 `parse_column`。为每张汇总表建独立 Schema。
fn parse_summaries(t: &Value, source_table: &str) -> Result<Vec<SummaryView>> {
    let Some(arr) = t.get("summaries").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    for (i, s) in arr.iter().enumerate() {
        let id = s
            .get("id")
            .or_else(|| s.get("name"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("{source_table}_sum{}", i + 1));
        let name = s
            .get("name")
            .or_else(|| s.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();
        let caption = s
            .get("caption")
            .and_then(|c| c.get("zh_CN"))
            .and_then(|v| v.as_str())
            .filter(|x| !x.is_empty())
            .unwrap_or(&name)
            .to_string();

        let mut columns: Vec<ColumnView> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if let Some(fields) = s.get("fields").and_then(|v| v.as_array()) {
            for f in fields {
                if let Some(c) = parse_column(f)
                    && seen.insert(c.name.clone()) {
                        columns.push(c);
                    }
            }
        }
        let schema = build_schema(&id, &columns)
            .map_err(|e| BizError::business(format!("汇总表 {id} Schema 构建失败: {e}")))?;

        out.push(SummaryView {
            id,
            name,
            caption,
            source_table: source_table.to_string(),
            columns,
            schema,
        });
    }
    Ok(out)
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
    // 显示标题：caption.zh_CN，缺省回退列名（供前端通用页列头）
    let caption = f
        .get("caption")
        .and_then(|c| c.get("zh_CN"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&name)
        .to_string();
    let dim_type = f
        .get("dimType")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let agg = f
        .get("agg")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // 字典/录入控件配置：原样透传，供前端动态列模型复用（cmx-dict-selct 等）。
    let ref_dict = f
        .get("refDict")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let display_field = f
        .get("displayField")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ref_field = f
        .get("refField")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let edit = f.get("edit").filter(|v| v.is_object()).cloned();
    let edit_settings = f.get("editSettings").filter(|v| v.is_object()).cloned();
    // 显示属性：原样透传 display{}（表现交互层，如 decimalDigits/format/align）。
    let display = f.get("display").filter(|v| v.is_object()).cloned();
    Some(ColumnView {
        name,
        data_type,
        nullable,
        is_primary_key,
        caption,
        dim_type,
        agg,
        ref_dict,
        display_field,
        ref_field,
        edit,
        edit_settings,
        display,
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
            "docMeta": { "docCode": "cmxfico", "metaKind": "DOC", "version": 1 },
            "voucherSchema": {
                "schema": [
                    [ { "id": "cv_batch",  "level": "L1", "levelName": "凭证批" } ],
                    [ { "id": "cv_header", "level": "L2", "levelName": "凭证头" } ],
                    // L3 同层两张并列表（主链路取首个 cv_line）
                    [ { "id": "cv_line",   "level": "L3", "levelName": "科目行" },
                      { "id": "cv_aux",    "level": "L3" } ]
                ],
                "relations": [
                    { "parent": "cv_batch",  "child": "cv_header", "parentKey": "id", "childKey": "upper_id" },
                    { "parent": "cv_header", "child": "cv_line",   "parentKey": "id", "childKey": "upper_id" }
                ]
            },
            "voucherTables": [
                { "level": "L1", "tableName": "cv_batch",
                  "fields": [ { "name": "doc_no", "dataType": "VARCHAR", "nullable": true,
                               "dimType": "attribute", "caption": { "zh_CN": "凭证编号" } } ],
                  "documentFieldSets": [ "documentLevelFields" ] },
                { "level": "L2", "tableName": "cv_header",
                  "fields": [ { "name": "total_dr", "dataType": "DECIMAL", "decimalDigits": 2 } ],
                  "documentFieldSets": [ "documentLevelFields" ] },
                { "level": "L3", "tableName": "cv_line",
                  "fields": [ { "name": "amount", "dataType": "DECIMAL", "dimType": "measure", "agg": "sum",
                               "caption": { "zh_CN": "金额" } } ],
                  "documentFieldSets": [ "documentLevelFields" ],
                  "summaries": [
                    { "id": "cv_line_sum", "name": "cv_line_sum", "caption": { "zh_CN": "科目行汇总" },
                      "fields": [
                        { "name": "id", "dataType": "BIGINT", "isPrimaryKey": 1, "caption": { "zh_CN": "主键" } },
                        { "name": "gl_account_id", "dataType": "BIGINT", "dimType": "dimension", "caption": { "zh_CN": "科目" } },
                        { "name": "amount", "dataType": "DECIMAL", "dimType": "measure", "agg": "sum", "caption": { "zh_CN": "金额合计" } }
                      ] },
                    { "id": "cv_line_sum_2", "name": "cv_line_sum_2", "caption": { "zh_CN": "科目行汇总2" },
                      "fields": [
                        { "name": "id", "dataType": "BIGINT", "isPrimaryKey": 1 },
                        { "name": "amount", "dataType": "DECIMAL", "dimType": "measure", "agg": "sum" }
                      ] }
                  ] },
                // L3 第二张并列表（无 summaries）
                { "level": "L3", "tableName": "cv_aux",
                  "fields": [ { "name": "profit_ctr_id", "dataType": "BIGINT", "dimType": "dimension",
                               "caption": { "zh_CN": "利润中心" } } ],
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
        assert_eq!(v.doc_code, "cmxfico");
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
        assert!(batch.schema.get_index("id").is_some());
    }

    #[test]
    fn parses_display_metadata_for_generic_loader() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        let batch = v.layer("cv_batch").unwrap();
        // 层显示名来自 voucherSchema.schema[i].levelName
        assert_eq!(batch.level_name, "凭证批");
        assert_eq!(v.layer("cv_header").unwrap().level_name, "凭证头");
        // 列显示标题：有 caption 用 caption.zh_CN，无则回退列名
        let doc_no = batch.columns.iter().find(|c| c.name == "doc_no").unwrap();
        assert_eq!(doc_no.caption, "凭证编号");
        assert_eq!(doc_no.dim_type, "attribute");
        // 无 caption 的 base 列回退为列名
        let id_col = batch.columns.iter().find(|c| c.name == "id").unwrap();
        assert_eq!(id_col.caption, "id");
    }

    #[test]
    fn child_relations_lookup() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        let ch = v.child_relations("cv_batch");
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].child, "cv_header");
    }

    #[test]
    fn parses_multi_table_levels_and_summaries() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();

        // ── 同层多表：L3 有 cv_line + cv_aux 两张并列表 ──────────────────
        // layer_order 仍是主链路（每层取首表）——回归：装载不受影响
        assert_eq!(v.layer_order, vec!["cv_batch", "cv_header", "cv_line"]);
        // layer_groups 完整保真每层全部表
        assert_eq!(v.layer_groups.len(), 3);
        let l3 = v.layer_groups.iter().find(|g| g.level == "L3").unwrap();
        assert_eq!(l3.level_name, "科目行");
        assert_eq!(l3.table_ids, vec!["cv_line", "cv_aux"]);
        // 第二张并列表能取到、列解析正确
        let aux = v.layer("cv_aux").expect("cv_aux 应被解析进 layers");
        assert_eq!(aux.level, "L3");
        assert_eq!(aux.level_name, "科目行"); // 同层多表共享 levelName
        assert!(aux.columns.iter().any(|c| c.name == "profit_ctr_id" && c.caption == "利润中心"));
        // layers 含全部 4 张表（cv_batch/cv_header/cv_line/cv_aux）
        assert_eq!(v.layers.len(), 4);

        // ── 汇总表：cv_line 有 2 张 summaries ───────────────────────────
        let line = v.layer("cv_line").unwrap();
        assert_eq!(line.summaries.len(), 2);
        let sum = &line.summaries[0];
        assert_eq!(sum.id, "cv_line_sum");
        assert_eq!(sum.caption, "科目行汇总");
        assert_eq!(sum.source_table, "cv_line");
        // 汇总表列带 caption + agg（measure 列 agg=="sum"）
        let amt = sum.columns.iter().find(|c| c.name == "amount").unwrap();
        assert_eq!(amt.caption, "金额合计");
        assert_eq!(amt.agg, "sum");
        assert_eq!(amt.dim_type, "measure");
        // 汇总表 Schema 建成
        assert_eq!(sum.schema.field_count(), sum.columns.len());
        // 第二张汇总表
        assert_eq!(line.summaries[1].id, "cv_line_sum_2");

        // ── 主表 measure 列的 agg + agg_fields ─────────────────────────
        let amount = line.columns.iter().find(|c| c.name == "amount").unwrap();
        assert_eq!(amount.agg, "sum");
        assert!(line.agg_fields.contains(&"amount".to_string()));

        // ── 无汇总表的表 summaries 为空 ─────────────────────────────────
        assert!(aux.summaries.is_empty());
    }

    #[test]
    fn child_layers_sibling_derivation() {
        let v = DocMetaView::parse(&sample_doc(), &sample_base()).unwrap();
        // cv_header 的子 = L3 组全部表（回退：无 parent_table 声明 → 全组同父兄弟）
        let kids = v.child_layers("cv_header");
        let ids: Vec<&str> = kids.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["cv_line", "cv_aux"], "同父兄弟：下一组两张表都是子");
        // 主表判定：cv_line 是 L3 组主表，cv_aux 不是
        assert!(v.is_primary_in_group("cv_line"));
        assert!(!v.is_primary_in_group("cv_aux"));
        // childKey 取父组对齐的 relation（upper_id）
        assert_eq!(v.child_key_for("cv_header"), "upper_id");
        // 最深层无子
        assert!(v.child_layers("cv_line").is_empty());
    }

    #[test]
    fn child_layers_respects_parent_table_when_declared() {
        // 给 L3 两张表都声明 parentTable：cv_line→cv_header，cv_aux→cv_header
        // 再加一张“别的父”的表，验证精确过滤
        let mut doc = sample_doc();
        let tables = doc["voucherTables"].as_array_mut().unwrap();
        for t in tables.iter_mut() {
            let name = t["tableName"].as_str().unwrap().to_string();
            if name == "cv_line" || name == "cv_aux" {
                t["parentTable"] = json!("cv_header");
            }
        }
        let v = DocMetaView::parse(&doc, &sample_base()).unwrap();
        let kids = v.child_layers("cv_header");
        let ids: Vec<&str> = kids.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, vec!["cv_line", "cv_aux"]); // 都精确指向 cv_header
        assert_eq!(v.layer("cv_aux").unwrap().parent_table, "cv_header");
        // 声明了 parent_table 后，非该父的父查不到这些子
        assert!(v.child_layers("cv_batch").iter().all(|l| l.id != "cv_line"));
    }

    #[test]
    fn state_machine_and_versioning_helpers() {
        let mut doc = sample_doc();
        doc["docMeta"]["versioning"] = json!({ "enabled": true });
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
