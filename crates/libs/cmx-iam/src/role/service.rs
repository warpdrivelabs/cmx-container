//! 角色服务实现 — RoleServiceImpl

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, Role};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use cmx_core::model::cell::DataValue;
use serde_json::Value;
use tracing::{debug, info};
use cmx_utils::snowflake_id_str;
use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::role::{RoleBmc, RoleFilter, RoleForCreate, RoleForUpdate};
use crate::rule::RuleEnforcer;
use crate::service_traits::RoleService;

/// 角色服务实现。
pub struct RoleServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证库 `db_id`。
    db_id: String,

    /// IAM 配置（含 `builtin_role_codes` 保护列表）。
    config: IamConfig,

    /// 审计日志记录器（可选，通过 `with_audit` 注入）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,

    /// 规则校验引擎（可选，用于 SoD 校验）
    rule_enforcer: Option<Arc<dyn RuleEnforcer>>,

    /// 权限校验器引用（可选，用于缓存失效）
    permission_checker: Option<Arc<crate::iam_checker::IamChecker>>,
}

impl RoleServiceImpl {
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
            rule_enforcer: None,
            permission_checker: None,
        }
    }

    /// 设置审计日志记录器（Builder 模式）
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 设置规则校验引擎（Builder 模式）
    pub fn with_rule_enforcer(mut self, enforcer: Arc<dyn RuleEnforcer>) -> Self {
        self.rule_enforcer = Some(enforcer);
        self
    }

    /// 设置权限校验器（Builder 模式，用于缓存失效）
    pub fn with_permission_checker(mut self, checker: Arc<crate::iam_checker::IamChecker>) -> Self {
        self.permission_checker = Some(checker);
        self
    }

    /// 从 DataSet 第一行提取 Role
    fn extract_role(dataset: cmx_core::model::data::dataset::DataSet) -> Result<Role, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::RoleNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<Role>(json_val)
            .map_err(|e| IamError::Business(format!("角色反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 Role 列表
    fn extract_roles(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<Role> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<Role>(json_val).ok()
            })
            .collect()
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

    /// 构造带 archived = 0 默认过滤的 RoleFilter
    fn with_default_archived(mut filter: RoleFilter) -> RoleFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }
}

impl AuditHelper for RoleServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl RoleService for RoleServiceImpl {
    /// 创建角色。
    ///
    /// 校验角色编码唯一性后写入数据库，并写入审计日志。
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
    /// * `IamError::RoleCodeExists` - 角色编码已存在。
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn create_role(
        &self,
        svr_ctx: &SVRContext,
        data: RoleForCreate,
    ) -> Result<Role, TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::create_role - {}",
            "IAM", data.code
        );

        // 检查角色编码唯一性
        let check_sql = "SELECT id FROM cmx_role WHERE code = $1 AND archived = 0";
        let check_params = vec![DataValue::String(data.code.clone())];
        let existing = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, check_sql, check_params, "check_role_code")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询角色编码失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::RoleCodeExists(data.code.clone())));
        }

        let dataset = GenericCrudService::<RoleBmc>::create(&self.mm, &self.db_id, None, data.clone())
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let role = Self::extract_role(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "code": &data.code,
            "name": &data.name,
        });
        self.audit_write(svr_ctx, "create_role", "role", &role.id, &audit_detail)
            .await;

        info!(role_id = %role.id, code = %data.code, "角色创建成功");
        Ok(role)
    }

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
    /// * `IamError::RoleNotFound` - 角色不存在。
    /// * `IamError::Crud` - 数据库查询失败。
    async fn get_role(&self, role_id: &str) -> Result<Role, TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::get_role - {}",
            "IAM", role_id
        );

        let dataset = GenericCrudService::<RoleBmc>::get(
            &self.mm,
            &self.db_id,
            None,
            Value::String(role_id.to_string()),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::RoleNotFound(role_id.to_string())));
        }

        Self::extract_role(dataset).map_err(TraitError::from)
    }

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
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn update_role(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        data: RoleForUpdate,
    ) -> Result<Role, TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::update_role - {}",
            "IAM", role_id
        );

        let dataset = GenericCrudService::<RoleBmc>::update(
            &self.mm,
            &self.db_id,
            None,
            Value::String(role_id.to_string()),
            data.clone(),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let role = Self::extract_role(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "description": &data.description,
        });
        self.audit_write(svr_ctx, "update_role", "role", role_id, &audit_detail)
            .await;

        info!(role_id = role_id, "角色更新成功");
        Ok(role)
    }

    /// 批量删除角色（事务保证软删除 + 权限关联清理的原子性）。
    ///
    /// 内置角色（`builtin_role_codes` 配置项）受保护，不可删除。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_ids` - 待删除的角色 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * `IamError::CannotDeleteBuiltinRole` - 尝试删除内置角色。
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn delete_role(
        &self,
        svr_ctx: &SVRContext,
        role_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::delete_role - count: {}",
            "IAM",
            role_ids.len()
        );

        if role_ids.is_empty() {
            return Ok(());
        }

        // 1. 内置角色保护检查
        for role_id in role_ids {
            let role = self.get_role(role_id).await?;
            if self.config.builtin_role_codes.contains(&role.code) {
                return Err(TraitError::from(IamError::CannotDeleteBuiltinRole));
            }
        }

        // 使用事务保证软删除+物理删除的原子性
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 2. 软删除 cmx_role
        for role_id in role_ids {
            let sql = "UPDATE cmx_role SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = vec![DataValue::String(role_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("软删除角色失败: {e}"))))?;
        }

        // 3. 物理删除 cmx_role_permission 关联
        for role_id in role_ids {
            let sql = "DELETE FROM cmx_role_permission WHERE role_id = $1";
            let params = vec![DataValue::String(role_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("删除角色权限关联失败: {e}"))))?;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 4. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "role_ids": role_ids,
            "count": role_ids.len(),
        });
        self.audit_write(svr_ctx, "delete_role", "role", "batch", &audit_detail)
            .await;

        info!(count = role_ids.len(), "角色删除成功");
        Ok(())
    }

    /// 分页查询角色。
    ///
    /// 默认附加 `archived = 0` 过滤；`current` 从 1 开始。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色查询过滤器。
    /// * `current` - 当前页码（从 1 开始）。
    /// * `size` - 每页记录数。
    ///
    /// # Returns
    ///
    /// 元组 `(角色列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库分页查询失败。
    async fn page_roles(
        &self,
        filter: RoleFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<Role>, i64), TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::page_roles - current: {}, size: {}",
            "IAM", current, size
        );

        let filters = Self::with_default_archived(filter);
        let offset = current.saturating_sub(1) * size;
        let list_options = ListOptions::from_offset_limit(offset as i64, size as i64);

        let (dataset, total) =
            GenericCrudService::<RoleBmc, RoleFilter>::page(
                &self.mm,
                &self.db_id,
                None,
                Some(vec![filters]),
                list_options,
            )
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let roles = Self::extract_roles(dataset);
        Ok((roles, total))
    }

    /// 列表查询角色。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 角色查询过滤器。
    ///
    /// # Returns
    ///
    /// 匹配的角色列表。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库查询失败。
    async fn list_roles(&self, filter: RoleFilter) -> Result<Vec<Role>, TraitError> {
        debug!("{:<12} - RoleServiceImpl::list_roles", "IAM");

        let filters = Self::with_default_archived(filter);

        let dataset = GenericCrudService::<RoleBmc, RoleFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            Some(vec![filters]),
            None,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_roles(dataset))
    }

    /// 为角色分配权限（全量替换，事务保证原子性）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `role_id` - 目标角色 ID。
    /// * `permission_ids` - 待分配的权限 ID 列表；空数组表示清空所有权限。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn assign_permissions(
        &self,
        svr_ctx: &SVRContext,
        role_id: &str,
        permission_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::assign_permissions - role: {}, perm_count: {}",
            "IAM", role_id, permission_ids.len()
        );

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 0. SoD 规则校验（仅当启用时）
        if let Some(enforcer) = &self.rule_enforcer {
            enforcer
                .check_role_permissions(permission_ids)
                .await
                .map_err(TraitError::from)?;
        }

        // 1. 物理删除旧关联
        let delete_sql = "DELETE FROM cmx_role_permission WHERE role_id = $1";
        let delete_params = vec![DataValue::String(role_id.to_string())];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), delete_sql, delete_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除旧权限关联失败: {e}"))))?;

        // 2. 批量插入新关联
        for perm_id in permission_ids {
            let rp_id = snowflake_id_str();
            let insert_sql = "INSERT INTO cmx_role_permission (id, role_id, permission_id, archived) \
                              VALUES ($1, $2, $3, 0) ON CONFLICT (role_id, permission_id) DO NOTHING";
            let params = vec![
                DataValue::String(rp_id),
                DataValue::String(role_id.to_string()),
                DataValue::String(perm_id.clone()),
            ];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("插入角色权限关联失败: {e}"))))?;
        }

        // 3. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 4. 审计日志
        let audit_detail = serde_json::json!({
            "role_id": role_id,
            "permission_ids": permission_ids,
        });
        self.audit_write(svr_ctx, "assign_permissions", "role", role_id, &audit_detail)
            .await;

        // 5. 失效缓存（角色权限变更后，关联用户的权限缓存需失效）
        if let Some(checker) = &self.permission_checker {
            checker.invalidate_role_cache(role_id).await;
        }

        info!(role_id = role_id, perm_count = permission_ids.len(), "角色权限分配成功");
        Ok(())
    }

    /// 获取角色已启用的权限列表（含 `status = 1` 且 `archived = 0` 过滤）。
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
    /// * `IamError::Business` - SQL 查询失败。
    async fn get_role_permissions(&self, role_id: &str) -> Result<Vec<Permission>, TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::get_role_permissions - role: {}",
            "IAM", role_id
        );

        let sql = r#"
            SELECT p.id, p.code, p.name, p.resource_type, p.parent_id, p.sort_order,
                   p.status, p.description, p.archived,
                   p.domain_code, p.app_code, p.module_code, p.extension,
                   p.create_time, p.update_time,
                   p.create_by, p.create_name, p.update_by, p.update_name
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            WHERE rp.role_id = $1 AND rp.archived = 0 AND p.archived = 0 AND p.status = 1
        "#;
        let params = vec![DataValue::String(role_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "role_permissions")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询角色权限失败: {e}"))))?;

        Ok(Self::extract_permissions(dataset))
    }

    /// 比较两个角色的权限差异
    async fn get_permission_diff(
        &self,
        role_id_1: &str,
        role_id_2: &str,
    ) -> Result<crate::service_traits::PermissionDiffResponse, TraitError> {
        debug!(
            "{:<12} - RoleServiceImpl::get_permission_diff - r1: {}, r2: {}",
            "IAM", role_id_1, role_id_2
        );

        // 查询两个角色信息
        let role1 = self.get_role(role_id_1).await?;
        let role2 = self.get_role(role_id_2).await?;

        // 查询两个角色的权限列表
        let perms1 = self.get_role_permissions(role_id_1).await?;
        let perms2 = self.get_role_permissions(role_id_2).await?;

        // 构建权限ID集合用于差异比较
        use std::collections::HashSet;
        let set1: HashSet<String> = perms1.iter().map(|p| p.id.clone()).collect();
        let set2: HashSet<String> = perms2.iter().map(|p| p.id.clone()).collect();

        let to_summary = |p: &Permission| crate::service_traits::PermissionSummary {
            id: p.id.clone(),
            code: p.code.clone(),
            name: p.name.clone(),
            resource_type: p.resource_type.clone(),
            description: p.description.clone(),
        };

        let only_in_role_1: Vec<_> = perms1.iter().filter(|p| !set2.contains(&p.id)).map(to_summary).collect();
        let only_in_role_2: Vec<_> = perms2.iter().filter(|p| !set1.contains(&p.id)).map(to_summary).collect();
        let common: Vec<_> = perms1.iter().filter(|p| set2.contains(&p.id)).map(to_summary).collect();

        Ok(crate::service_traits::PermissionDiffResponse {
            role_1: crate::service_traits::RoleSummary {
                id: role1.id,
                code: role1.code,
                name: role1.name,
                description: role1.description,
            },
            role_2: crate::service_traits::RoleSummary {
                id: role2.id,
                code: role2.code,
                name: role2.name,
                description: role2.description,
            },
            only_in_role_1,
            only_in_role_2,
            common,
        })
    }
}

