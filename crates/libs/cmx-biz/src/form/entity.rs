//! Form 实体定义
//!
//! 定义 Form 实体的数据结构，包括完整实体和创建/更新 DTO
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 表单实体（完整字段，用于查询返回）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Form {
    pub id: String,
    /// 表单编码
    pub code: String,
    /// 表单名称
    pub name: String,
    /// 表单描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 表单完整定义 JSON（字段/布局/校验）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
    /// 表单版本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
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

/// 创建请求 DTO（不含 id 与自动生成字段）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormForCreate {
    /// 表单编码
    pub code: String,
    /// 表单名称
    pub name: String,
    /// 表单描述
    pub description: Option<String>,
    /// 表单完整定义 JSON
    pub definition: Option<serde_json::Value>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
}

/// 更新请求 DTO（所有字段可选，仅更新提供的字段）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormForUpdate {
    /// 表单名称
    pub name: Option<String>,
    /// 表单描述
    pub description: Option<String>,
    /// 表单完整定义 JSON
    pub definition: Option<serde_json::Value>,
    /// 状态（0: 禁用, 1: 启用）
    pub status: Option<i32>,
}
