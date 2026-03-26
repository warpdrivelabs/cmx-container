//! Application 实体定义
//!
//! 定义 Application 实体的数据结构，包括完整实体和创建/更新 DTO

use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

/// 应用实体（完整字段，用于查询返回）
///
/// 表示系统中的一个应用对象
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
pub struct Application {
    /// 主键，应用编码，全局唯一，如: FI, CO, MM
    pub code: String,
    /// 所属域编码，逻辑关联到cmx_domain.code
    pub domain_code: String,
    /// 应用名称，如: 财务会计, 管理会计
    pub name: String,
    /// 应用描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: product(产品应用), platform(平台应用), integration(集成应用)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序字段，数值小的靠前
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态：0-禁用，1-启用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 归档标志：0-未归档，1-已归档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
    /// 创建时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<OffsetDateTime>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<OffsetDateTime>,
    /// 创建人ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_by: Option<String>,
    /// 创建人名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    /// 更新人ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_by: Option<String>,
    /// 更新人名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

/// 创建请求 DTO
///
/// 用于创建 Application 的请求数据
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct ApplicationForCreate {
    /// 应用名称，如: 财务会计, 管理会计
    pub name: String,
    /// 所属域编码，逻辑关联到cmx_domain.code
    pub domain_code: String,
    /// 应用描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: product(产品应用), platform(平台应用), integration(集成应用)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序字段，数值小的靠前
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// 更新请求 DTO
///
/// 用于更新 Application 的请求数据，所有字段均为可选
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct ApplicationForUpdate {
    /// 应用名称，如: 财务会计, 管理会计
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 所属域编码，逻辑关联到cmx_domain.code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_code: Option<String>,
    /// 应用描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: product(产品应用), platform(平台应用), integration(集成应用)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序字段，数值小的靠前
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 状态：0-禁用，1-启用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 归档标志：0-未归档，1-已归档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
}
