//! 泛型树节点模块
//!
//! 提供通用的树形数据结构，支持将任意扁平列表转换为树形结构。
//! 只要数据类型实现了 `TreeNodeData` trait，即可使用 `TreeNode::from_list()` 构建树。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// 树节点数据 trait，用于泛型树构建
///
/// 任何需要构建树形结构的数据类型都需要实现此 trait，
/// 提供节点 ID、父节点 ID 和排序键三个核心方法。
///
/// # 示例
/// ```ignore
/// impl TreeNodeData for MyData {
///     fn node_id(&self) -> &str { &self.id }
///     fn parent_id(&self) -> Option<&str> { self.parent_id.as_deref() }
///     fn sort_key(&self) -> i32 { self.sort_order.unwrap_or(0) }
/// }
/// ```
pub trait TreeNodeData {
    /// 获取节点唯一标识
    fn node_id(&self) -> &str;

    /// 获取父节点 ID，根节点返回 None
    fn parent_id(&self) -> Option<&str>;

    /// 获取同级排序值，值越小越靠前
    fn sort_key(&self) -> i32;
}

/// 泛型树节点，支持任意数据格式的树形结构
///
/// 通过 `data` 字段承载业务数据，`children` 字段承载子节点列表。
/// 使用 `TreeNode::from_list()` 可将扁平列表转换为树形结构。
///
/// # 类型参数
/// * `T` - 业务数据类型，必须实现 `TreeNodeData` + `Serialize` + `Clone`
///
/// # 序列化输出示例
/// ```json
/// {
///   "data": { "code": "D001", "name": "域A" },
///   "children": [
///     { "data": { "code": "A001", "name": "应用X" }, "children": [] }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TreeNode<T> {
    /// 节点关联的业务数据
    pub data: T,
    /// 子节点列表
    #[schema(no_recursion)]
    pub children: Vec<TreeNode<T>>,
}

impl<T> TreeNode<T>
where
    T: TreeNodeData + Clone,
{
    /// 从扁平列表构建树形结构
    ///
    /// 将一组实现了 `TreeNodeData` 的扁平数据转换为多棵树（森林）。
    /// 算法步骤：
    /// 1. 以 `node_id()` 为 key 建立节点索引
    /// 2. 按 `parent_id()` 将子节点挂载到对应父节点
    /// 3. `parent_id()` 为 None 的节点作为根节点
    /// 4. 每层递归按 `sort_key()` 升序排列
    ///
    /// # 参数
    /// * `items` - 扁平数据列表
    ///
    /// # 返回值
    /// 根节点列表（可能有多棵树，即森林）
    pub fn from_list(items: Vec<T>) -> Vec<TreeNode<T>> {
        let mut node_map: HashMap<String, TreeNode<T>> = HashMap::new();
        let mut child_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut root_ids: Vec<String> = Vec::new();

        for item in items {
            let id = item.node_id().to_string();
            match item.parent_id() {
                Some(pid) if !pid.is_empty() => {
                    child_map.entry(pid.to_string()).or_default().push(id.clone());
                }
                _ => {
                    root_ids.push(id.clone());
                }
            }
            node_map.insert(id, TreeNode {
                data: item,
                children: Vec::new(),
            });
        }

        // 递归构建子节点，使用引用而非 remove，避免因 HashMap 迭代顺序不确定导致 children 丢失
        fn build_children<T: TreeNodeData + Clone>(
            parent_id: &str,
            node_map: &HashMap<String, TreeNode<T>>,
            child_map: &HashMap<String, Vec<String>>,
        ) -> Vec<TreeNode<T>> {
            let child_ids = match child_map.get(parent_id) {
                Some(ids) => ids,
                None => return Vec::new(),
            };

            child_ids
                .iter()
                .filter_map(|cid| {
                    node_map.get(cid).map(|node| TreeNode {
                        data: node.data.clone(),
                        children: build_children(cid, node_map, child_map),
                    })
                })
                .collect()
        }

        let mut root_nodes: Vec<TreeNode<T>> = root_ids
            .iter()
            .filter_map(|id| {
                node_map.get(id).map(|node| TreeNode {
                    data: node.data.clone(),
                    children: build_children(id, &node_map, &child_map),
                })
            })
            .collect();

        root_nodes.sort_by_key(|n| n.data.sort_key());
        for root in &mut root_nodes {
            Self::sort_children_recursive(root);
        }

        root_nodes
    }

    /// 递归对子节点按 sort_key 排序
    fn sort_children_recursive(node: &mut TreeNode<T>) {
        node.children.sort_by_key(|n| n.data.sort_key());
        for child in &mut node.children {
            Self::sort_children_recursive(child);
        }
    }
}
