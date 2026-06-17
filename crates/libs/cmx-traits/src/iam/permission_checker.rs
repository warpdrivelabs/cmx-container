//! 权限校验器 trait

use async_trait::async_trait;

use crate::error::TraitError;
use crate::iam::data_scope::DataScope;

/// 权限校验器 — 功能权限 + 数据权限扩展点
///
/// 功能权限（本次实现）：has_permission / has_role / get_user_permissions
/// 数据权限（本次预留）：get_data_scope 默认返回 All，未来实现具体逻辑
#[async_trait]
pub trait PermissionChecker: Send + Sync {
    /// 检查用户是否拥有指定权限码（如 "user:create"）
    ///
    /// 优化：先通过 EXISTS 子查询检查 system:all（轻量），再精确检查目标权限
    async fn has_permission(
        &self,
        user_id: &str,
        permission_code: &str,
    ) -> Result<bool, TraitError>;

    /// 检查用户是否拥有指定角色码（如 "admin"）
    async fn has_role(
        &self,
        user_id: &str,
        role_code: &str,
    ) -> Result<bool, TraitError>;

    /// 获取用户的所有权限码列表（聚合所有角色的权限）
    async fn get_user_permissions(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError>;

    /// 获取用户的所有角色码列表
    async fn get_user_role_codes(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError>;

    /// 获取用户的数据权限范围
    ///
    /// 默认实现返回 DataScope::All（无限制），后续实现时覆盖
    async fn get_data_scope(
        &self,
        _user_id: &str,
    ) -> Result<DataScope, TraitError> {
        Ok(DataScope::All)
    }
}
