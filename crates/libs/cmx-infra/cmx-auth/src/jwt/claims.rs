//! JWT Claims 定义。

use serde::{Deserialize, Serialize};

/// Access Token Claims。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// subject（用户 ID）。
    pub sub: String,
    /// 过期时间（Unix 时间戳）。
    pub exp: i64,
    /// 签发时间（Unix 时间戳）。
    pub iat: i64,
    /// JWT ID（用于黑名单）。
    pub jti: String,
    /// 签发者。
    pub iss: String,
    /// 受众。
    pub aud: String,
    /// 用户名。
    pub username: String,
    /// 用户昵称（展示用；`#[serde(default)]` 兼容旧令牌缺失该 claim）。
    #[serde(default)]
    pub nickname: Option<String>,
    /// 角色列表。
    #[serde(default)]
    pub roles: Vec<String>,
    /// 权限列表。
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 组织 ID。
    #[serde(default)]
    pub org_id: Option<String>,
    /// 会话 ID。
    pub sid: String,
    /// 设备类型。
    pub device: String,
    /// Token 类型：`access`。
    pub typ: String,
    /// 密钥 ID（用于密钥轮换）。
    #[serde(default)]
    pub kid: Option<String>,
}

/// Refresh Token Claims（精简）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    /// subject（用户 ID）。
    pub sub: String,
    /// 过期时间（Unix 时间戳）。
    pub exp: i64,
    /// 签发时间（Unix 时间戳）。
    pub iat: i64,
    /// JWT ID。
    pub jti: String,
    /// 签发者。
    pub iss: String,
    /// Token 类型：`refresh`。
    pub typ: String,
    /// 会话 ID。
    pub sid: String,
    /// 设备类型。
    pub device: String,
}
