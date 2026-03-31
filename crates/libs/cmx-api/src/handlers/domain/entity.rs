//! Domain 实体定义
//!
//! 定义 Domain 实体的数据结构，包括完整实体和创建/更新 DTO
use crate::rest::TreeNodeData;
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use utoipa::ToSchema;

/// 领域实体（完整字段，用于查询返回）
///
/// 表示系统中的一个领域/域对象
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow, ToSchema)]
pub struct Domain {
    /// 唯一标识码（主键）
    pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态（0: 禁用, 1: 启用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档（0: 否, 1: 是）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
    /// 创建时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    /// 创建者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_by: Option<String>,
    /// 创建者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    /// 更新者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_by: Option<String>,
    /// 更新者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

/// 创建请求 DTO
///
/// 用于创建 Domain 的请求数据
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct DomainForCreate {
    // /// 唯一标识码（主键）
    // pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// 更新请求 DTO
///
/// 用于更新 Domain 的请求数据，所有字段均为可选
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct DomainForUpdate {
    /// 名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态（0: 禁用, 1: 启用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档（0: 否, 1: 是）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
}

/// 域-应用-模块 树形节点数据
///
/// 用于接收 tree.sql 查询返回的扁平数据，
/// 实现 `TreeNodeData` trait 后可通过 `TreeNode::from_list()` 构建树形结构。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DomainTreeNodeData {
    /// 父节点编码（域节点的 parent_id 为 NULL）
    pub parent_id: Option<String>,
    /// 节点编码（唯一标识）
    pub code: String,
    /// 节点名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// 标签
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 节点类型（domain / application / module）
    pub node_type: String,
    /// 层级（1=域, 2=应用, 3=模块）
    pub level: i32,
    /// 所属域编码
    pub domain_code: Option<String>,
    /// 所属应用编码
    pub application_code: Option<String>,
    /// 所属模块编码
    pub module_code: Option<String>,
    /// 排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
    /// 创建时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    /// 创建者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_by: Option<String>,
    /// 创建者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    /// 更新者 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_by: Option<String>,
    /// 更新者名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

impl TreeNodeData for DomainTreeNodeData {
    /// 节点 ID 为 code 字段
    fn node_id(&self) -> &str {
        &self.code
    }

    /// 父节点 ID 为 parent_id 字段，域节点为 None
    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// 排序键为 sort_order 字段，默认 0
    fn sort_key(&self) -> i32 {
        self.sort_order.unwrap_or(0)
    }
}
