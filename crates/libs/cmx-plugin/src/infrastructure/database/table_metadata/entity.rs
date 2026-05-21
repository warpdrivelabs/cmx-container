//! 表元数据实体定义
//!
//! 定义表元数据的各种数据结构

use chrono::{DateTime, Utc};
use modql::field::Fields;
use serde::{Deserialize, Serialize};

/// cmx_meta_table_define_version 记录（版本历史）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataVersion {
    pub id: String,
    pub table_name: String,
    pub display_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub metadata: serde_json::Value,
    pub archived: i32,
    pub app_id: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 表元数据详情（联查结果，包含 metadata）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataDetail {
    pub id: String,
    pub table_name: String,
    pub display_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub metadata: serde_json::Value,
    pub archived: i32,
    pub ddl_status: Option<String>,
    pub app_id: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 创建请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForCreate {
    pub table_name: String,
    pub display_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub metadata: serde_json::Value,
    /// 应用ID（可选，默认值由数据库层设置）
    pub app_id: Option<String>,
}

/// 更新请求 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct TableMetadataForUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// 表元数据 ID 信息（用于删除时获取关联键）
#[derive(Debug, Clone)]
pub struct TableMetadataIdentity {
    pub id: String,
    pub table_name: String,
    pub db_id: String,
    pub version: String,
}
