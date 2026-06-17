//! IAM Service trait 定义（cmx-iam 内部）

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, PermissionTreeNode, Role, User};
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;

use crate::permission::{PermissionFilter, PermissionForCreate, PermissionForUpdate};
use crate::role::{RoleFilter, RoleForCreate, RoleForUpdate};
use crate::user::{UserFilter, UserForCreate, UserForUpdate};

/// 用户服务 trait
#[async_trait]
pub trait UserService: Send + Sync {
    /// 创建用户
    async fn create_user(
        &self,
        svr_ctx: &SVRContext,
        data: UserForCreate,
    ) -> Result<User, TraitError>;
    /// 获取单个用户（按 username 查询）
    async fn get_user(&self, username: &str) -> Result<User, TraitError>;
    /// 更新用户
    async fn update_user(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        data: UserForUpdate,
    ) -> Result<User, TraitError>;
    /// 删除用户（支持批量）
    async fn delete_user(&self, svr_ctx: &SVRContext, user_ids: &[String]) -> Result<(), TraitError>;
    /// 分页查询用户
    async fn page_users(
        &self,
        filter: UserFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<User>, i64), TraitError>;
    /// 列表查询用户
    async fn list_users(&self, filter: UserFilter) -> Result<Vec<User>, TraitError>;
    /// 为用户分配角色（全量替换，按 username 查询）
    async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        username: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError>;
    /// 获取用户的角色列表（按 username 查询）
    async fn get_user_roles(&self, username: &str) -> Result<Vec<Role>, TraitError>;
}

/// 角色服务 trait
#[async_trait]
pub trait RoleService: Send + Sync {
    /// 创建角色
    async fn create_role(
        &self,
        svr_ctx: &SVRContext,
        data: RoleForCreate,
    ) -> Result<Role, TraitError>;
    /// 获取单个角色
    async fn get_role(&self, role_id: &str) -> Result<Role, TraitError>;
    /// 更新角色
    async fn update_role(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        data: RoleForUpdate,
    ) -> Result<Role, TraitError>;
    /// 删除角色（支持批量）
    async fn delete_role(&self, svr_ctx: &SVRContext, role_ids: &[String]) -> Result<(), TraitError>;
    /// 分页查询角色
    async fn page_roles(
        &self,
        filter: RoleFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<Role>, i64), TraitError>;
    /// 列表查询角色
    async fn list_roles(&self, filter: RoleFilter) -> Result<Vec<Role>, TraitError>;
    /// 为角色分配权限（全量替换）
    async fn assign_permissions(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        permission_ids: &[String],
    ) -> Result<(), TraitError>;
    /// 获取角色的权限列表
    async fn get_role_permissions(&self, role_id: &str) -> Result<Vec<Permission>, TraitError>;
}

/// 权限服务 trait
#[async_trait]
pub trait PermissionService: Send + Sync {
    /// 创建权限
    async fn create_permission(
        &self,
        svr_ctx: &SVRContext,
        data: PermissionForCreate,
    ) -> Result<Permission, TraitError>;
    /// 获取单个权限
    async fn get_permission(&self, permission_id: &str) -> Result<Permission, TraitError>;
    /// 更新权限
    async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError>;
    /// 删除权限（支持批量）
    async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<(), TraitError>;
    /// 分页查询权限
    async fn page_permissions(
        &self,
        filter: PermissionFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<Permission>, i64), TraitError>;
    /// 列表查询权限
    async fn list_permissions(&self, filter: PermissionFilter) -> Result<Vec<Permission>, TraitError>;
    /// 获取权限树（递归结构）
    async fn get_permission_tree(&self) -> Result<Vec<PermissionTreeNode>, TraitError>;
}
