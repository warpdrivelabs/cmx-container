//! IAM 错误类型定义。
//!
//! 定义 `IamError` 错误枚举与 `Result` 类型别名，
//! 并提供到 `cmx_api_types::Error` 和 `cmx_traits::error::TraitError` 的转换实现。

use thiserror::Error;

/// IAM 错误类型。
///
/// 涵盖数据库操作、业务校验、规则违反等错误场景，
/// 通过 `#[from]` 自动支持从 `cmx_database::crud::ServiceError` 转换。
#[derive(Debug, Error)]
pub enum IamError {
    /// 数据库 CRUD 操作错误。
    #[error("数据库操作错误: {0}")]
    Crud(#[from] cmx_database::crud::ServiceError),

    /// 用户不存在。
    #[error("用户不存在: {0}")]
    UserNotFound(String),

    /// 角色不存在。
    #[error("角色不存在: {0}")]
    RoleNotFound(String),

    /// 权限不存在。
    #[error("权限不存在: {0}")]
    PermissionNotFound(String),

    /// 用户名已存在。
    #[error("用户名已存在: {0}")]
    UsernameExists(String),

    /// 角色编码已存在。
    #[error("角色编码已存在: {0}")]
    RoleCodeExists(String),

    /// 权限编码已存在。
    #[error("权限编码已存在: {0}")]
    PermissionCodeExists(String),

    /// 角色组不存在。
    #[error("角色组不存在: {0}")]
    RoleGroupNotFound(String),

    /// 角色组下存在子组或关联角色，无法删除。
    #[error("角色组下存在子组或关联角色，无法删除")]
    RoleGroupInUse,

    /// 不能删除系统内置角色。
    #[error("不能删除系统内置角色")]
    CannotDeleteBuiltinRole,

    /// 密码哈希失败。
    #[error("密码哈希失败: {0}")]
    PasswordHashError(String),

    /// IAM 业务错误。
    #[error("IAM 业务错误: {0}")]
    Business(String),

    /// 权限规则违反。
    #[error("权限规则违反: 规则={rule_code}, 原因={message}")]
    RuleViolation { rule_code: String, message: String },
}

/// IAM `Result` 类型别名。
pub type Result<T> = core::result::Result<T, IamError>;

/// `IamError` 到 `cmx_api_types::Error` 的转换实现。
///
/// 将 IAM 业务错误映射为 HTTP API 响应错误：
/// - `UserNotFound`/`RoleNotFound`/`PermissionNotFound`/`RoleGroupNotFound` → `NotFound`
/// - `RoleGroupInUse`/`UsernameExists`/`RoleCodeExists`/`PermissionCodeExists`/`Business`/`RuleViolation` → `BusinessError`
/// - `CannotDeleteBuiltinRole` → `Forbidden`
/// - `Crud`/`PasswordHashError` → 内部错误
impl From<IamError> for cmx_api_types::Error {
    fn from(e: IamError) -> Self {
        match e {
            IamError::UserNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::PermissionNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleGroupNotFound(msg) => cmx_api_types::Error::NotFound(msg),
            IamError::RoleGroupInUse => cmx_api_types::Error::BusinessError(
                "角色组下存在子组或关联角色，无法删除".to_string(),
            ),
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

/// `IamError` 到 `TraitError` 的转换实现（保留错误类型语义）。
///
/// 与 `cmx_api_types::Error` 转换类似，但映射到 `TraitError` 的变体，
/// 供 trait 实现层使用。
impl From<IamError> for cmx_traits::error::TraitError {
    fn from(e: IamError) -> Self {
        match e {
            IamError::UserNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::PermissionNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleGroupNotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            IamError::RoleGroupInUse => cmx_traits::error::TraitError::Business(
                "角色组下存在子组或关联角色，无法删除".to_string(),
            ),
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
