//! SysDatasource 实体定义
//!
//! 定义 SysDatasource 实体的数据结构，包括完整实体和创建/更新 DTO

use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

/// 数据源实体（完整字段，用于查询返回）
///
/// 表示系统中的一个数据源配置
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
pub struct SysDatasource {
    /// 主键
    pub id: String,
    /// 数据源标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
    /// 数据源描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 数据库类型
    pub db_type: String,
    /// 数据库连接 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_url: Option<String>,
    /// 数据库模式
    pub db_schema: Option<String>,
    /// 是否默认;0否1是
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_flag: Option<i32>,
    /// 最大连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<i32>,
    /// 最小空闲连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_connections: Option<i32>,
    /// 连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<i64>,
    /// 空闲连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<i64>,
    /// 最大生命周期（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lifetime: Option<i64>,
    /// 健康检查间隔（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_interval: Option<i64>,
    /// 健康检查超时（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_timeout: Option<i64>,
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
/// 用于创建 SysDatasource 的请求数据，包含完整的数据库连接配置
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct SysDatasourceForCreate {
    /// 数据源标识（唯一）
    pub db_id: String,
    /// 数据源描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 数据库类型 (postgres, mysql, sqlite)
    pub db_type: String,
    /// 数据库连接 URL
    pub db_url: String,
    /// 数据库模式（PostgreSQL 默认为 public）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_schema: Option<String>,
    /// 是否默认数据源；0否1是
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_flag: Option<i32>,
    /// 最大连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<i32>,
    /// 最小空闲连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_connections: Option<i32>,
    /// 连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<i64>,
    /// 空闲连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<i64>,
    /// 最大生命周期（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lifetime: Option<i64>,
    /// 健康检查间隔（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_interval: Option<i64>,
    /// 健康检查超时（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_timeout: Option<i64>,
}

/// 更新请求 DTO
///
/// 用于更新 SysDatasource 的请求数据，所有字段均为可选
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct SysDatasourceForUpdate {
    /// 数据源标识
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_id: Option<String>,
    /// 数据源描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 数据库类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_type: Option<String>,
    /// 数据库连接 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_url: Option<String>,
    /// 数据库模式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_schema: Option<String>,
    /// 是否默认；0否1是
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_flag: Option<i32>,
    /// 最大连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<i32>,
    /// 最小空闲连接数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_connections: Option<i32>,
    /// 连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<i64>,
    /// 空闲连接超时时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<i64>,
    /// 最大生命周期（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_lifetime: Option<i64>,
    /// 健康检查间隔（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_interval: Option<i64>,
    /// 健康检查超时（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_timeout: Option<i64>,
    /// 状态：0-禁用，1-启用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 归档标志：0-未归档，1-已归档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
}
