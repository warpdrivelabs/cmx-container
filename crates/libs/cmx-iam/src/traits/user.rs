//! 用户服务 trait 及用户专属审计查询响应结构体。
//!
//! 定义 `UserService` trait（用户 CRUD、角色分配、临时授权及审计查询），
//! 以及 `TempAssignmentStatusFilter`、`UserRoleAssignment`、`EffectivePermissionsResponse` 等类型。
//! 具体实现位于 `crate::user::service::UserServiceImpl`。

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_core::model::iam::{Role, User};
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use super::audit::{PermissionSummary, RoleSummary};
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
    /// 目标用户名（关联 `cmx_user.username` 查询得到）。
    pub username: String,
    /// 目标用户昵称（关联 `cmx_user.nickname` 查询得到）。
    pub nickname: Option<String>,
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

/// 用户服务 trait。
///
/// 定义用户 CRUD、角色分配、临时授权及审计查询等操作。
/// 实现见 `crate::user::service::UserServiceImpl`。
#[async_trait]
#[allow(clippy::too_many_arguments)] // assign_temp_role 等方法参数由业务契约决定
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
    async fn delete_user(
        &self,
        svr_ctx: &SVRContext,
        user_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 分页查询用户。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 用户查询过滤器数组（每个元素是一个 AND 组）。
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
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64), TraitError>;

    /// 列表查询用户（不分页）。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 用户查询过滤器数组（每个元素是一个 AND 组）。
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
        filters: Option<Vec<UserFilter>>,
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
