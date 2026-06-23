//! IAM Service trait 定义（cmx-iam 内部）

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, PermissionTreeNode, Role, RoleGroup, RoleGroupTreeNode, User};
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;

use crate::permission::{PermissionFilter, PermissionForCreate, PermissionForUpdate};
use crate::role::{RoleFilter, RoleForCreate, RoleForUpdate};
use crate::role_group::{RoleGroupFilter, RoleGroupForCreate, RoleGroupForUpdate};
use crate::user::{UserFilter, UserForCreate, UserForUpdate};

/// 临时授权状态过滤
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempAssignmentStatusFilter {
    /// 全部
    All,
    /// 生效中
    Active,
    /// 已过期（status=1 但 effective_until < NOW）
    Expired,
    /// 已撤销（status=0）
    Revoked,
}

/// 用户角色临时授权记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserRoleAssignment {
    pub id: String,
    pub user_id: String,
    pub role_id: String,
    pub role_code: String,
    pub role_name: String,
    pub effective_from: chrono::DateTime<chrono::Utc>,
    pub effective_until: chrono::DateTime<chrono::Utc>,
    pub reason: Option<String>,
    pub source: String,
    pub status: i64,
    pub revoked_by: Option<String>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub create_time: chrono::DateTime<chrono::Utc>,
}

// ===== 审计查询相关结构体（阶段5新增） =====

/// 用户有效权限响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EffectivePermissionsResponse {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<RoleSummary>,
    pub permissions: Vec<PermissionSummary>,
    pub active_temp_roles: u32,
    pub expired_temp_roles: u32,
    /// 7天内将过期的临时角色数
    pub upcoming_expirations: u32,
}

/// 角色摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RoleSummary {
    pub id: String,
    pub code: String,
    pub name: String,
}

/// 权限摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionSummary {
    pub id: String,
    pub code: String,
    pub name: String,
}

/// 角色权限差异响应
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionDiffResponse {
    pub role_1: RoleSummary,
    pub role_2: RoleSummary,
    pub only_in_role_1: Vec<PermissionSummary>,
    pub only_in_role_2: Vec<PermissionSummary>,
    pub common: Vec<PermissionSummary>,
}

/// 权限使用统计
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionUsageStat {
    pub permission_id: String,
    pub permission_code: String,
    pub permission_name: String,
    pub role_count: u32,
    pub user_count: u32,
    /// 最后一次分配时间
    pub last_assigned_at: Option<chrono::DateTime<chrono::Utc>>,
}

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

    // ===== 临时角色授权相关（阶段1新增） =====

    /// 分配临时角色（带有效期）
    async fn assign_temp_role(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        role_id: &str,
        effective_from: chrono::DateTime<chrono::Utc>,
        effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
        source: &str,
    ) -> Result<UserRoleAssignment, TraitError>;

    /// 撤销临时角色（逻辑撤销 status=0）
    async fn revoke_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        reason: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 批量撤销临时角色
    async fn revoke_temp_roles_batch(
        &self,
        svr_ctx: &SVRContext,
        assignment_ids: &[String],
        reason: Option<&str>,
    ) -> Result<u64, TraitError>;

    /// 延长临时授权有效期
    async fn extend_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        new_effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 查询用户的临时授权列表
    async fn get_user_temp_assignments(
        &self,
        user_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError>;

    /// 查询角色被授权的用户列表（临时授权）
    async fn get_role_temp_assigned_users(
        &self,
        role_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError>;

    // ===== 审计查询（阶段5新增） =====

    /// 查询用户有效权限（合并永久 + 临时授权）
    async fn get_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<EffectivePermissionsResponse, TraitError>;
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

    // ===== 审计查询（阶段5新增） =====

    /// 比较两个角色的权限差异
    async fn get_permission_diff(
        &self,
        role_id_1: &str,
        role_id_2: &str,
    ) -> Result<PermissionDiffResponse, TraitError>;
}

/// 角色组服务 trait
#[async_trait]
pub trait RoleGroupService: Send + Sync {
    /// 创建角色组
    async fn create_role_group(
        &self,
        svr_ctx: &SVRContext,
        data: RoleGroupForCreate,
    ) -> Result<RoleGroup, TraitError>;
    /// 获取单个角色组
    async fn get_role_group(&self, role_group_id: &str) -> Result<RoleGroup, TraitError>;
    /// 更新角色组
    async fn update_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_id: &str,
        data: RoleGroupForUpdate,
    ) -> Result<RoleGroup, TraitError>;
    /// 删除角色组（支持批量）
    async fn delete_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_ids: &[String],
    ) -> Result<(), TraitError>;
    /// 分页查询角色组
    async fn page_role_groups(
        &self,
        filter: RoleGroupFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<RoleGroup>, i64), TraitError>;
    /// 列表查询角色组
    async fn list_role_groups(
        &self,
        filter: RoleGroupFilter,
    ) -> Result<Vec<RoleGroup>, TraitError>;
    /// 获取角色组树（递归结构）
    async fn get_role_group_tree(&self) -> Result<Vec<RoleGroupTreeNode>, TraitError>;
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
    /// 获取权限树（递归结构，支持按域/应用/模块过滤）
    async fn get_permission_tree(
        &self,
        domain_code: Option<&str>,
        app_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<PermissionTreeNode>, TraitError>;

    // ===== 审计查询（阶段5新增） =====

    /// 统计每个权限被多少角色使用
    async fn get_permission_usage_stat(&self) -> Result<Vec<PermissionUsageStat>, TraitError>;
}
