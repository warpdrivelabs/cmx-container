//! Auth API 请求结构体

use serde::Deserialize;
use utoipa::ToSchema;

/// 用户登录请求载荷。
///
/// 包含用户名、密码及可选的设备信息。`device_type` 用于会话维度统计和审计；
/// `device_id` 用于设备绑定校验（可选）。
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// 用户名。
    pub username: String,
    /// 明文密码（由 cmx-auth 内部使用 argon2 哈希比对）。
    pub password: String,
    /// 设备类型（web/mobile/desktop），默认 "web"。
    #[serde(default = "default_device_type")]
    pub device_type: String,
    /// 设备 ID，由调用方生成并保持稳定。
    #[serde(default)]
    pub device_id: String,
}

fn default_device_type() -> String {
    "web".to_string()
}

/// 刷新 Token 请求载荷。
///
/// 用于在 Access Token 即将过期时换发新 Token。Refresh Token 由 cmx-auth
/// 维护其 jti 状态并检测重放。
#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    /// Refresh Token。
    pub refresh_token: String,
}

/// 撤销指定 Token 的请求载荷。
///
/// 可撤销 Access Token 或 Refresh Token。撤销后该 Token 立即失效。
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeRequest {
    /// 要撤销的 Token（Access 或 Refresh）。
    pub token: String,
}

/// 撤销用户全部 Token 的请求载荷。
///
/// 通常由管理员在紧急情况下使用（例如怀疑账号泄露），将强制该用户所有设备下线。
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeAllRequest {
    /// 用户 ID。
    pub user_id: String,
}

/// 校验 Token 有效性的请求载荷。
///
/// 主要供网关或中间件在不重建 AuthContext 的情况下快速验证 Token。
#[derive(Debug, Deserialize, ToSchema)]
pub struct ValidateRequest {
    /// Access Token。
    pub token: String,
}

/// 修改当前用户密码的请求载荷。
///
/// 旧密码会通过 cmx-auth 校验，校验通过后写入新密码哈希并更新密码历史。
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// 旧密码。
    pub old_password: String,
    /// 新密码（受密码策略和历史去重约束）。
    pub new_password: String,
}
