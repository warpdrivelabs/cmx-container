//! 权限/角色检查错误类型

use thiserror::Error;

/// 角色需求语义(AND/OR)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleRequirement {
    /// 必须拥有所有角色(AND)
    All,
    /// 拥有任一角色即可(OR)
    Any,
}

impl std::fmt::Display for RoleRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "全部"),
            Self::Any => write!(f, "任一"),
        }
    }
}

/// 访问拒绝错误(权限检查 + 角色检查统一错误类型)
#[derive(Debug, Clone, Error)]
pub enum PermissionDeniedError {
    /// 未认证(auth_context 缺失)
    #[error("未认证:缺少认证上下文")]
    Unauthenticated,

    /// 权限不足
    #[error("用户 {user_id} 缺少权限: {permission}")]
    Permission { user_id: String, permission: String },

    /// 角色不足(单角色)
    #[error("用户 {user_id} 缺少角色: {role}")]
    Role { user_id: String, role: String },

    /// 角色不足(多角色,AND/OR 语义)
    #[error("用户 {user_id} 缺少角色(需{requirement}): {roles}")]
    Roles {
        user_id: String,
        requirement: RoleRequirement,
        roles: String,
    },
}

impl PermissionDeniedError {
    /// 是否为未认证错误
    pub fn is_unauthenticated(&self) -> bool {
        matches!(self, Self::Unauthenticated)
    }
}
