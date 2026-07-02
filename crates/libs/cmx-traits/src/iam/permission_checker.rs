//! 权限校验器 trait。

use async_trait::async_trait;

use crate::error::TraitError;
use crate::iam::data_scope::DataScope;

/// 权限校验器 — 功能权限 + 数据权限扩展点。
///
/// 功能权限（本次实现）：`has_permission` / `has_role` / `get_user_permissions`。
/// 数据权限（本次预留）：`get_data_scope` 默认返回 `All`，未来实现具体逻辑。
#[async_trait]
pub trait PermissionChecker: Send + Sync {
    /// 检查用户是否拥有指定权限码（如 `user:create`）。
    ///
    /// 优化：先通过 EXISTS 子查询检查 `system:all`（轻量），再精确检查目标权限。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `permission_code` - 权限码。
    ///
    /// # Returns
    ///
    /// 拥有权限返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn has_permission(
        &self,
        user_id: &str,
        permission_code: &str,
    ) -> Result<bool, TraitError>;

    /// 检查用户是否拥有指定角色码（如 `admin`）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `role_code` - 角色码。
    ///
    /// # Returns
    ///
    /// 拥有角色返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn has_role(&self, user_id: &str, role_code: &str) -> Result<bool, TraitError>;

    /// 获取用户的所有权限码列表（聚合所有角色的权限）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回权限码列表，无权限时返回空 `Vec`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, TraitError>;

    /// 获取用户的所有角色码列表。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回角色码列表，无角色时返回空 `Vec`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_user_role_codes(&self, user_id: &str) -> Result<Vec<String>, TraitError>;

    /// 获取用户的数据权限范围。
    ///
    /// 默认实现返回 [`DataScope::All`]（无限制），后续实现时覆盖。
    ///
    /// # Arguments
    ///
    /// * `_user_id` - 用户 ID（默认实现未使用）。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`DataScope`]，默认为 [`DataScope::All`]。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_data_scope(&self, _user_id: &str) -> Result<DataScope, TraitError> {
        Ok(DataScope::All)
    }
}
