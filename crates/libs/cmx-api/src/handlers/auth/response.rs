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

/// 当前登录用户信息响应
#[derive(Debug, Serialize, ToSchema)]
pub struct UserInfoResponse {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 昵称
    pub nickname: Option<String>,
    /// 邮箱
    pub email: Option<String>,
    /// 手机号
    pub phone: Option<String>,
    /// 头像 URL
    pub avatar: Option<String>,
    /// 组织 ID
    pub org_id: Option<String>,
    /// 性别：0-未知，1-男，2-女
    pub gender: i64,
    /// 最后登录时间（Unix 时间戳）
    pub last_login_at: Option<i64>,
    /// 最后登录 IP
    pub last_login_ip: Option<String>,
    /// 描述
    pub description: Option<String>,
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
