//! 权限服务内部辅助方法
//!
//! 提供 `PermissionServiceImpl` 的 DataSet 提取、默认过滤注入与权限树构建工具，
//! 供各功能子模块复用。这些方法以 `pub(super)` 暴露给 [`super`] 的其他子模块，不构成对外 API。

use cmx_core::model::iam::{Permission, PermissionTreeNode};
use modql::filter::{OpValInt64, OpValsInt64};

use crate::error::IamError;
use crate::permission::PermissionFilter;
use crate::permission::service::PermissionServiceImpl;

impl PermissionServiceImpl {
    /// 从 DataSet 第一行提取 `Permission`。
    pub(super) fn extract_permission(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<Permission, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::PermissionNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<Permission>(json_val)
            .map_err(|e| IamError::Business(format!("权限反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 `Permission` 列表（跳过反序列化失败的行）。
    pub(super) fn extract_permissions(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Vec<Permission> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<Permission>(json_val).ok()
            })
            .collect()
    }

    /// 构造带 `archived = 0` 默认过滤的 `PermissionFilter`。
    pub(super) fn with_default_archived(mut filter: PermissionFilter) -> PermissionFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }

    /// 将扁平权限列表组装为树形结构（按 `parent_id` 递归）。
    pub(super) fn build_tree(permissions: Vec<Permission>) -> Vec<PermissionTreeNode> {
        // 找出根节点（parent_id 为 None 或空字符串）
        let roots: Vec<Permission> = permissions
            .iter()
            .filter(|p| p.parent_id.as_ref().map(|s| s.is_empty()).unwrap_or(true))
            .cloned()
            .collect();

        // 递归构建子树
        roots
            .into_iter()
            .map(|root| Self::build_subtree(root, &permissions))
            .collect()
    }

    /// 递归构建子树。
    pub(super) fn build_subtree(parent: Permission, all: &[Permission]) -> PermissionTreeNode {
        let children: Vec<PermissionTreeNode> = all
            .iter()
            .filter(|p| p.parent_id.as_deref() == Some(parent.id.as_str()))
            .cloned()
            .map(|child| Self::build_subtree(child, all))
            .collect();

        PermissionTreeNode {
            permission: parent,
            children,
        }
    }
}
