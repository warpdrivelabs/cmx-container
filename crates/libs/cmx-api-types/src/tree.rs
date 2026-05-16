//! 树形结构构建与序列化。
//!
//! 该模块提供了将扁平列表转换为树形结构的通用能力，
//! 通过 [`TreeNodeData`] trait 定义节点数据约束，[`TreeNode`] 表示树节点。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// 树节点数据 trait。
///
/// 实现此 trait 的类型可以作为树节点数据，用于 [`TreeNode::from_list`] 构建树形结构。
/// 需要提供节点 ID、父节点 ID 和排序键。
pub trait TreeNodeData {
    /// 返回节点的唯一标识。
    fn node_id(&self) -> &str;

    /// 返回父节点的唯一标识，根节点返回 `None` 或空字符串。
    fn parent_id(&self) -> Option<&str>;

    /// 返回排序键，用于同级节点的排序，值越小越靠前。
    fn sort_key(&self) -> i32;
}

/// 树节点。
///
/// 包含节点数据和子节点列表，支持序列化和 OpenAPI 文档生成。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TreeNode<T> {
    /// 节点数据。
    pub data: T,
    /// 子节点列表。
    #[schema(no_recursion)]
    pub children: Vec<TreeNode<T>>,
}

impl<T> TreeNode<T>
where
    T: TreeNodeData + Clone,
{
    /// 将扁平列表转换为树形结构。
    ///
    /// 根据每个节点的 `parent_id` 建立父子关系，使用 `sort_key` 对同级节点排序。
    /// `parent_id` 为 `None` 或空字符串的节点作为根节点。
    ///
    /// # Examples
    ///
    /// ```
    /// use cmx_api_types::{TreeNode, TreeNodeData};
    ///
    /// #[derive(Clone)]
    /// struct Item {
    ///     id: String,
    ///     parent_id: Option<String>,
    ///     sort: i32,
    ///     name: String,
    /// }
    ///
    /// impl TreeNodeData for Item {
    ///     fn node_id(&self) -> &str { &self.id }
    ///     fn parent_id(&self) -> Option<&str> { self.parent_id.as_deref() }
    ///     fn sort_key(&self) -> i32 { self.sort }
    /// }
    ///
    /// let items = vec![
    ///     Item { id: "1".into(), parent_id: None, sort: 0, name: "root".into() },
    ///     Item { id: "2".into(), parent_id: Some("1".into()), sort: 0, name: "child".into() },
    /// ];
    ///
    /// let tree = TreeNode::from_list(items);
    /// assert_eq!(tree.len(), 1);
    /// assert_eq!(tree[0].children.len(), 1);
    /// ```
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

    fn sort_children_recursive(node: &mut TreeNode<T>) {
        node.children.sort_by_key(|n| n.data.sort_key());
        for child in &mut node.children {
            Self::sort_children_recursive(child);
        }
    }
}
