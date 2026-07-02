//! 权限服务 trait 及权限专属审计查询响应结构体。
//!
//! 定义 `PermissionService` trait（权限 CRUD、权限树查询及使用统计），
//! 以及 `PermissionUsageStat` 类型。
//! 具体实现位于 `crate::permission::service::PermissionServiceImpl`。

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use crate::permission::{
    DeletePermissionOutcome, PermissionFilter, PermissionForCreate, PermissionForUpdate,
};

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
