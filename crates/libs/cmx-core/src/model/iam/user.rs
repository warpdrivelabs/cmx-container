//! 用户基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 用户基础数据模型（WASM 可见，不含敏感信息）
///
/// 注意：不含 password_hash，避免跨 WASM 边界泄漏
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
    #[serde(default)]
    pub last_login_at: Option<String>,
    #[serde(default)]
    pub last_login_ip: Option<String>,
    // 审计字段
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub update_time: Option<String>,
    #[serde(default)]
    pub create_by: Option<String>,
    #[serde(default)]
    pub create_name: Option<String>,
    #[serde(default)]
    pub update_by: Option<String>,
    #[serde(default)]
    pub update_name: Option<String>,
}
