//! 角色组服务实现 — `RoleGroupServiceImpl`。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{RoleGroup, RoleGroupTreeNode};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use cmx_core::model::cell::DataValue;
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::role_group::{RoleGroupBmc, RoleGroupFilter, RoleGroupForCreate, RoleGroupForUpdate};
use crate::service_traits::RoleGroupService;

/// 角色组服务实现。
pub struct RoleGroupServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,
    /// 认证库 `db_id`。
    db_id: String,
    /// IAM 配置（预留）。
    #[allow(dead_code)]
    config: IamConfig,
    /// 审计日志记录器（可选）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
}

impl RoleGroupServiceImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id`。
    ///
    /// # Returns
    ///
    /// 返回 `RoleGroupServiceImpl` 实例，未设置审计记录器。
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self {
            mm,
            db_id,
            config,
            audit: None,
        }
    }

    /// 设置审计日志记录器（Builder 模式）。
    ///
    /// # Arguments
    ///
    /// * `audit` - 审计日志记录器。
    ///
    /// # Returns
    ///
    /// 返回 `Self`，便于链式调用。
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 从 DataSet 第一行提取 `RoleGroup`。
    fn extract_role_group(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<RoleGroup, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::RoleGroupNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<RoleGroup>(json_val)
            .map_err(|e| IamError::Business(format!("角色组反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 `RoleGroup` 列表。
    fn extract_role_groups(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<RoleGroup> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<RoleGroup>(json_val).ok()
            })
            .collect()
    }

    /// 构造带 `archived = 0` 默认过滤的 `RoleGroupFilter`。
    fn with_default_archived(mut filter: RoleGroupFilter) -> RoleGroupFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }

    /// 将扁平角色组列表组装为树形结构（按 `parent_id` 递归）。
    fn build_tree(role_groups: Vec<RoleGroup>) -> Vec<RoleGroupTreeNode> {
        // 找出根节点（parent_id 为 None 或空字符串）
        let roots: Vec<RoleGroup> = role_groups
            .iter()
            .filter(|g| g.parent_id.as_ref().map(|s| s.is_empty()).unwrap_or(true))
            .cloned()
            .collect();

        // 递归构建子树
        roots
            .into_iter()
            .map(|root| Self::build_subtree(root, &role_groups))
            .collect()
    }

    /// 递归构建子树。
    fn build_subtree(parent: RoleGroup, all: &[RoleGroup]) -> RoleGroupTreeNode {
        let children: Vec<RoleGroupTreeNode> = all
            .iter()
            .filter(|g| g.parent_id.as_deref() == Some(parent.id.as_str()))
            .cloned()
            .map(|child| Self::build_subtree(child, all))
            .collect();

        RoleGroupTreeNode {
            role_group: parent,
            children,
        }
    }
}

impl AuditHelper for RoleGroupServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl RoleGroupService for RoleGroupServiceImpl {
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
    async fn create_role_group(
        &self,
        svr_ctx: &SVRContext,
        data: RoleGroupForCreate,
    ) -> Result<RoleGroup, TraitError> {
        debug!(
            "{:<12} - RoleGroupServiceImpl::create_role_group - {}",
            "IAM", data.name
        );

        let dataset =
            GenericCrudService::<RoleGroupBmc>::create(&self.mm, &self.db_id, None, data.clone())
                .await
                .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let role_group = Self::extract_role_group(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "parent_id": &data.parent_id,
        });
        self.audit_write(svr_ctx, "create_role_group", "role_group", &role_group.id, &audit_detail)
            .await;

        info!(role_group_id = %role_group.id, name = %data.name, "角色组创建成功");
        Ok(role_group)
    }

    /// 获取单个角色组。
    ///
    /// # Arguments
    ///
    /// * `role_group_id` - 角色组唯一标识。
    ///
    /// # Errors
    ///
    /// * `IamError::RoleGroupNotFound` - 角色组不存在。
    async fn get_role_group(&self, role_group_id: &str) -> Result<RoleGroup, TraitError> {
        debug!(
            "{:<12} - RoleGroupServiceImpl::get_role_group - {}",
            "IAM", role_group_id
        );

        let dataset = GenericCrudService::<RoleGroupBmc>::get(
            &self.mm,
            &self.db_id,
            None,
            Value::String(role_group_id.to_string()),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::RoleGroupNotFound(
                role_group_id.to_string(),
            )));
        }

        Self::extract_role_group(dataset).map_err(TraitError::from)
    }

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
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn update_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_id: &str,
        data: RoleGroupForUpdate,
    ) -> Result<RoleGroup, TraitError> {
        debug!(
            "{:<12} - RoleGroupServiceImpl::update_role_group - {}",
            "IAM", role_group_id
        );

        let dataset = GenericCrudService::<RoleGroupBmc>::update(
            &self.mm,
            &self.db_id,
            None,
            Value::String(role_group_id.to_string()),
            data.clone(),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let role_group = Self::extract_role_group(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "description": &data.description,
        });
        self.audit_write(svr_ctx, "update_role_group", "role_group", role_group_id, &audit_detail)
            .await;

        info!(role_group_id = role_group_id, "角色组更新成功");
        Ok(role_group)
    }

    /// 批量删除角色组（软删除）。
    ///
    /// 删除前校验：待删除角色组下不能存在子角色组或关联角色。
    ///
    /// # Errors
    ///
    /// * `IamError::RoleGroupInUse` - 角色组下存在子组或关联角色。
    async fn delete_role_group(
        &self,
        svr_ctx: &SVRContext,
        role_group_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - RoleGroupServiceImpl::delete_role_group - count: {}",
            "IAM",
            role_group_ids.len()
        );

        if role_group_ids.is_empty() {
            return Ok(());
        }

        // 1. 检查待删除角色组下是否有子角色组
        let child_check_sql =
            "SELECT id FROM cmx_role_group WHERE parent_id = ANY($1) AND archived = 0 LIMIT 1";
        let child_params: Vec<DataValue> = role_group_ids
                .iter()
                .map(|id| DataValue::String(id.clone()))
                .collect();
        let existing = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, child_check_sql, child_params, "check_child_groups")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询子角色组失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::RoleGroupInUse));
        }

        // 2. 检查待删除角色组下是否有关联角色
        let role_check_sql =
            "SELECT id FROM cmx_role WHERE role_group_id = ANY($1) AND archived = 0 LIMIT 1";
        let role_params: Vec<DataValue> = role_group_ids
                .iter()
                .map(|id| DataValue::String(id.clone()))
                .collect();
        let existing = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, role_check_sql, role_params, "check_role_group_usage")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询关联角色失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::RoleGroupInUse));
        }

        // 3. 软删除角色组
        for role_group_id in role_group_ids {
            let sql = "UPDATE cmx_role_group SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = vec![DataValue::String(role_group_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, None, sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("软删除角色组失败: {e}"))))?;
        }

        // 4. 审计日志
        let audit_detail = serde_json::json!({
            "role_group_ids": role_group_ids,
            "count": role_group_ids.len(),
        });
        self.audit_write(svr_ctx, "delete_role_group", "role_group", "batch", &audit_detail)
            .await;

        info!(count = role_group_ids.len(), "角色组删除成功");
        Ok(())
    }

    /// 分页查询角色组。
    ///
    /// 默认附加 `archived = 0` 过滤；`current` 从 1 开始。
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
    /// * `IamError::Crud` - 数据库分页查询失败。
    async fn page_role_groups(
        &self,
        filters: Option<Vec<RoleGroupFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<RoleGroup>, i64), TraitError> {
        debug!("{:<12} - RoleGroupServiceImpl::page_role_groups", "IAM");

        // 对每个 filter 组注入默认 archived = 0
        let filters = filters.map(|fs| {
            fs.into_iter().map(Self::with_default_archived).collect::<Vec<_>>()
        });

        let (dataset, total) =
            GenericCrudService::<RoleGroupBmc, RoleGroupFilter>::page(
                &self.mm,
                &self.db_id,
                None,
                filters,
                list_options,
            )
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let role_groups = Self::extract_role_groups(dataset);
        Ok((role_groups, total))
    }

    /// 列表查询角色组。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filters` - 角色组查询过滤器数组（每个 filter 为一个 AND 组，组间 OR）。
    ///
    /// # Returns
    ///
    /// 匹配的角色组列表。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库查询失败。
    async fn list_role_groups(
        &self,
        filters: Option<Vec<RoleGroupFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<RoleGroup>, TraitError> {
        debug!("{:<12} - RoleGroupServiceImpl::list_role_groups", "IAM");

        // 对每个 filter 组注入默认 archived = 0
        let filters = filters.map(|fs| {
            fs.into_iter().map(Self::with_default_archived).collect::<Vec<_>>()
        });

        let dataset = GenericCrudService::<RoleGroupBmc, RoleGroupFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_role_groups(dataset))
    }

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
    /// * `IamError::Business` - SQL 查询失败。
    async fn get_role_group_tree(&self) -> Result<Vec<RoleGroupTreeNode>, TraitError> {
        debug!("{:<12} - RoleGroupServiceImpl::get_role_group_tree", "IAM");

        let sql = r#"
            SELECT id, name, parent_id, sort_order, description, archived,
                   create_time, update_time,
                   create_by, create_name, update_by, update_name
            FROM cmx_role_group
            WHERE archived = 0
            ORDER BY sort_order ASC NULLS LAST
        "#;

        let dataset = self
            .mm
            .query_sql(&self.db_id, None, sql, "role_group_tree")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询角色组树失败: {e}"))))?;

        let role_groups = Self::extract_role_groups(dataset);

        Ok(Self::build_tree(role_groups))
    }
}
