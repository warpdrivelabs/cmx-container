//! 用户基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 用户基础数据模型（WASM 可见，不含敏感信息）。
///
/// 注意：不含 `password_hash`，避免跨 WASM 边界泄漏。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct User {
    /// 用户唯一标识（主键）。
    pub id: String,

    /// 登录用户名，业务唯一。
    pub username: String,

    /// 用户昵称，可空。
    #[serde(default)]
    pub nickname: Option<String>,

    /// 邮箱地址，可空。
    #[serde(default)]
    pub email: Option<String>,

    /// 手机号，可空。
    #[serde(default)]
    pub phone: Option<String>,

    /// 头像 URL 或路径，可空。
    #[serde(default)]
    pub avatar: Option<String>,

    /// 所属组织 ID，可空。
    #[serde(default)]
    pub org_id: Option<String>,

    /// 备注/描述，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 账户状态（如 1 启用 / 0 禁用），可空。
    #[serde(default)]
    pub status: Option<i64>,

    /// 最近一次登录时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub last_login_at: Option<String>,

    /// 最近一次登录来源 IP，可空。
    #[serde(default)]
    pub last_login_ip: Option<String>,

    // 审计字段
    /// 创建时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub create_time: Option<String>,

    /// 更新时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub update_time: Option<String>,

    /// 创建人 ID，可空。
    #[serde(default)]
    pub create_by: Option<String>,

    /// 创建人姓名，可空。
    #[serde(default)]
    pub create_name: Option<String>,

    /// 更新人 ID，可空。
    #[serde(default)]
    pub update_by: Option<String>,

    /// 更新人姓名，可空。
    #[serde(default)]
    pub update_name: Option<String>,
}
