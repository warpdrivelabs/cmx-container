//! JWT Claims 定义

use serde::{Deserialize, Serialize};

/// Access Token Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// subject（用户 ID）
    pub sub: String,
    /// expiration time
    pub exp: i64,
    /// issued at
    pub iat: i64,
    /// JWT ID（用于黑名单）
    pub jti: String,
    /// issuer
    pub iss: String,
    /// audience
    pub aud: String,
    /// 用户名
    pub username: String,
    /// 角色列表
    #[serde(default)]
    pub roles: Vec<String>,
    /// 权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 组织 ID
    #[serde(default)]
    pub org_id: Option<String>,
    /// 会话 ID
    pub sid: String,
    /// 设备类型
    pub device: String,
    /// Token 类型："access"
    pub typ: String,
    /// 密钥 ID（用于密钥轮换）
    #[serde(default)]
    pub kid: Option<String>,
}

/// Refresh Token Claims（精简）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    /// subject（用户 ID）
    pub sub: String,
    /// expiration time
    pub exp: i64,
    /// issued at
    pub iat: i64,
    /// JWT ID
    pub jti: String,
    /// issuer
    pub iss: String,
    /// Token 类型："refresh"
    pub typ: String,
    /// 会话 ID
    pub sid: String,
    /// 设备类型
    pub device: String,
}
