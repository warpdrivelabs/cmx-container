//! 用户 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// User 从 cmx-core re-export，不在此定义
pub use cmx_core::model::iam::User;

/// 创建用户请求（API 层入参，含明文密码）。
///
/// 注意：不 derive `Fields`！`password` 不是数据库列，不能直接用于 `GenericCrudService`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserForCreate {
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

    /// 明文密码（Service 层会调用 `AuthService::hash_password` 进行哈希后丢弃）。
    pub password: String,

    /// 头像 URL 或路径，可空。
    #[serde(default)]
    pub avatar: Option<String>,

    /// 所属组织 ID，可空。
    #[serde(default)]
    pub org_id: Option<String>,

    /// 备注/描述，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 账户状态（如 1 启用 / 0 禁用），为空时默认启用。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 用户入库数据（Service 层内部使用，含 `password_hash`）。
///
/// 与 `cmx_user` 表列一一对应，derive `Fields` 用于 `GenericCrudService`。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct UserForInsert {
    /// 登录用户名，业务唯一。
    pub username: String,

    /// 用户昵称。
    #[serde(default)]
    pub nickname: Option<String>,

    /// 邮箱地址。
    #[serde(default)]
    pub email: Option<String>,

    /// 手机号。
    #[serde(default)]
    pub phone: Option<String>,

    /// 密码哈希（Argon2id），由 Service 层生成。
    pub password_hash: String,

    /// 头像 URL 或路径。
    #[serde(default)]
    pub avatar: Option<String>,

    /// 所属组织 ID。
    #[serde(default)]
    pub org_id: Option<String>,

    /// 备注/描述。
    #[serde(default)]
    pub description: Option<String>,

    /// 账户状态，1 启用 / 0 禁用。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 更新用户请求（不含用户名，全 `Option`）。
///
/// 注意：不 derive `Fields`！`password` 字段非 DB 列，需 Service 层特殊处理。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UserForUpdate {
    /// 用户昵称。
    #[serde(default)]
    pub nickname: Option<String>,

    /// 邮箱地址。
    #[serde(default)]
    pub email: Option<String>,

    /// 手机号。
    #[serde(default)]
    pub phone: Option<String>,

    /// 新明文密码（提供时触发密码修改流程）。
    #[serde(default)]
    pub password: Option<String>,

    /// 头像 URL 或路径。
    #[serde(default)]
    pub avatar: Option<String>,

    /// 备注/描述。
    #[serde(default)]
    pub description: Option<String>,

    /// 账户状态。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 用户更新入库数据（Service 层内部使用，含 `password_hash`）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct UserForUpdateInsert {
    /// 用户昵称。
    #[serde(default)]
    pub nickname: Option<String>,

    /// 邮箱地址。
    #[serde(default)]
    pub email: Option<String>,

    /// 手机号。
    #[serde(default)]
    pub phone: Option<String>,

    /// 新密码哈希（仅在修改密码时填充）。
    #[serde(default)]
    pub password_hash: Option<String>,

    /// 头像 URL 或路径。
    #[serde(default)]
    pub avatar: Option<String>,

    /// 备注/描述。
    #[serde(default)]
    pub description: Option<String>,

    /// 账户状态。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 分配角色请求（IAM 专用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssignRolesRequest {
    /// 目标用户名。
    pub username: String,

    /// 待分配的角色 ID 列表（空数组表示清空所有角色）。
    pub role_ids: Vec<String>,
}
