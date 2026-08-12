//! Auth API 响应结构体

use serde::Serialize;
use utoipa::ToSchema;

/// 登录成功响应载荷。
///
/// 同时返回 Access Token 和 Refresh Token，调用方需自行持久化（建议使用
/// HttpOnly Cookie 或安全存储）。`access_expires_at` 和 `refresh_expires_at`
/// 为绝对 Unix 时间戳。
#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// Access Token，用于后续请求的 `Authorization: Bearer` 头。
    pub access_token: String,
    /// Refresh Token，用于在 Access Token 过期时换发新 Token。
    pub refresh_token: String,
    /// Token 类型，固定为 "Bearer"。
    pub token_type: String,
    /// Access Token 过期时间（Unix 时间戳）。
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）。
    pub refresh_expires_at: i64,
}

/// Token 校验响应载荷。
///
/// 包含 Token 关联用户的身份与权限上下文，常用于网关或上游服务进行二次校验。
#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateResponse {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 角色编码列表。
    pub roles: Vec<String>,
    /// 权限编码列表。
    pub permissions: Vec<String>,
    /// 关联会话 ID。
    pub session_id: Option<String>,
    /// 设备类型。
    pub device_type: Option<String>,
    /// 认证方式（password/apikey/oauth2 等）。
    pub auth_method: Option<String>,
}

/// 在线用户数统计响应载荷。
///
/// 由 `cmx-auth` 维护 Redis HyperLogLog 或计数器得出。
#[derive(Debug, Serialize, ToSchema)]
pub struct OnlineCountResponse {
    /// 在线用户数。
    pub count: u64,
}

/// 当前登录用户完整信息响应载荷。
///
/// 包含用户基础信息、角色/权限列表以及当前会话的认证上下文。
/// 用于个人中心、用户菜单等需要展示登录态全量信息的场景。
#[derive(Debug, Serialize, ToSchema)]
pub struct UserInfoResponse {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 昵称。
    pub nickname: Option<String>,
    /// 邮箱。
    pub email: Option<String>,
    /// 手机号。
    pub phone: Option<String>,
    /// 头像 URL。
    pub avatar: Option<String>,
    /// 组织 ID。
    pub org_id: Option<String>,
    /// 性别：0-未知，1-男，2-女。
    pub gender: i64,
    /// 最后登录时间（Unix 时间戳）。
    pub last_login_at: Option<i64>,
    /// 最后登录 IP。
    pub last_login_ip: Option<String>,
    /// 描述。
    pub description: Option<String>,
    /// 角色编码列表。
    pub roles: Vec<String>,
    /// 权限编码列表。
    pub permissions: Vec<String>,
    /// 关联会话 ID。
    pub session_id: Option<String>,
    /// 设备类型。
    pub device_type: Option<String>,
    /// 认证方式（password/apikey/oauth2 等）。
    pub auth_method: Option<String>,
}
