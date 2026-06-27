//! WASM 用户/权限类型定义
//!
//! 定义 WASM 插件通过 `cmx:iam` 宿主函数查询用户信息时使用的请求/响应类型。
//!
//! # 安全设计
//!
//! 所有用户信息类型均为 **脱敏结构**：`WasmUserDetails` 从内部 `UserAuthData` 映射时
//! 显式丢弃 `password_hash` 等敏感字段，编译期保证不跨 WASM 边界泄露。
//!
//! # 序列化
//!
//! 全部类型 derive `Serialize, Deserialize`，使用 MsgPack（rmp_serde）序列化，
//! 与现有 `DbRequest`/`DbResponse` 等宿主函数类型保持一致。

use serde::{Deserialize, Serialize};

/// WASM 可见的用户详情（脱敏，无 password_hash）。
///
/// 由宿主函数从内部 `UserAuthData` 映射而来，仅保留业务必要字段。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WasmUserDetails {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 昵称。
    #[serde(default)]
    pub nickname: Option<String>,
    /// 邮箱。
    #[serde(default)]
    pub email: Option<String>,
    /// 手机号。
    #[serde(default)]
    pub phone: Option<String>,
    /// 头像 URL。
    #[serde(default)]
    pub avatar: Option<String>,
    /// 所属组织 ID。
    #[serde(default)]
    pub org_id: Option<String>,
    /// 状态（1-启用，0-禁用）。
    #[serde(default)]
    pub status: i64,
    /// 描述。
    #[serde(default)]
    pub description: Option<String>,
}

/// WASM 可见的有效权限聚合（脱敏版 `EffectivePermissionsResponse`）。
///
/// 角色与权限均以 code 列表形式返回（轻量，插件做判断够用），
/// 并附带临时角色统计，便于插件展示授权状态。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WasmEffectivePermissions {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 有效角色 code 列表（合并永久 + 活跃临时角色）。
    #[serde(default)]
    pub roles: Vec<String>,
    /// 有效权限 code 列表（合并永久 + 活跃临时授权）。
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 当前活跃的临时角色数。
    #[serde(default)]
    pub active_temp_roles: u32,
}

/// IAM 宿主函数请求（统一信封，按变体分发）。
///
/// 单一宿主函数入口接收此 enum，根据变体路由到对应查询逻辑，
/// 避免注册多个 Extism host function（每个 host function 都有注册开销）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "args")]
pub enum IamRequest {
    /// 查询单个用户详情。
    GetUserDetails {
        /// 目标用户 ID。
        user_id: String,
    },
    /// 批量查询用户详情（WHERE id = ANY($1)，无 N+1）。
    GetUsersDetails {
        /// 目标用户 ID 列表。
        user_ids: Vec<String>,
    },
    /// 查询用户有效权限聚合（roles + permissions + 临时角色统计）。
    GetEffectivePermissions {
        /// 目标用户 ID。
        user_id: String,
    },
    /// 权限校验：用户是否拥有指定权限码。
    HasPermission {
        /// 目标用户 ID。
        user_id: String,
        /// 权限码（如 `user:read`）。
        code: String,
    },
    /// 角色判断：用户是否拥有指定角色码。
    HasRole {
        /// 目标用户 ID。
        user_id: String,
        /// 角色码（如 `admin`）。
        code: String,
    },
}

/// IAM 宿主函数响应（统一信封）。
///
/// 扁平字段设计 + `#[serde(default)]`：不同操作只填充对应字段，其余为默认值，
/// 与现有 `DbResponse` 风格一致，MsgPack 紧凑。
/// 失败时 `success: false` 且 `error` 携带原因，不抛 WASM trap。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IamResponse {
    /// 是否成功。
    pub success: bool,
    /// 错误信息（失败时）。
    #[serde(default)]
    pub error: Option<String>,
    /// `GetUserDetails` 结果。
    #[serde(default)]
    pub user: Option<WasmUserDetails>,
    /// `GetUsersDetails` 结果。
    #[serde(default)]
    pub users: Vec<WasmUserDetails>,
    /// `GetEffectivePermissions` 结果。
    #[serde(default)]
    pub permissions: Option<WasmEffectivePermissions>,
    /// `HasPermission` / `HasRole` 结果。
    #[serde(default)]
    pub allowed: Option<bool>,
}

impl IamResponse {
    /// 构建成功响应（用于无数据的操作，一般配合字段赋值）。
    pub fn ok() -> Self {
        Self {
            success: true,
            ..Default::default()
        }
    }

    /// 构建错误响应。
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(msg.into()),
            ..Default::default()
        }
    }
}
