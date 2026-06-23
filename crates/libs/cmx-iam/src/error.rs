//! IAM 错误类型定义

use thiserror::Error;

/// IAM 错误类型
#[derive(Debug, Error)]
pub enum IamError {
    #[error("数据库操作错误: {0}")]
    Crud(#[from] cmx_database::crud::ServiceError),

    #[error("用户不存在: {0}")]
    UserNotFound(String),

    #[error("角色不存在: {0}")]
    RoleNotFound(String),

    #[error("权限不存在: {0}")]
    PermissionNotFound(String),

    #[error("用户名已存在: {0}")]
    UsernameExists(String),

    #[error("角色编码已存在: {0}")]
    RoleCodeExists(String),

    #[error("权限编码已存在: {0}")]
    PermissionCodeExists(String),

    #[error("角色组不存在: {0}")]
    RoleGroupNotFound(String),

    #[error("角色组下存在子组或关联角色，无法删除")]
    RoleGroupInUse,

    #[error("不能删除系统内置角色")]
    CannotDeleteBuiltinRole,

    #[error("密码哈希失败: {0}")]
    PasswordHashError(String),

    #[error("IAM 业务错误: {0}")]
    Business(String),

    #[error("权限规则违反: 规则={rule_code}, 原因={message}")]
    RuleViolation { rule_code: String, message: String },
}

/// IAM Result 类型别名
pub type Result<T> = core::result::Result<T, IamError>;

/// IamError 到 cmx_api_types::Error 的转换
impl From<IamError> for cmx_api_types::Error {
    fn from(e: IamError) -> Self {
        match e {
            IamError::UserNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::PermissionNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleGroupNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleGroupInUse => {
                cmx_api_types::Error::BusinessError("角色组下存在子组或关联角色，无法删除".to_string())
            }
            IamError::UsernameExists(msg) => cmx_api_types::Error::BusinessError(msg),
            IamError::RoleCodeExists(msg) => cmx_api_types::Error::BusinessError(msg),
            IamError::PermissionCodeExists(msg) => cmx_api_types::Error::BusinessError(msg),
            IamError::CannotDeleteBuiltinRole => {
                cmx_api_types::Error::Forbidden("不能删除系统内置角色".to_string())
            }
            IamError::Business(msg) => cmx_api_types::Error::BusinessError(msg),
            IamError::RuleViolation { rule_code, message } => {
                cmx_api_types::Error::BusinessError(format!("[{}] {}", rule_code, message))
            }
            IamError::Crud(e) => cmx_api_types::Error::from(e),
            IamError::PasswordHashError(msg) => {
                cmx_api_types::Error::internal_error(format!("密码哈希失败: {msg}"))
            }
        }
    }
}

/// IamError 到 TraitError 的转换（保留错误类型语义）
impl From<IamError> for cmx_traits::error::TraitError {
    fn from(e: IamError) -> Self {
        match e {
            IamError::UserNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::PermissionNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleGroupNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleGroupInUse => {
                cmx_traits::error::TraitError::Business("角色组下存在子组或关联角色，无法删除".to_string())
            }
            IamError::UsernameExists(msg) => cmx_traits::error::TraitError::Business(msg),
            IamError::RoleCodeExists(msg) => cmx_traits::error::TraitError::Business(msg),
            IamError::PermissionCodeExists(msg) => cmx_traits::error::TraitError::Business(msg),
            IamError::CannotDeleteBuiltinRole => {
                cmx_traits::error::TraitError::Forbidden("不能删除系统内置角色".to_string())
            }
            IamError::Business(msg) => cmx_traits::error::TraitError::Business(msg),
            IamError::RuleViolation { rule_code, message } => {
                cmx_traits::error::TraitError::Business(format!("[{}] {}", rule_code, message))
            }
            IamError::PasswordHashError(msg) => {
                cmx_traits::error::TraitError::Internal(format!("密码哈希失败: {msg}"))
            }
            IamError::Crud(e) => {
                cmx_traits::error::TraitError::Internal(format!("数据库操作错误: {e}"))
            }
        }
    }
}
