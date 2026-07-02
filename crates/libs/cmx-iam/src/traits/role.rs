//! 角色服务 trait 及角色专属审计查询响应结构体。
//!
//! 定义 `RoleService` trait（角色 CRUD、权限/用户分配及审计查询），
//! 以及 `PermissionDiffResponse` 类型。
//! 具体实现位于 `crate::role::service::RoleServiceImpl`。

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_core::model::iam::{Permission, Role};
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use super::audit::{PermissionSummary, RoleSummary};
use crate::role::{RoleFilter, RoleForCreate, RoleForUpdate};

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
    async fn delete_role(
        &self,
        svr_ctx: &SVRContext,
        role_ids: &[String],
    ) -> Result<(), TraitError>;

    /// 分页查询角色。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 角色查询过滤器数组（每个元素是一个 AND 组）。
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
        filters: Option<Vec<RoleFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<Role>, i64), TraitError>;

    /// 列表查询角色（不分页）。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 角色查询过滤器数组（每个元素是一个 AND 组）。
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
        filters: Option<Vec<RoleFilter>>,
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
