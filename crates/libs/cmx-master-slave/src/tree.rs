//! 内存树 `MsTree` —— 协调器持有的递归主从数据树。
//!
//! 装载态数据来自 [`ZmcDataSet`](cmx_rowsource::ZmcDataSet)（零拷贝，与前端 wire 同构）；
//! 入树时转成可变 JSON 行（[`from_zmc`](MsTree::from_zmc)），以便汇总回写承接字段。
//! 用 arena（`Vec<Node>` + 索引）避免 `Rc<RefCell>`。
//!
//! 对齐前端 `_data.tables` + 每行 `_children` 的递归结构，以及 `_collectRows`/`_descendFromRow`
//! 的按路径收集。

use cmx_rowsource::{ZmcColType, ZmcDataSet, ZmcRowSource};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// 节点在 arena 中的索引。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

/// 一个层级节点：一行 JSON 数据 + 父子指针 + 所属层路径。
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    /// 业务行主键的字符串形式（稳定跨保存）。
    pub key: String,
    /// 所属层的完整点分路径。
    pub path: String,
    /// 行字段（JSON 值层，可变——汇总回写用）。
    pub row: Map<String, Value>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

impl Node {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.row.get(field)
    }
    pub fn set(&mut self, field: &str, value: Value) {
        self.row.insert(field.to_string(), value);
    }
}

/// 协调器的中立层级树。
#[derive(Debug, Clone, Default)]
pub struct MsTree {
    nodes: Vec<Node>,
    roots: Vec<NodeId>,
}

impl MsTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加节点，返回 NodeId（不自动挂父）。
    pub fn add(
        &mut self,
        key: impl Into<String>,
        path: impl Into<String>,
        row: Map<String, Value>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            id,
            key: key.into(),
            path: path.into(),
            row,
            parent: None,
            children: Vec::new(),
        });
        id
    }

    pub fn add_root(&mut self, id: NodeId) {
        self.roots.push(id);
    }

    pub fn link(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[child.0].parent = Some(parent);
        self.nodes[parent.0].children.push(child);
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }

    /// 收集某条完整路径下的所有节点（对齐前端 `_collectRows`）。
    pub fn collect_path(&self, path: &str) -> Vec<NodeId> {
        let segs: Vec<&str> = path.split('.').collect();
        if segs.is_empty() {
            return Vec::new();
        }
        let mut cursor: Vec<NodeId> = self
            .roots
            .iter()
            .copied()
            .filter(|r| self.nodes[r.0].path == segs[0])
            .collect();
        for i in 1..segs.len() {
            let want = segs[..=i].join(".");
            let mut next = Vec::new();
            for &p in &cursor {
                for &c in &self.nodes[p.0].children {
                    if self.nodes[c.0].path == want {
                        next.push(c);
                    }
                }
            }
            cursor = next;
        }
        cursor
    }

    /// 从单节点沿相对子路径下降（对齐前端 `_descendFromRow`）。
    pub fn descend_from(&self, start: NodeId, rel_segs: &[&str]) -> Vec<NodeId> {
        let mut cursor = vec![start];
        let mut acc = self.nodes[start.0].path.clone();
        for seg in rel_segs {
            acc = format!("{acc}.{seg}");
            let mut next = Vec::new();
            for &p in &cursor {
                for &c in &self.nodes[p.0].children {
                    if self.nodes[c.0].path == acc {
                        next.push(c);
                    }
                }
            }
            cursor = next;
        }
        cursor
    }

    /// 从一棵 [`ZmcDataSet`] 递归装配 `MsTree`。
    ///
    /// ZmcDataSet 的 `children: Vec<ZmcChildGroup>` 已按 childKey 分好组、每子行带 `parent_ids`——
    /// 直接据此建父子。`root_path` 是根层的点分路径；子层路径 = 父路径 + "." + childGroup.child_key
    /// （前端 `_children[childId]` 的 childId 即此）。`pk` 用于取每行业务键。
    ///
    /// 适用**形状 A**（DOC：异构 path-tree，childRows 嵌套）。
    pub fn from_zmc<R: ZmcRowSource>(zmc: &ZmcDataSet<R>, root_path: &str, pk: &str) -> Self {
        let mut tree = MsTree::new();
        let root_index = ingest_layer(&mut tree, zmc, root_path, pk, None);
        for &id in root_index.values() {
            tree.add_root(id);
        }
        ingest_children(&mut tree, zmc, root_path, &root_index);
        tree
    }

    /// 从一棵**扁平自引用** [`ZmcDataSet`] 装配 `MsTree`（**形状 B**，DCT：单表 `parent_id`）。
    ///
    /// 数据集是平铺一层（无 childRows），层内经 `parent_field` 自引用成树。所有行同属 `path`，
    /// 无父/父不在集内 → 兜底为根（对齐前端 `buildTreeFromFlat` 防丢数据）。
    pub fn from_zmc_self_ref<R: ZmcRowSource>(
        zmc: &ZmcDataSet<R>,
        path: &str,
        pk: &str,
        parent_field: &str,
    ) -> Self {
        let mut tree = MsTree::new();
        let pk_idx = zmc.schema.col_index(pk);
        let pf_idx = zmc.schema.col_index(parent_field);
        // 先建全部节点，记 (业务键 → NodeId)
        let mut by_key: HashMap<String, NodeId> = HashMap::new();
        let mut order: Vec<(usize, String, Option<String>)> = Vec::with_capacity(zmc.row_count());
        for r in 0..zmc.row_count() {
            let row = row_to_json(zmc, r);
            let key = pk_idx.and_then(|ci| zmc.row_key_string(r, ci)).unwrap_or_default();
            let parent = pf_idx.and_then(|ci| zmc.row_key_string(r, ci));
            let id = tree.add(key.clone(), path.to_string(), row);
            by_key.insert(key.clone(), id);
            order.push((id.0, key, parent));
        }
        // 再连边
        for (idx, _key, parent) in &order {
            let id = NodeId(*idx);
            match parent {
                Some(pk) if !pk.is_empty() => {
                    if let Some(&pid) = by_key.get(pk) {
                        tree.link(pid, id);
                    } else {
                        tree.add_root(id); // 父不在集内 → 兜底为根
                    }
                }
                _ => tree.add_root(id), // 无父 → 根
            }
        }
        tree
    }
}

/// 把一层 ZmcDataSet 的行入树，返回该层 (业务键 → NodeId)。
/// 若 `parent_of` 给出，则每行按其 index 对应的父 NodeId 挂上。
fn ingest_layer<R: ZmcRowSource>(
    tree: &mut MsTree,
    ds: &ZmcDataSet<R>,
    path: &str,
    pk: &str,
    parent_of: Option<&[Option<NodeId>]>,
) -> HashMap<String, NodeId> {
    let pk_idx = ds.schema.col_index(pk);
    let mut index = HashMap::new();
    for r in 0..ds.row_count() {
        let row = row_to_json(ds, r);
        let key = pk_idx
            .and_then(|ci| ds.row_key_string(r, ci))
            .unwrap_or_default();
        let id = tree.add(key.clone(), path.to_string(), row);
        if let Some(parents) = parent_of
            && let Some(Some(pid)) = parents.get(r)
        {
            tree.link(*pid, id);
        }
        index.insert(key, id);
    }
    index
}

/// 递归装配 ds 的所有 child group 到 parent_index 指示的父上。
fn ingest_children<R: ZmcRowSource>(
    tree: &mut MsTree,
    ds: &ZmcDataSet<R>,
    parent_path: &str,
    parent_index: &HashMap<String, NodeId>,
) {
    for group in &ds.children {
        let child_path = format!("{}.{}", parent_path, group.child_key);
        let child_ds = &group.child;
        // 为子层每行算它的父 NodeId（group.parent_ids[r] 是父业务键字符串）
        let parents: Vec<Option<NodeId>> = (0..child_ds.row_count())
            .map(|r| {
                group
                    .parent_ids
                    .get(r)
                    .and_then(|pk| parent_index.get(pk).copied())
            })
            .collect();
        let pk = child_pk(child_ds);
        let child_index = ingest_layer(tree, child_ds, &child_path, &pk, Some(&parents));
        // 孤儿（父键未命中）兜底为根，避免丢数据（对齐前端 buildTreeFromFlat）
        for r in 0..child_ds.row_count() {
            if parents.get(r).map(|p| p.is_none()).unwrap_or(true) {
                let key = child_ds
                    .schema
                    .col_index(&pk)
                    .and_then(|ci| child_ds.row_key_string(r, ci))
                    .unwrap_or_default();
                if let Some(&id) = child_index.get(&key) {
                    tree.add_root(id);
                }
            }
        }
        // 递归孙层
        ingest_children(tree, child_ds, &child_path, &child_index);
    }
}

/// 子数据集的主键列名猜测：优先 "id"，否则第 0 列。
fn child_pk<R: ZmcRowSource>(ds: &ZmcDataSet<R>) -> String {
    if ds.schema.col_index("id").is_some() {
        "id".to_string()
    } else {
        ds.schema.columns.first().cloned().unwrap_or_else(|| "id".to_string())
    }
}

/// 把 ZmcDataSet 的第 r 行转成 JSON object（列名 → 值）。
fn row_to_json<R: ZmcRowSource>(ds: &ZmcDataSet<R>, r: usize) -> Map<String, Value> {
    let row = &ds.rows[r];
    let mut m = Map::new();
    for i in 0..ds.schema.col_count() {
        let name = ds.schema.columns[i].clone();
        m.insert(name, cell_to_json(row, i, ds.schema.types[i]));
    }
    m
}

/// 单元格 → JSON 值（按中立列类型取，失败即 null）。
fn cell_to_json<R: ZmcRowSource>(row: &R, i: usize, ty: ZmcColType) -> Value {
    match ty {
        ZmcColType::Bool => row.get_bool(i).map(Value::from).unwrap_or(Value::Null),
        ZmcColType::Int2 => row.get_i16(i).map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
        ZmcColType::Int4 => row.get_i32(i).map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
        ZmcColType::Int8 => row.get_i64(i).map(Value::from).unwrap_or(Value::Null),
        ZmcColType::Float4 => row.get_f32(i).map(|v| Value::from(v as f64)).unwrap_or(Value::Null),
        ZmcColType::Float8 => row.get_f64(i).map(Value::from).unwrap_or(Value::Null),
        ZmcColType::Numeric => row
            .get_decimal(i)
            .and_then(|d| d.to_string().parse::<f64>().ok())
            .map(Value::from)
            .unwrap_or(Value::Null),
        ZmcColType::Text => row.get_str(i).map(|s| Value::from(s.to_string())).unwrap_or(Value::Null),
        ZmcColType::Json | ZmcColType::Jsonb => row.get_json_value(i).unwrap_or(Value::Null),
        ZmcColType::Uuid => row
            .get_uuid(i)
            .map(|u| Value::from(u.to_string()))
            .unwrap_or(Value::Null),
        ZmcColType::Date => row.get_date(i).map(|d| Value::from(d.to_string())).unwrap_or(Value::Null),
        ZmcColType::Timestamp => row
            .get_naive_datetime(i)
            .map(|t| Value::from(t.to_string()))
            .unwrap_or(Value::Null),
        ZmcColType::Timestamptz => row
            .get_datetime_utc(i)
            .map(|t| Value::from(t.to_rfc3339()))
            .unwrap_or(Value::Null),
        ZmcColType::Bytea => Value::Null, // 汇总不涉及二进制列
        ZmcColType::Unknown => row.get_str(i).map(|s| Value::from(s.to_string())).unwrap_or(Value::Null),
    }
}

/// 便捷装配：由平铺多层行 + 父键建树（对齐前端 `setFlatData`；测试与非 Zmc 场景用）。
pub fn from_flat(
    rows_by_path: &HashMap<String, Vec<Map<String, Value>>>,
    layers: &[(String, String, Option<String>)],
) -> MsTree {
    let mut tree = MsTree::new();
    let mut by_key: HashMap<String, NodeId> = HashMap::new();
    for (path, pk_field, parent_fk) in layers {
        let rows = match rows_by_path.get(path) {
            Some(r) => r,
            None => continue,
        };
        let parent_path = path.rsplit_once('.').map(|(p, _)| p.to_string());
        for row in rows {
            let biz_key = row.get(pk_field).map(value_key).unwrap_or_default();
            let id = tree.add(biz_key.clone(), path.clone(), row.clone());
            by_key.insert(format!("{path}#{biz_key}"), id);
            match (parent_fk, &parent_path) {
                (Some(fk), Some(pp)) => {
                    let pk = row.get(fk).map(value_key).unwrap_or_default();
                    if let Some(&pid) = by_key.get(&format!("{pp}#{pk}")) {
                        tree.link(pid, id);
                    } else {
                        tree.add_root(id);
                    }
                }
                _ => tree.add_root(id),
            }
        }
    }
    tree
}

/// JSON 值 → 稳定字符串键。
pub(crate) fn value_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn collect_and_descend() {
        let mut t = MsTree::new();
        let h = t.add("h1", "head", row(json!({"id":"h1"})));
        t.add_root(h);
        let i1 = t.add("i1", "head.items", row(json!({"id":"i1"})));
        let i2 = t.add("i2", "head.items", row(json!({"id":"i2"})));
        t.link(h, i1);
        t.link(h, i2);
        let x = t.add("t1", "head.items.taxes", row(json!({"id":"t1"})));
        t.link(i1, x);
        assert_eq!(t.collect_path("head.items").len(), 2);
        assert_eq!(t.collect_path("head.items.taxes").len(), 1);
        assert_eq!(t.descend_from(h, &["items"]).len(), 2);
        assert_eq!(t.descend_from(h, &["items", "taxes"]).len(), 1);
    }

    #[test]
    fn from_flat_builds_and_orphans_root() {
        let mut rows: HashMap<String, Vec<Map<String, Value>>> = HashMap::new();
        rows.insert("head".into(), vec![row(json!({"id":"h1"}))]);
        rows.insert(
            "head.items".into(),
            vec![
                row(json!({"id":"i1","upper_id":"h1"})),
                row(json!({"id":"i9","upper_id":"MISSING"})),
            ],
        );
        let layers = vec![
            ("head".to_string(), "id".to_string(), None),
            ("head.items".to_string(), "id".to_string(), Some("upper_id".to_string())),
        ];
        let t = from_flat(&rows, &layers);
        // h1 + 孤儿 i9 = 2 根
        assert_eq!(t.roots().len(), 2);
        // collect_path 从 head 下钻：只到达正常挂载的 i1（孤儿 i9 是根，不在 head 子树里）
        assert_eq!(t.collect_path("head.items").len(), 1);
    }
}
