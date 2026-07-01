//! IAM 审计查询共享响应结构体。
//!
//! 定义 `RoleSummary`、`PermissionSummary` 等摘要类型，
//! 被用户审计（`EffectivePermissionsResponse`）与角色审计（`PermissionDiffResponse`）共同复用。

/// 角色摘要。
///
/// 用于审计查询响应中的角色精简信息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoleSummary {
    /// 角色 ID。
    pub id: String,
    /// 角色编码。
    pub code: String,
    /// 角色名称。
    pub name: String,
    /// 角色描述。
    #[serde(default)]
    pub description: Option<String>,
}

/// 权限摘要。
///
/// 用于审计查询响应中的权限精简信息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionSummary {
    /// 权限 ID。
    pub id: String,
    /// 权限编码。
    pub code: String,
    /// 权限名称。
    pub name: String,
    /// 资源类型（如 menu / button / api）。
    #[serde(default)]
    pub resource_type: Option<String>,
    /// 权限描述。
    #[serde(default)]
    pub description: Option<String>,
}
