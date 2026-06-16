//! Auth API 响应结构体

use serde::Serialize;
use utoipa::ToSchema;

/// 登录响应
#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// Access Token
    pub access_token: String,
    /// Refresh Token
    pub refresh_token: String,
    /// Token 类型
    pub token_type: String,
    /// Access Token 过期时间（Unix 时间戳）
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）
    pub refresh_expires_at: i64,
}

/// Token 校验响应
#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 角色列表
    pub roles: Vec<String>,
    /// 权限列表
    pub permissions: Vec<String>,
    /// 会话 ID
    pub session_id: Option<String>,
    /// 设备类型
    pub device_type: Option<String>,
    /// 认证方式
    pub auth_method: Option<String>,
}

/// 在线用户统计响应
#[derive(Debug, Serialize, ToSchema)]
pub struct OnlineCountResponse {
    /// 在线用户数
    pub count: u64,
}
