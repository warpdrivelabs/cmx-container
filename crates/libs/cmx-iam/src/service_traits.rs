//! IAM Service trait 定义（cmx-iam 内部）。
//!
//! 定义 `UserService`、`RoleService`、`RoleGroupService`、`PermissionService` 四个服务 trait，
//! 以及审计查询相关的响应结构体。各 trait 的具体实现位于对应子模块的 `service.rs` 中。

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, PermissionTreeNode, Role, RoleGroup, RoleGroupTreeNode, User};
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use crate::permission::{
    DeletePermissionOutcome, PermissionFilter, PermissionForCreate, PermissionForUpdate,
};
use crate::role::{RoleFilter, RoleForCreate, RoleForUpdate};
use crate::role_group::{RoleGroupFilter, RoleGroupForCreate, RoleGroupForUpdate};
use crate::user::{UserFilter, UserForCreate, UserForUpdate};

/// 临时授权状态过滤条件。
///
/// 用于 `get_user_temp_assignments` / `get_role_temp_assigned_users` 等查询接口，
/// 控制返回哪些状态的临时授权记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempAssignmentStatusFilter {
    /// 全部记录，不过滤状态。
    All,
    /// 生效中（`status = 1` 且当前时间在有效期内）。
    Active,
    /// 已过期（`status = 1` 但 `effective_until < NOW`）。
    Expired,
    /// 已撤销（`status = 0`）。
    Revoked,
}

/// 用户角色临时授权记录。
///
/// 表示一条 `cmx_user_role_assignment` 表记录，包含有效期、撤销信息等字段，
/// 用于临时角色授权的查询响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserRoleAssignment {
    /// 授权记录唯一 ID。
    pub id: String,
    /// 目标用户 ID。
    pub user_id: String,
    /// 目标角色 ID。
    pub role_id: String,
    /// 角色编码（关联 `cmx_role.code` 查询得到）。
    pub role_code: String,
    /// 角色名称（关联 `cmx_role.name` 查询得到）。
    pub role_name: String,
    /// 授权生效时间（UTC）。
    pub effective_from: chrono::DateTime<chrono::Utc>,
    /// 授权失效时间（UTC）。
    pub effective_until: chrono::DateTime<chrono::Utc>,
    /// 授权原因（可选）。
    pub reason: Option<String>,
    /// 授权来源（如 `manual` / `oauth2` 等）。
    pub source: String,
    /// 状态（1 生效 / 0 已撤销）。
    pub status: i64,
    /// 撤销操作者 ID（仅已撤销时填充）。
    pub revoked_by: Option<String>,
    /// 撤销时间（UTC，仅已撤销时填充）。
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 记录创建时间（UTC）。
    pub create_time: chrono::DateTime<chrono::Utc>,
}

// ===== 审计查询相关结构体（阶段5新增） =====

/// 用户有效权限响应。
///
/// 合并永久授权与临时授权后的用户权限视图，
/// 包含角色列表、权限列表及临时角色统计信息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EffectivePermissionsResponse {
    /// 用户 ID。
    pub user_id: String,
    /// 用户名。
    pub username: String,
    /// 用户拥有的角色摘要列表（含永久 + 临时）。
    pub roles: Vec<RoleSummary>,
    /// 用户拥有的权限摘要列表（含永久 + 临时）。
    pub permissions: Vec<PermissionSummary>,
    /// 当前生效中的临时角色数。
    pub active_temp_roles: u32,
    /// 已过期但未撤销的临时角色数。
    pub expired_temp_roles: u32,
    /// 7 天内将过期的临时角色数。
    pub upcoming_expirations: u32,
}

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

/// 角色权限差异响应。
///
/// 比较两个角色的权限集合，返回仅存在于各自的权限及共有权限。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionDiffResponse {
    /// 第一个角色摘要。
    pub role_1: RoleSummary,
    /// 第二个角色摘要。
    pub role_2: RoleSummary,
    /// 仅第一个角色拥有的权限列表。
    pub only_in_role_1: Vec<PermissionSummary>,
    /// 仅第二个角色拥有的权限列表。
    pub only_in_role_2: Vec<PermissionSummary>,
    /// 两个角色共有的权限列表。
    pub common: Vec<PermissionSummary>,
}

/// 权限使用统计。
///
/// 统计每个权限被多少角色、多少用户使用，用于权限审计与清理决策。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PermissionUsageStat {
    /// 权限 ID。
    pub permission_id: String,
    /// 权限编码。
    pub permission_code: String,
    /// 权限名称。
    pub permission_name: String,
    /// 引用该权限的角色数。
    pub role_count: u32,
    /// 通过角色间接拥有该权限的用户数。
    pub user_count: u32,
    /// 最后一次分配时间（UTC，可选）。
    pub last_assigned_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 用户服务 trait。
///
/// 定义用户 CRUD、角色分配、临时授权及审计查询等操作。
/// 实现见 `crate::user::service::UserServiceImpl`。
#[async_trait]
pub trait UserService: Send + Sync {
    /// 创建用户。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `data` - 用户创建参数，包含用户名、明文密码等。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `User` 实例。
    ///
    /// # Errors
    ///
    /// 当用户名已存在、密码不满足要求或数据库操作失败时返回错误。
    async fn create_user(
        &self,
        svr_ctx: &SVRContext,
        data: UserForCreate,
    ) -> Result<User, TraitError>;

    /// 获取单个用户（按 `username` 查询）。
    ///
    /// # Arguments
    ///
    /// * `username` - 用户名。
    ///
    /// # Returns
    ///
    /// 成功时返回 `User` 实例。
    ///
    /// # Errors
    ///
    /// 当用户不存在或数据库查询失败时返回错误。
    async fn get_user(&self, username: &str) -> Result<User, TraitError>;

    /// 更新用户。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `user_id` - 目标用户 ID。
    /// * `data` - 更新参数（不含用户名，全 `Option`）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `User` 实例。
    ///
    /// # Errors
    ///
    /// 当密码长度不满足要求或数据库操作失败时返回错误。
    async fn update_user(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        data: UserForUpdate,
    ) -> Result<User, TraitError>;

    /// 批量删除用户（软删除 + 角色关联清理）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `user_ids` - 待删除的用户 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败或 SQL 执行失败时返回错误。
    async fn delete_user(&self, svr_ctx: &SVRContext, user_ids: &[String]) -> Result<(), TraitError>;

    /// 分页查询用户。
    ///
    /// # Arguments
    ///
    /// * `filter` - 用户查询过滤器。
    /// * `list_options` - 分页与排序选项（由 `PageParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 元组 `(用户列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// 当数据库分页查询失败时返回错误。
    async fn page_users(
        &self,
        filter: UserFilter,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64), TraitError>;

    /// 列表查询用户（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 用户查询过滤器。
    /// * `list_options` - 排序选项（由 `ListParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 匹配的用户列表。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回错误。
    async fn list_users(
        &self,
        filter: UserFilter,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<User>, TraitError>;

    /// 为用户分配角色（全量替换，按 `username` 查询）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `username` - 目标用户名。
    /// * `role_ids` - 待分配的角色 ID 列表；空数组表示清空所有角色。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败、SQL 执行失败或用户不存在时返回错误。
    async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        username: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 获取用户的角色列表（按 `username` 查询）。
    ///
    /// # Arguments
    ///
    /// * `username` - 目标用户名。
    ///
    /// # Returns
    ///
    /// 用户关联的角色列表，可能为空。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_user_roles(&self, username: &str) -> Result<Vec<Role>, TraitError>;

    // ===== 临时角色授权相关（阶段1新增） =====

    /// 分配临时角色（带有效期）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `user_id` - 目标用户 ID。
    /// * `role_id` - 目标角色 ID。
    /// * `effective_from` - 授权生效时间（UTC）。
    /// * `effective_until` - 授权失效时间（UTC，必须晚于 `effective_from`）。
    /// * `reason` - 授权原因（可选）。
    /// * `source` - 授权来源（如 `manual` / `oauth2`）。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `UserRoleAssignment` 完整记录。
    ///
    /// # Errors
    ///
    /// 当有效期校验失败、SoD 规则违反或 SQL 执行失败时返回错误。
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

    /// 撤销临时角色（逻辑撤销 `status = 0`）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `assignment_id` - 待撤销的授权记录 ID。
    /// * `reason` - 撤销原因（可选）。
    ///
    /// # Errors
    ///
    /// 当授权记录不存在、已撤销或 SQL 执行失败时返回错误。
    async fn revoke_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        reason: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 批量撤销临时角色。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `assignment_ids` - 待撤销的授权记录 ID 列表。
    /// * `reason` - 撤销原因（可选）。
    ///
    /// # Returns
    ///
    /// 实际撤销的记录数。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败或 SQL 执行失败时返回错误。
    async fn revoke_temp_roles_batch(
        &self,
        svr_ctx: &SVRContext,
        assignment_ids: &[String],
        reason: Option<&str>,
    ) -> Result<u64, TraitError>;

    /// 延长临时授权有效期。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `assignment_id` - 待延期的授权记录 ID。
    /// * `new_effective_until` - 新的失效时间（UTC，必须晚于原 `effective_until`）。
    /// * `reason` - 延期原因（可选）。
    ///
    /// # Errors
    ///
    /// 当授权记录不存在、已撤销、新有效期不晚于原有效期或 SQL 执行失败时返回错误。
    async fn extend_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        new_effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 查询用户的临时授权列表。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    /// * `status_filter` - 状态过滤条件。
    ///
    /// # Returns
    ///
    /// 匹配的临时授权记录列表，按创建时间倒序排列。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_user_temp_assignments(
        &self,
        user_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError>;

    /// 查询角色被授权的用户列表（临时授权）。
    ///
    /// # Arguments
    ///
    /// * `role_id` - 目标角色 ID。
    /// * `status_filter` - 状态过滤条件。
    ///
    /// # Returns
    ///
    /// 匹配的临时授权记录列表，按创建时间倒序排列。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_role_temp_assigned_users(
        &self,
        role_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError>;

    // ===== 审计查询（阶段5新增） =====

    /// 查询用户有效权限（合并永久 + 临时授权）。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    ///
    /// # Returns
    ///
    /// 包含角色、权限及临时角色统计的 `EffectivePermissionsResponse`。
    ///
    /// # Errors
    ///
    /// 当用户不存在或 SQL 查询失败时返回错误。
    async fn get_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<EffectivePermissionsResponse, TraitError>;
}

/// 角色服务 trait。
///
/// 定义角色 CRUD、权限分配及审计查询等操作。
/// 实现见 `crate::role::service::RoleServiceImpl`。
#[async_trait]
pub trait RoleService: Send + Sync {
    /// 创建角色。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `data` - 角色创建参数。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `Role` 实例。
    ///
    /// # Errors
    ///
    /// 当角色编码已存在或数据库操作失败时返回错误。
    async fn create_role(
        &self,
        svr_ctx: &SVRContext,
        data: RoleForCreate,
    ) -> Result<Role, TraitError>;

    /// 获取单个角色。
    ///
    /// # Arguments
    ///
    /// * `role_id` - 角色唯一标识。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Role` 实例。
    ///
    /// # Errors
    ///
    /// 当角色不存在或数据库查询失败时返回错误。
    async fn get_role(&self, role_id: &str) -> Result<Role, TraitError>;

    /// 更新角色。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_id` - 目标角色 ID。
    /// * `data` - 更新参数（全 `Option`，未提供字段不更新）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `Role` 实例。
    ///
    /// # Errors
    ///
    /// 当数据库 CRUD 操作失败时返回错误。
    async fn update_role(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        data: RoleForUpdate,
    ) -> Result<Role, TraitError>;

    /// 批量删除角色（软删除 + 权限关联清理）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_ids` - 待删除的角色 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当尝试删除内置角色、事务开启/提交失败或 SQL 执行失败时返回错误。
    async fn delete_role(&self, svr_ctx: &SVRContext, role_ids: &[String]) -> Result<(), TraitError>;

    /// 分页查询角色。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色查询过滤器。
    /// * `list_options` - 分页与排序选项（由 `PageParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 元组 `(角色列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// 当数据库分页查询失败时返回错误。
    async fn page_roles(
        &self,
        filter: RoleFilter,
        list_options: ListOptions,
    ) -> Result<(Vec<Role>, i64), TraitError>;

    /// 列表查询角色（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色查询过滤器。
    /// * `list_options` - 排序选项（由 `ListParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 匹配的角色列表。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回错误。
    async fn list_roles(
        &self,
        filter: RoleFilter,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<Role>, TraitError>;

    /// 为角色分配权限（全量替换）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_id` - 目标角色 ID。
    /// * `permission_ids` - 待分配的权限 ID 列表；空数组表示清空所有权限。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败、SoD 规则违反或 SQL 执行失败时返回错误。
    async fn assign_permissions(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        permission_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 为角色分配用户（全量替换）。
    ///
    /// 将目标角色的用户集合设置为 `user_ids`：事务内先删除该角色的所有用户关联，
    /// 再分块批量插入新关联。空数组表示清空。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志。
    /// * `role_id` - 目标角色 ID。
    /// * `user_ids` - 待分配的用户 ID 列表（全量替换）。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败、SoD 规则违反或 SQL 执行失败时返回错误。
    async fn assign_role_users(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        user_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 获取角色的权限列表。
    ///
    /// # Arguments
    ///
    /// * `role_id` - 目标角色 ID。
    ///
    /// # Returns
    ///
    /// 角色关联的权限列表，可能为空。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_role_permissions(&self, role_id: &str) -> Result<Vec<Permission>, TraitError>;

    // ===== 审计查询（阶段5新增） =====

    /// 比较两个角色的权限差异。
    ///
    /// # Arguments
    ///
    /// * `role_id_1` - 第一个角色 ID。
    /// * `role_id_2` - 第二个角色 ID。
    ///
    /// # Returns
    ///
    /// 包含仅各自拥有及共有权限的 `PermissionDiffResponse`。
    ///
    /// # Errors
    ///
    /// 当角色不存在或查询失败时返回错误。
    async fn get_permission_diff(
        &self,
        role_id_1: &str,
        role_id_2: &str,
    ) -> Result<PermissionDiffResponse, TraitError>;
}

/// 角色组服务 trait。
///
/// 定义角色组 CRUD 及树形查询等操作。
/// 实现见 `crate::role_group::service::RoleGroupServiceImpl`。
#[async_trait]
pub trait RoleGroupService: Send + Sync {
    /// 创建角色组。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `data` - 角色组创建参数。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `RoleGroup` 实例。
    ///
    /// # Errors
    ///
    /// 当数据库 CRUD 操作失败时返回错误。
    async fn create_role_group(
        &self,
        svr_ctx: &SVRContext,
        data: RoleGroupForCreate,
    ) -> Result<RoleGroup, TraitError>;

    /// 获取单个角色组。
    ///
    /// # Arguments
    ///
    /// * `role_group_id` - 角色组唯一标识。
    ///
    /// # Returns
    ///
    /// 成功时返回 `RoleGroup` 实例。
    ///
    /// # Errors
    ///
    /// 当角色组不存在或数据库查询失败时返回错误。
    async fn get_role_group(&self, role_group_id: &str) -> Result<RoleGroup, TraitError>;

    /// 更新角色组。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_group_id` - 目标角色组 ID。
    /// * `data` - 更新参数（全 `Option`，未提供字段不更新）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `RoleGroup` 实例。
    ///
    /// # Errors
    ///
    /// 当数据库 CRUD 操作失败时返回错误。
    async fn update_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_id: &str,
        data: RoleGroupForUpdate,
    ) -> Result<RoleGroup, TraitError>;

    /// 批量删除角色组（软删除）。
    ///
    /// 删除前校验：待删除角色组下不能存在子角色组或关联角色。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_group_ids` - 待删除的角色组 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当角色组下存在子组或关联角色、或 SQL 执行失败时返回错误。
    async fn delete_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 分页查询角色组。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色组查询过滤器。
    /// * `current` - 当前页码（从 1 开始）。
    /// * `size` - 每页记录数。
    ///
    /// # Returns
    ///
    /// 元组 `(角色组列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// 当数据库分页查询失败时返回错误。
    async fn page_role_groups(
        &self,
        filter: RoleGroupFilter,
        list_options: ListOptions,
    ) -> Result<(Vec<RoleGroup>, i64), TraitError>;

    /// 列表查询角色组（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色组查询过滤器。
    /// * `list_options` - 排序选项（由 `ListParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 匹配的角色组列表。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回错误。
    async fn list_role_groups(
        &self,
        filter: RoleGroupFilter,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<RoleGroup>, TraitError>;

    /// 获取角色组树（递归结构）。
    ///
    /// 一次性加载所有有效角色组（`archived = 0`），
    /// 在内存中按 `parent_id` 递归构建树形结构。
    ///
    /// # Returns
    ///
    /// 树根列表（每个根节点包含嵌套的 `children`）。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_role_group_tree(&self) -> Result<Vec<RoleGroupTreeNode>, TraitError>;
}

/// 权限服务 trait。
///
/// 定义权限 CRUD、权限树查询及使用统计等操作。
/// 实现见 `crate::permission::service::PermissionServiceImpl`。
#[async_trait]
pub trait PermissionService: Send + Sync {
    /// 创建权限。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `data` - 权限创建参数。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// 当权限编码已存在或数据库操作失败时返回错误。
    async fn create_permission(
        &self,
        svr_ctx: &SVRContext,
        data: PermissionForCreate,
    ) -> Result<Permission, TraitError>;

    /// 获取单个权限。
    ///
    /// # Arguments
    ///
    /// * `permission_id` - 权限唯一标识。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// 当权限不存在或数据库查询失败时返回错误。
    async fn get_permission(&self, permission_id: &str) -> Result<Permission, TraitError>;

    /// 更新权限。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_id` - 目标权限 ID。
    /// * `data` - 更新参数（全 `Option`，未提供字段不更新）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// 当数据库 CRUD 操作失败时返回错误。
    async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError>;

    /// 批量删除权限（物理删除 + 前置校验 + 级联子权限）。
    ///
    /// 流程：
    /// 1. 按 `full_code_path` LIKE 收集每个根权限及其所有后代子权限 ID。
    /// 2. 查询这些权限是否被角色关联（`cmx_role_permission`）。
    /// 3. 若任一被使用，返回 `DeletePermissionOutcome::Blocked`，携带角色 code+name 详情，不执行删除。
    /// 4. 若均未被使用，事务内物理删除权限及其所有后代、物理删除相关角色关联，返回 `Deleted`。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_ids` - 待删除的根权限 ID 列表；空数组返回空的 `Deleted`。
    ///
    /// # Errors
    ///
    /// 当事务开启/提交失败或 SQL 执行失败时返回错误。
    async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<DeletePermissionOutcome, TraitError>;

    /// 分页查询权限。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 权限查询过滤器数组（每个元素是一个 AND 组）。
    /// * `list_options` - 分页与排序选项（由 `PageParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 元组 `(权限列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// 当数据库分页查询失败时返回错误。
    async fn page_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<Permission>, i64), TraitError>;

    /// 列表查询权限（不分页）。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 权限查询过滤器数组（每个元素是一个 AND 组）。
    /// * `list_options` - 排序选项（由 `ListParams::to_list_options()` 构造）。
    ///
    /// # Returns
    ///
    /// 匹配的权限列表。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回错误。
    async fn list_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<Permission>, TraitError>;

    /// 获取权限树（递归结构，支持按域/应用/模块过滤）。
    ///
    /// 一次性加载所有有效权限（`archived = 0 AND status = 1`），
    /// 在内存中按 `parent_id` 递归构建树形结构。
    /// 当指定 `domain_code`/`app_code`/`module_code` 时，通过参数化 SQL WHERE 子句过滤。
    ///
    /// # Arguments
    ///
    /// * `domain_code` - 所属域编码过滤（可选）。
    /// * `app_code` - 所属应用编码过滤（可选）。
    /// * `module_code` - 所属模块编码过滤（可选）。
    ///
    /// # Returns
    ///
    /// 树根列表（每个根节点包含嵌套的 `children`）。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_permission_tree(
        &self,
        domain_code: Option<&str>,
        app_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<PermissionTreeNode>, TraitError>;

    // ===== 审计查询（阶段5新增） =====

    /// 统计每个权限被多少角色使用。
    ///
    /// # Returns
    ///
    /// 权限使用统计列表，按 `role_count` 降序排列。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn get_permission_usage_stat(&self) -> Result<Vec<PermissionUsageStat>, TraitError>;
}
