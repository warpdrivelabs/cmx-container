//! 表元数据实体定义
//!
//! 定义表元数据的各种数据结构

use chrono::{DateTime, Utc};
use modql::field::Fields;
use serde::{Deserialize, Serialize};

/// cmx_meta_table_define_version 记录（版本历史）
///
/// 存储表元数据的每个版本的详细信息，包括表结构定义 JSON。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataVersion {
    /// 主键 ID
    pub id: String,
    /// 表名
    pub table_name: String,
    /// 表显示名称
    pub display_name: String,
    /// 数据库 ID
    pub db_id: String,
    /// 所属插件 ID
    pub plugin_id: String,
    /// 版本号（通常等于插件版本号）
    pub version: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 表元数据 JSON（包含字段定义、约束等）
    pub metadata: serde_json::Value,
    /// 归档标志：0-未归档，1-已归档
    pub archived: i32,
    /// 应用隔离标识，用于多租户/多应用场景隔离
    pub app_id: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 创建人 ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人 ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 表元数据详情（联查结果，包含 metadata）
///
/// 主表与版本表联查后的完整结果，用于返回给调用方。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataDetail {
    /// 主键 ID
    pub id: String,
    /// 表名
    pub table_name: String,
    /// 表显示名称
    pub display_name: String,
    /// 数据库 ID
    pub db_id: String,
    /// 所属插件 ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 表元数据 JSON
    pub metadata: serde_json::Value,
    /// 归档标志：0-未归档，1-已归档
    pub archived: i32,
    /// DDL 执行状态：pending-待执行，executing-执行中，completed-已完成，failed-执行失败
    pub ddl_status: Option<String>,
    /// 应用隔离标识
    pub app_id: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 创建人 ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人 ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 创建请求 DTO
///
/// 用于创建新的表元数据记录。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForCreate {
    /// 表名
    pub table_name: String,
    /// 表显示名称
    pub display_name: String,
    /// 数据库 ID
    pub db_id: String,
    /// 所属插件 ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
    /// 表元数据 JSON
    pub metadata: serde_json::Value,
    /// 应用隔离标识，用于多租户/多应用场景隔离
    pub app_id: Option<String>,
}

/// 更新请求 DTO
///
/// 用于更新表元数据的部分字段，非必填字段为 None 时不更新。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForUpdate {
    /// 表显示名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// 版本号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 域编码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_code: Option<String>,
    /// 应用编码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_code: Option<String>,
    /// 模块编码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_code: Option<String>,
    /// 表元数据 JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// 表元数据 ID 信息（用于删除时获取关联键）
#[derive(Debug, Clone)]
pub struct TableMetadataIdentity {
    /// 主键 ID
    pub id: String,
    /// 表名
    pub table_name: String,
    /// 数据库 ID
    pub db_id: String,
    /// 版本号
    pub version: String,
}
