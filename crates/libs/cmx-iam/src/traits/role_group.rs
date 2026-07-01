//! 角色组服务 trait。
//!
//! 定义 `RoleGroupService` trait（角色组 CRUD 及树形查询）。
//! 具体实现位于 `crate::role_group::service::RoleGroupServiceImpl`。

use async_trait::async_trait;
use cmx_core::model::iam::{RoleGroup, RoleGroupTreeNode};
use cmx_core::SVRContext;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use crate::role_group::{RoleGroupFilter, RoleGroupForCreate, RoleGroupForUpdate};

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
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 角色组查询过滤器数组（每个元素是一个 AND 组）。
    /// * `list_options` - 分页与排序选项（由 `PageParams::to_list_options()` 构造）。
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
        filters: Option<Vec<RoleGroupFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<RoleGroup>, i64), TraitError>;

    /// 列表查询角色组（不分页）。
    ///
    /// 接收一个 filter 组的数组（`filters`），组间为 OR 关系，组内为 AND。
    /// 当传入 `None` 或空数组时，仅应用默认 `archived = 0` 过滤。
    ///
    /// # Arguments
    ///
    /// * `filters` - 角色组查询过滤器数组（每个元素是一个 AND 组）。
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
        filters: Option<Vec<RoleGroupFilter>>,
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
