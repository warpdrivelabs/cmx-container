//! Auth API 请求结构体

use serde::Deserialize;
use utoipa::ToSchema;

/// 登录请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 设备类型（web/mobile/desktop）
    #[serde(default = "default_device_type")]
    pub device_type: String,
    /// 设备 ID
    #[serde(default)]
    pub device_id: String,
}

fn default_device_type() -> String {
    "web".to_string()
}

/// 刷新 Token 请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// Refresh Token
    pub refresh_token: String,
}

/// 撤销 Token 请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeRequest {
    /// 要撤销的 Token（Access 或 Refresh）
    pub token: String,
}

/// 撤销用户所有 Token 请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeAllRequest {
    /// 用户 ID
    pub user_id: String,
}

/// 校验 Token 请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidateRequest {
    /// Access Token
    pub token: String,
}

/// 修改密码请求
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// 旧密码
    pub old_password: String,
    /// 新密码
    pub new_password: String,
}
