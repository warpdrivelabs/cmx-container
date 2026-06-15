//! Module 实体定义
//!
//! 定义 Module 实体的数据结构，包括完整实体和创建/更新 DTO

use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 模块实体（完整字段，用于查询返回）
///
/// 表示系统中的一个模块对象
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Module {
    pub  id: String,

    /// 模块编码，全局唯一，如: GL, AR, AP
    pub code: String,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码，逻辑关联到cmx_application.code
    pub application_code: String,
    /// 模块名称，如: 总账模块, 应收模块
    pub name: String,
    /// 模块描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: business(业务模块), extension(扩展点), integration(集成点)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]
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
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub create_time: Option<OffsetDateTime>,
    /// 更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
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
/// 用于创建 Module 的请求数据
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleForCreate {
    //// 模块编码，全局唯一，如: GL, AR, AP
    pub code: String,
    /// 模块名称，如: 总账模块, 应收模块
    pub name: String,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码，逻辑关联到cmx_application.code
    pub application_code: String,
    /// 模块描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: business(业务模块), extension(扩展点), integration(集成点)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// 排序字段，数值小的靠前
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

/// 更新请求 DTO
///
/// 用于更新 Module 的请求数据，所有字段均为可选
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleForUpdate {
    /// 模块名称，如: 总账模块, 应收模块
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 所属域编码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_code: Option<String>,
    /// 所属应用编码，逻辑关联到cmx_application.code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_code: Option<String>,
    /// 模块描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 类型: business(业务模块), extension(扩展点), integration(集成点)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[field(name="type")]
    pub r#type: Option<String>,
    /// 多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]
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
