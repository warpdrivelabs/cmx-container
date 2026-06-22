//! 权限服务实现 — PermissionServiceImpl

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::permission::{PermissionBmc, PermissionFilter, PermissionForCreate, PermissionForUpdate};
use crate::service_traits::PermissionService;

/// 权限服务实现
pub struct PermissionServiceImpl {
    /// 数据库管理器
    mm: Arc<DatabaseManager>,
    /// 认证库 db_id
    db_id: String,
    /// IAM 配置（预留：用于权限缓存 TTL 等扩展）
    #[allow(dead_code)]
    config: IamConfig,
    /// 审计日志记录器（可选）
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
}

impl PermissionServiceImpl {
    /// 构造函数
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

    /// 设置审计日志记录器（Builder 模式）
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 从 DataSet 第一行提取 Permission
    fn extract_permission(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<Permission, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::PermissionNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<Permission>(json_val)
            .map_err(|e| IamError::Business(format!("权限反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 Permission 列表
    fn extract_permissions(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<Permission> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<Permission>(json_val).ok()
            })
            .collect()
    }

    /// 构造带 archived = 0 默认过滤的 PermissionFilter
    fn with_default_archived(mut filter: PermissionFilter) -> PermissionFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }

    /// 将扁平权限列表组装为树形结构（按 parent_id 递归）
    fn build_tree(permissions: Vec<Permission>) -> Vec<PermissionTreeNode> {
        // 找出根节点（parent_id 为 None 或空字符串）
        let roots: Vec<Permission> = permissions
            .iter()
            .filter(|p| p.parent_id.as_ref().map(|s| s.is_empty()).unwrap_or(true))
            .cloned()
            .collect();

        // 递归构建子树
        roots
            .into_iter()
            .map(|root| Self::build_subtree(root, &permissions))
            .collect()
    }

    /// 递归构建子树
    fn build_subtree(parent: Permission, all: &[Permission]) -> PermissionTreeNode {
        let children: Vec<PermissionTreeNode> = all
            .iter()
            .filter(|p| p.parent_id.as_deref() == Some(&parent.id))
            .cloned()
            .map(|child| Self::build_subtree(child, all))
            .collect();

        PermissionTreeNode {
            permission: parent,
            children,
        }
    }
}

impl AuditHelper for PermissionServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl PermissionService for PermissionServiceImpl {
    /// 创建权限。
    ///
    /// 校验权限编码唯一性后写入数据库，并写入审计日志。
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
    /// * `IamError::PermissionCodeExists` - 权限编码已存在。
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn create_permission(
        &self,
        svr_ctx: &SVRContext,
        data: PermissionForCreate,
    ) -> Result<Permission, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::create_permission - {}",
            "IAM", data.code
        );

        // 检查权限编码唯一性
        let check_sql = "SELECT id FROM cmx_permission WHERE code = $1 AND archived = 0";
        let check_params = Value::Array(vec![Value::String(data.code.clone())]);
        let existing = self
            .mm
            .query_sql_with_json(&self.db_id, None, check_sql, check_params, "check_perm_code")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限编码失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::PermissionCodeExists(data.code.clone())));
        }

        let dataset =
            GenericCrudService::<PermissionBmc>::create(&self.mm, &self.db_id, None, data.clone())
                .await
                .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permission = Self::extract_permission(dataset).map_err(|e| TraitError::from(e))?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "code": &data.code,
            "name": &data.name,
        });
        self.audit_write(svr_ctx, "create_permission", "permission", &permission.id, &audit_detail)
            .await;

        info!(permission_id = %permission.id, code = %data.code, "权限创建成功");
        Ok(permission)
    }

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
    /// * `IamError::PermissionNotFound` - 权限不存在。
    /// * `IamError::Crud` - 数据库查询失败。
    async fn get_permission(&self, permission_id: &str) -> Result<Permission, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::get_permission - {}",
            "IAM", permission_id
        );

        let dataset = GenericCrudService::<PermissionBmc>::get(
            &self.mm,
            &self.db_id,
            None,
            Value::String(permission_id.to_string()),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::PermissionNotFound(permission_id.to_string())));
        }

        Self::extract_permission(dataset).map_err(|e| TraitError::from(e))
    }

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
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::update_permission - {}",
            "IAM", permission_id
        );

        let dataset = GenericCrudService::<PermissionBmc>::update(
            &self.mm,
            &self.db_id,
            None,
            Value::String(permission_id.to_string()),
            data.clone(),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permission = Self::extract_permission(dataset).map_err(|e| TraitError::from(e))?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "description": &data.description,
        });
        self.audit_write(svr_ctx, "update_permission", "permission", permission_id, &audit_detail)
            .await;

        info!(permission_id = permission_id, "权限更新成功");
        Ok(permission)
    }

    /// 批量删除权限（事务保证软删除 + 角色关联清理的原子性）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_ids` - 待删除的权限 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::delete_permission - count: {}",
            "IAM",
            permission_ids.len()
        );

        if permission_ids.is_empty() {
            return Ok(());
        }

        // 使用事务保证软删除+物理删除的原子性
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 软删除 cmx_permission
        for permission_id in permission_ids {
            let sql = "UPDATE cmx_permission SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = Value::Array(vec![Value::String(permission_id.clone())]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("软删除权限失败: {e}"))))?;
        }

        // 2. 物理删除 cmx_role_permission 关联
        for permission_id in permission_ids {
            let sql = "DELETE FROM cmx_role_permission WHERE permission_id = $1";
            let params = Value::Array(vec![Value::String(permission_id.clone())]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("删除权限角色关联失败: {e}"))))?;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 3. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "permission_ids": permission_ids,
            "count": permission_ids.len(),
        });
        self.audit_write(svr_ctx, "delete_permission", "permission", "batch", &audit_detail)
            .await;

        info!(count = permission_ids.len(), "权限删除成功");
        Ok(())
    }

    /// 分页查询权限。
    ///
    /// 默认附加 `archived = 0` 过滤；`current` 从 1 开始。
    ///
    /// # Arguments
    ///
    /// * `filter` - 权限查询过滤器。
    /// * `current` - 当前页码（从 1 开始）。
    /// * `size` - 每页记录数。
    ///
    /// # Returns
    ///
    /// 元组 `(权限列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库分页查询失败。
    async fn page_permissions(
        &self,
        filter: PermissionFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<Permission>, i64), TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::page_permissions - current: {}, size: {}",
            "IAM", current, size
        );

        let filters = Self::with_default_archived(filter);
        let offset = current.saturating_sub(1) * size;
        let list_options = ListOptions::from_offset_limit(offset as i64, size as i64);

        let (dataset, total) =
            GenericCrudService::<PermissionBmc, PermissionFilter>::page(
                &self.mm,
                &self.db_id,
                None,
                Some(vec![filters]),
                list_options,
            )
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permissions = Self::extract_permissions(dataset);
        Ok((permissions, total))
    }

    /// 列表查询权限。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 权限查询过滤器。
    ///
    /// # Returns
    ///
    /// 匹配的权限列表。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库查询失败。
    async fn list_permissions(
        &self,
        filter: PermissionFilter,
    ) -> Result<Vec<Permission>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::list_permissions", "IAM");

        let filters = Self::with_default_archived(filter);

        let dataset = GenericCrudService::<PermissionBmc, PermissionFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            Some(vec![filters]),
            None,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_permissions(dataset))
    }

    /// 获取权限树（递归结构）。
    ///
    /// 一次性加载所有有效权限（`archived = 0 AND status = 1`），
    /// 在内存中按 `parent_id` 递归构建树形结构。
    ///
    /// # Returns
    ///
    /// 树根列表（每个根节点包含嵌套的 `children`）。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - SQL 查询失败。
    async fn get_permission_tree(&self) -> Result<Vec<PermissionTreeNode>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::get_permission_tree", "IAM");

        // 一次性加载所有有效权限，在内存中递归构建树
        let sql = r#"
            SELECT id, code, name, resource_type, parent_id, sort_order, status, description
            FROM cmx_permission
            WHERE archived = 0 AND status = 1
            ORDER BY sort_order ASC NULLS LAST, code ASC
        "#;

        let dataset = self
            .mm
            .query_sql(&self.db_id, None, sql, "permission_tree")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限树失败: {e}"))))?;

        let permissions = Self::extract_permissions(dataset);

        Ok(Self::build_tree(permissions))
    }

    /// 统计每个权限被多少角色使用
    async fn get_permission_usage_stat(
        &self,
    ) -> Result<Vec<crate::service_traits::PermissionUsageStat>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::get_permission_usage_stat", "IAM");

        let sql = r#"
            SELECT p.id, p.code, p.name,
                   COUNT(DISTINCT rp.role_id) AS role_count,
                   COUNT(DISTINCT ur.user_id) AS user_count,
                   MAX(rp.create_time) AS last_assigned_at
            FROM cmx_permission p
            LEFT JOIN cmx_role_permission rp ON rp.permission_id = p.id AND rp.archived = 0
            LEFT JOIN cmx_user_role ur ON ur.role_id = rp.role_id AND ur.archived = 0
            WHERE p.archived = 0 AND p.status = 1
            GROUP BY p.id, p.code, p.name
            ORDER BY role_count DESC, p.sort_order, p.code
        "#;
        let params = serde_json::Value::Array(vec![]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "perm_usage_stat")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询权限使用统计失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let stats: Vec<crate::service_traits::PermissionUsageStat> = dataset
            .iter()
            .filter_map(|row| {
                Some(crate::service_traits::PermissionUsageStat {
                    permission_id: row.get_by_name_as(schema, "id")?,
                    permission_code: row.get_by_name_as(schema, "code")?,
                    permission_name: row.get_by_name_as(schema, "name")?,
                    role_count: row
                        .get_by_name_as::<i64>(schema, "role_count")
                        .unwrap_or(0) as u32,
                    user_count: row
                        .get_by_name_as::<i64>(schema, "user_count")
                        .unwrap_or(0) as u32,
                    last_assigned_at: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "last_assigned_at"),
                })
            })
            .collect();

        Ok(stats)
    }
}
