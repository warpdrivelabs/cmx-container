//! 用户 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// User 从 cmx-core re-export，不在此定义
pub use cmx_core::model::iam::User;

/// 创建用户请求（API 层入参，含明文密码）
///
/// 注意：不 derive Fields！password 不是数据库列，不能直接用于 GenericCrudService
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserForCreate {
    pub username: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    pub password: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 用户入库数据（Service 层内部使用，含 password_hash）
///
/// 与 cmx_user 表列一一对应，derive Fields 用于 GenericCrudService
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct UserForInsert {
    pub username: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    pub password_hash: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 更新用户请求（不含用户名，全 Option）
///
/// 注意：不 derive Fields！password 字段非 DB 列，需 Service 层特殊处理
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserForUpdate {
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 用户更新入库数据（Service 层内部使用，含 password_hash）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct UserForUpdateInsert {
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub password_hash: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 分配角色请求（IAM 专用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssignRolesRequest {
    pub user_id: String,
    pub role_ids: Vec<String>,
}
