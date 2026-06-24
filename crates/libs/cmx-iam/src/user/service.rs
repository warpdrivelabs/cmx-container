//! 用户服务实现 — UserServiceImpl

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{Role, User};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::auth::AuthService;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use serde_json::Value;
use tracing::{debug, info};
use cmx_utils::snowflake_id_str;
use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::rule::RuleEnforcer;
use crate::service_traits::{
    EffectivePermissionsResponse, PermissionSummary, RoleSummary, TempAssignmentStatusFilter,
    UserRoleAssignment,
};
use crate::service_traits::UserService;
use crate::user::{
    UserBmc, UserFilter, UserForCreate, UserForInsert, UserForUpdate, UserForUpdateInsert,
};

/// 用户服务实现。
pub struct UserServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证服务（用于密码哈希）。
    auth: Arc<dyn AuthService>,

    /// 认证库 `db_id`。
    db_id: String,

    /// IAM 配置。
    config: IamConfig,

    /// 审计日志记录器（可选，通过 `with_audit` 注入）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,

    /// 规则校验引擎（可选，用于 SoD 校验）
    rule_enforcer: Option<Arc<dyn RuleEnforcer>>,

    /// 权限校验器引用（可选，用于缓存失效）
    permission_checker: Option<Arc<crate::iam_checker::IamChecker>>,
}

impl UserServiceImpl {
    /// 构造函数
    pub async fn new(
        mm: Arc<DatabaseManager>,
        auth: Arc<dyn AuthService>,
        config: IamConfig,
    ) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self {
            mm,
            auth,
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

    /// 从 DataSet 第一行提取 User
    fn extract_user(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<User, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::UserNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<User>(json_val)
            .map_err(|e| IamError::Business(format!("用户反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 User 列表
    fn extract_users(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<User> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<User>(json_val).ok()
            })
            .collect()
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

    /// 从 DataSet 提取 UserRoleAssignment 列表
    fn extract_assignments(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Vec<UserRoleAssignment> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                Some(UserRoleAssignment {
                    id: row.get_by_name_as(schema, "id")?,
                    user_id: row.get_by_name_as(schema, "user_id")?,
                    role_id: row.get_by_name_as(schema, "role_id")?,
                    role_code: row.get_by_name_as(schema, "code").unwrap_or_default(),
                    role_name: row.get_by_name_as(schema, "name").unwrap_or_default(),
                    effective_from: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "effective_from")?,
                    effective_until: row.get_by_name_as::<chrono::DateTime<chrono::Utc>>(
                        schema,
                        "effective_until",
                    )?,
                    reason: row.get_by_name_as(schema, "reason"),
                    source: row
                        .get_by_name_as(schema, "source")
                        .unwrap_or_else(|| "manual".to_string()),
                    status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
                    revoked_by: row.get_by_name_as(schema, "revoked_by"),
                    revoked_at: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "revoked_at"),
                    create_time: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "create_time")
                        .unwrap_or_else(chrono::Utc::now),
                })
            })
            .collect()
    }

    /// 构造带 archived = 0 默认过滤的 UserFilter
    fn with_default_archived(mut filter: UserFilter) -> UserFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }
}

impl AuditHelper for UserServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl UserService for UserServiceImpl {
    /// 创建用户。
    ///
    /// 校验用户名唯一性、密码长度，调用 `AuthService` 哈希密码后写入数据库，
    /// 并写入审计日志。
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
    /// * `IamError::Business` - 密码长度不满足配置要求。
    /// * `IamError::UsernameExists` - 用户名已存在。
    /// * `IamError::PasswordHashError` - 密码哈希失败。
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn create_user(
        &self,
        svr_ctx: &SVRContext,
        data: UserForCreate,
    ) -> Result<User, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::create_user - {}",
            "IAM", data.username
        );

        // 1. 校验密码长度
        if data.password.len() < self.config.password_min_length {
            return Err(TraitError::from(IamError::Business(format!(
                "密码长度不能少于 {} 位",
                self.config.password_min_length
            ))));
        }

        // 2. 检查用户名唯一性
        let check_sql = "SELECT id FROM cmx_user WHERE username = $1 AND archived = 0";
        let check_params = Value::Array(vec![Value::String(data.username.clone())]);
        let existing = self
            .mm
            .query_sql_with_json(&self.db_id, None, check_sql, check_params, "check_username")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户名失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::UsernameExists(data.username.clone())));
        }

        // 3. 密码哈希
        let password_hash = self
            .auth
            .hash_password(&data.password)
            .await
            .map_err(|e| TraitError::from(IamError::PasswordHashError(e.to_string())))?;

        // 4. 构建入库结构并创建
        let insert_data = UserForInsert {
            username: data.username.clone(),
            nickname: data.nickname.clone(),
            email: data.email.clone(),
            phone: data.phone.clone(),
            password_hash,
            avatar: data.avatar.clone(),
            org_id: data.org_id.clone(),
            description: data.description.clone(),
            status: data.status.or(Some(1)),
        };

        let dataset =
            GenericCrudService::<UserBmc>::create(&self.mm, &self.db_id, None, insert_data)
                .await
                .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let user = Self::extract_user(dataset).map_err(TraitError::from)?;

        // 5. 审计日志（脱敏后记录）
        let audit_detail = serde_json::json!({
            "username": &data.username,
            "email": &data.email,
            "password_changed": true,
        });
        self.audit_write(svr_ctx, "create_user", "user", &user.id, &audit_detail)
            .await;

        info!(user_id = %user.id, username = %data.username, "用户创建成功");
        Ok(user)
    }

    /// 获取单个用户（按 username 查询）。
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
    /// * `IamError::UserNotFound` - 用户不存在。
    /// * `IamError::Crud` - 数据库查询失败。
    async fn get_user(&self, username: &str) -> Result<User, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user - username: {}",
            "IAM", username
        );

        let sql = r#"
            SELECT id, username, nickname, email, phone, avatar, org_id, description,
                   status, last_login_at, last_login_ip, archived,
                   create_time, update_time,
                   create_by, create_name, update_by, update_name
            FROM cmx_user
            WHERE username = $1 AND archived = 0
        "#;
        let params = Value::Array(vec![Value::String(username.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "get_user_by_username")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户失败: {e}"))))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::UserNotFound(username.to_string())));
        }

        Self::extract_user(dataset).map_err(TraitError::from)
    }

    /// 更新用户。
    ///
    /// 支持可选密码更新（提供时触发哈希 + 写入），并写入审计日志。
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
    /// * `IamError::Business` - 密码长度不满足配置要求。
    /// * `IamError::PasswordHashError` - 密码哈希失败。
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn update_user(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        data: UserForUpdate,
    ) -> Result<User, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::update_user - {}",
            "IAM", user_id
        );

        // 若提供了密码，校验长度并哈希
        let password_hash = if let Some(ref pwd) = data.password {
            if pwd.len() < self.config.password_min_length {
                return Err(TraitError::from(IamError::Business(format!(
                    "密码长度不能少于 {} 位",
                    self.config.password_min_length
                ))));
            }
            let hash = self
                .auth
                .hash_password(pwd)
                .await
                .map_err(|e| TraitError::from(IamError::PasswordHashError(e.to_string())))?;
            Some(hash)
        } else {
            None
        };

        let update_data = UserForUpdateInsert {
            nickname: data.nickname.clone(),
            email: data.email.clone(),
            phone: data.phone.clone(),
            password_hash,
            avatar: data.avatar.clone(),
            description: data.description.clone(),
            status: data.status,
        };

        let dataset = GenericCrudService::<UserBmc>::update(
            &self.mm,
            &self.db_id,
            None,
            Value::String(user_id.to_string()),
            update_data,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let user = Self::extract_user(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "nickname": &data.nickname,
            "email": &data.email,
            "password_changed": data.password.is_some(),
        });
        self.audit_write(svr_ctx, "update_user", "user", user_id, &audit_detail)
            .await;

        info!(user_id = user_id, "用户更新成功");
        Ok(user)
    }

    /// 批量删除用户（事务保证软删除 + 角色关联清理的原子性）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `user_ids` - 待删除的用户 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn delete_user(
        &self,
        svr_ctx: &SVRContext,
        user_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::delete_user - count: {}",
            "IAM",
            user_ids.len()
        );

        if user_ids.is_empty() {
            return Ok(());
        }

        // 使用事务保证软删除+物理删除的原子性
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 软删除 cmx_user（archived = 1）
        for user_id in user_ids {
            let sql = "UPDATE cmx_user SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = Value::Array(vec![Value::String(user_id.clone())]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("软删除用户失败: {e}"))))?;
        }

        // 2. 物理删除 cmx_user_role 关联
        for user_id in user_ids {
            let sql = "DELETE FROM cmx_user_role WHERE user_id = $1";
            let params = Value::Array(vec![Value::String(user_id.clone())]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("删除用户角色关联失败: {e}"))))?;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 3. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "user_ids": user_ids,
            "count": user_ids.len(),
        });
        self.audit_write(svr_ctx, "delete_user", "user", "batch", &audit_detail)
            .await;

        info!(count = user_ids.len(), "用户删除成功");
        Ok(())
    }

    /// 分页查询用户。
    ///
    /// 默认附加 `archived = 0` 过滤；`current` 从 1 开始。
    ///
    /// # Arguments
    ///
    /// * `filter` - 用户查询过滤器。
    /// * `current` - 当前页码（从 1 开始）。
    /// * `size` - 每页记录数。
    ///
    /// # Returns
    ///
    /// 元组 `(用户列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库分页查询失败。
    async fn page_users(
        &self,
        filter: UserFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<User>, i64), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::page_users - current: {}, size: {}",
            "IAM", current, size
        );

        let filters = Self::with_default_archived(filter);
        let offset = current.saturating_sub(1) * size;
        let list_options = ListOptions::from_offset_limit(offset as i64, size as i64);

        let (dataset, total) =
            GenericCrudService::<UserBmc, UserFilter>::page(
                &self.mm,
                &self.db_id,
                None,
                Some(vec![filters]),
                list_options,
            )
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let users = Self::extract_users(dataset);
        Ok((users, total))
    }

    /// 列表查询用户。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 用户查询过滤器。
    ///
    /// # Returns
    ///
    /// 匹配的用户列表。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库查询失败。
    async fn list_users(&self, filter: UserFilter) -> Result<Vec<User>, TraitError> {
        debug!("{:<12} - UserServiceImpl::list_users", "IAM");

        let filters = Self::with_default_archived(filter);

        let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            Some(vec![filters]),
            None,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_users(dataset))
    }

    /// 为用户分配角色（全量替换，事务保证原子性，按 username 查询）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `username` - 目标用户名。
    /// * `role_ids` - 待分配的角色 ID 列表；空数组表示清空所有角色。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    /// * `IamError::UserNotFound` - 用户名不存在。
    async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        username: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::assign_roles - username: {}, role_count: {}",
            "IAM", username, role_ids.len()
        );

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 先解析 username → user_id（校验用户存在）
        let resolve_sql = "SELECT id FROM cmx_user WHERE username = $1 AND archived = 0";
        let resolve_params = Value::Array(vec![Value::String(username.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, Some(txn_id), resolve_sql, resolve_params, "resolve_user_id")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let user_id = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<String>(schema, "id"))
            .ok_or_else(|| TraitError::from(IamError::UserNotFound(username.to_string())))?;

        // 0. SoD 规则校验（仅当启用时）
        if let Some(enforcer) = &self.rule_enforcer {
            enforcer
                .check_user_roles(&user_id, role_ids)
                .await
                .map_err(TraitError::from)?;
        }

        // 1. 物理删除旧关联
        let delete_sql = "DELETE FROM cmx_user_role WHERE user_id = $1";
        let delete_params = Value::Array(vec![Value::String(user_id.clone())]);
        self.mm
            .execute_sql_with_json(&self.db_id, Some(txn_id), delete_sql, delete_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除旧角色关联失败: {e}"))))?;

        // 2. 批量插入新关联
        for role_id in role_ids {
            let ur_id = snowflake_id_str();
            let insert_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived) \
                              VALUES ($1, $2, $3, 0) ON CONFLICT (user_id, role_id) DO NOTHING";
            let params = Value::Array(vec![
                Value::String(ur_id),
                Value::String(user_id.clone()),
                Value::String(role_id.clone()),
            ]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), insert_sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("插入用户角色关联失败: {e}"))))?;
        }

        // 3. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 4. 审计日志（提交后记录）
        let audit_detail = serde_json::json!({
            "username": username,
            "user_id": user_id,
            "role_ids": role_ids,
        });
        self.audit_write(svr_ctx, "assign_roles", "user", &user_id, &audit_detail)
            .await;

        // 5. 失效用户缓存
        if let Some(checker) = &self.permission_checker {
            checker.invalidate_user_cache(&user_id).await;
        }

        info!(username = username, user_id = %user_id, role_count = role_ids.len(), "用户角色分配成功");
        Ok(())
    }

    /// 获取用户已启用的角色列表（含 `status = 1` 且 `archived = 0` 过滤，按 username 查询）。
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
    /// * `IamError::Business` - SQL 查询失败。
    async fn get_user_roles(&self, username: &str) -> Result<Vec<Role>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user_roles - username: {}",
            "IAM", username
        );

        let sql = r#"
            SELECT r.id, r.code, r.name, r.data_scope, r.sort_order, r.status, r.description,
                   r.archived, r.create_time, r.update_time,
                   r.create_by, r.create_name, r.update_by, r.update_name
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            INNER JOIN cmx_user u ON u.id = ur.user_id
            WHERE u.username = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
              AND u.archived = 0
        "#;
        let params = Value::Array(vec![Value::String(username.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "user_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户角色失败: {e}"))))?;

        Ok(Self::extract_roles(dataset))
    }

    /// 分配临时角色（带有效期）。
    ///
    /// 为指定用户分配一个临时角色授权，支持有效期范围、来源标记和撤销原因。
    /// 当配置了 SoD 规则执行器时，会先校验角色互斥约束。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务上下文，包含认证信息用于审计。
    /// * `user_id` - 目标用户 ID。
    /// * `role_id` - 待分配的角色 ID。
    /// * `effective_from` - 授权生效时间。
    /// * `effective_until` - 授权失效时间，必须晚于 `effective_from`。
    /// * `reason` - 授权原因（可选）。
    /// * `source` - 授权来源标记。
    ///
    /// # Returns
    ///
    /// 成功时返回 `UserRoleAssignment`，包含完整的授权记录。
    ///
    /// # Errors
    ///
    /// * 当 `effective_until <= effective_from` 时返回业务错误。
    /// * 当 SoD 规则校验失败时返回规则违反错误。
    /// * 当数据库插入或查询失败时返回内部错误。
    async fn assign_temp_role(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        role_id: &str,
        effective_from: chrono::DateTime<chrono::Utc>,
        effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
        source: &str,
    ) -> Result<UserRoleAssignment, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::assign_temp_role - user: {}, role: {}",
            "IAM", user_id, role_id
        );

        if effective_until <= effective_from {
            return Err(TraitError::from(IamError::Business(
                "effective_until 必须晚于 effective_from".to_string(),
            )));
        }

        // SoD 规则校验（仅当启用时）
        if let Some(enforcer) = &self.rule_enforcer {
            enforcer
                .check_user_roles(user_id, &[role_id.to_string()])
                .await
                .map_err(TraitError::from)?;
        }

        let assignment_id = snowflake_id_str();
        let insert_sql = r#"
            INSERT INTO cmx_user_role_assignment
                (id, user_id, role_id, effective_from, effective_until, reason, source, status, archived)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 0)
        "#;
        let params = Value::Array(vec![
            Value::String(assignment_id.clone()),
            Value::String(user_id.to_string()),
            Value::String(role_id.to_string()),
            Value::String(effective_from.to_rfc3339()),
            Value::String(effective_until.to_rfc3339()),
            reason.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
            Value::String(source.to_string()),
        ]);
        self.mm
            .execute_sql_with_json(&self.db_id, None, insert_sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("分配临时角色失败: {e}")))
            })?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "user_id": user_id,
            "role_id": role_id,
            "effective_from": effective_from.to_rfc3339(),
            "effective_until": effective_until.to_rfc3339(),
            "reason": reason,
            "source": source,
        });
        self.audit_write(svr_ctx, "assign_temp_role", "user", user_id, &audit_detail)
            .await;

        // 失效用户缓存
        if let Some(checker) = &self.permission_checker {
            checker.invalidate_user_cache(user_id).await;
        }

        // 查询返回完整记录（含 role_code/role_name）
        let query_sql = r#"
            SELECT a.id, a.user_id, a.role_id, a.effective_from, a.effective_until,
                   a.reason, a.source, a.status, a.revoked_by, a.revoked_at, a.create_time,
                   r.code, r.name
            FROM cmx_user_role_assignment a
            INNER JOIN cmx_role r ON r.id = a.role_id
            WHERE a.id = $1
        "#;
        let params = Value::Array(vec![Value::String(assignment_id)]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, query_sql, params, "temp_assignment")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询临时授权失败: {e}")))
            })?;

        Self::extract_assignments(dataset)
            .into_iter()
            .next()
            .ok_or_else(|| {
                TraitError::from(IamError::Business("临时授权创建后查询失败".to_string()))
            })
    }

    /// 撤销临时角色（逻辑撤销 status=0）。
    ///
    /// 将指定授权记录的状态置为已撤销，并记录撤销人和撤销时间。
    /// 撤销后会失效对应用户的权限缓存。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务上下文，包含认证信息用于审计。
    /// * `assignment_id` - 待撤销的授权记录 ID。
    /// * `reason` - 撤销原因（可选）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * 当授权记录不存在或已撤销时返回业务错误。
    /// * 当数据库更新或查询失败时返回内部错误。
    async fn revoke_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        reason: Option<&str>,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::revoke_temp_role - assignment: {}",
            "IAM", assignment_id
        );

        let operator = svr_ctx
            .auth_context
            .as_ref()
            .map(|ctx| ctx.user_id.clone())
            .unwrap_or_default();

        let update_sql = r#"
            UPDATE cmx_user_role_assignment
            SET status = 0, revoked_by = $2, revoked_at = NOW(), update_time = NOW()
            WHERE id = $1 AND status = 1 AND archived = 0
        "#;
        let params = Value::Array(vec![
            Value::String(assignment_id.to_string()),
            Value::String(operator),
        ]);
        let affected = self
            .mm
            .execute_sql_with_json(&self.db_id, None, update_sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("撤销临时角色失败: {e}")))
            })?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "临时授权记录不存在或已撤销: {assignment_id}"
            ))));
        }

        // 查询 assignment 对应的 user_id（用于审计 target_id 和缓存失效）
        let user_id = {
            let query_sql = "SELECT user_id FROM cmx_user_role_assignment WHERE id = $1";
            let params = Value::Array(vec![Value::String(assignment_id.to_string())]);
            let dataset = self
                .mm
                .query_sql_with_json(&self.db_id, None, query_sql, params, "revoke_get_user")
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("查询用户ID失败: {e}"))))?;
            let schema = dataset.schema.as_ref();
            dataset
                .iter()
                .next()
                .and_then(|row| row.get_by_name_as::<String>(schema, "user_id"))
                .unwrap_or_default()
        };

        // 审计日志（target_id 为 user_id）
        let audit_detail = serde_json::json!({
            "assignment_id": assignment_id,
            "reason": reason,
        });
        self.audit_write(svr_ctx, "revoke_temp_role", "user", &user_id, &audit_detail)
            .await;

        // 失效用户缓存
        if let Some(checker) = &self.permission_checker {
            checker.invalidate_user_cache(&user_id).await;
        }

        Ok(())
    }

    /// 批量撤销临时角色。
    ///
    /// 在单个事务中撤销多个授权记录，并聚合审计日志。
    /// 撤销后会失效所有受影响用户的权限缓存。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务上下文，包含认证信息用于审计。
    /// * `assignment_ids` - 待撤销的授权记录 ID 列表。
    /// * `reason` - 撤销原因（可选）。
    ///
    /// # Returns
    ///
    /// 成功时返回实际撤销的记录数。
    ///
    /// # Errors
    ///
    /// 当事务开启、提交或数据库更新失败时返回内部错误。
    async fn revoke_temp_roles_batch(
        &self,
        svr_ctx: &SVRContext,
        assignment_ids: &[String],
        reason: Option<&str>,
    ) -> Result<u64, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::revoke_temp_roles_batch - count: {}",
            "IAM",
            assignment_ids.len()
        );

        let operator = svr_ctx
            .auth_context
            .as_ref()
            .map(|ctx| ctx.user_id.clone())
            .unwrap_or_default();

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        let mut total_affected: u64 = 0;
        let mut affected_user_ids: Vec<String> = Vec::new();
        for assignment_id in assignment_ids {
            // 先查询 user_id（用于缓存失效）
            let query_sql = "SELECT user_id FROM cmx_user_role_assignment WHERE id = $1";
            let params = Value::Array(vec![Value::String(assignment_id.clone())]);
            if let Ok(dataset) = self
                .mm
                .query_sql_with_json(&self.db_id, Some(txn_id), query_sql, params, "batch_revoke_get_user")
                .await
            {
                let schema = dataset.schema.as_ref();
                if let Some(uid) = dataset
                    .iter()
                    .next()
                    .and_then(|row| row.get_by_name_as::<String>(schema, "user_id"))
                    && !affected_user_ids.contains(&uid) {
                        affected_user_ids.push(uid);
                    }
            }

            let update_sql = r#"
                UPDATE cmx_user_role_assignment
                SET status = 0, revoked_by = $2, revoked_at = NOW(), update_time = NOW()
                WHERE id = $1 AND status = 1 AND archived = 0
            "#;
            let params = Value::Array(vec![
                Value::String(assignment_id.clone()),
                Value::String(operator.clone()),
            ]);
            let affected = self
                .mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), update_sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("批量撤销临时角色失败: {e}")))
                })?;
            total_affected += affected as u64;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 审计日志（批量聚合）
        let audit_detail = serde_json::json!({
            "assignment_ids": assignment_ids,
            "user_ids": affected_user_ids,
            "count": assignment_ids.len(),
            "affected": total_affected,
            "reason": reason,
        });
        self.audit_write(
            svr_ctx,
            "revoke_temp_roles_batch",
            "user",
            "batch",
            &audit_detail,
        )
        .await;

        // 失效受影响用户的缓存
        if let Some(checker) = &self.permission_checker {
            for uid in &affected_user_ids {
                checker.invalidate_user_cache(uid).await;
            }
        }

        Ok(total_affected)
    }

    /// 延长临时授权有效期。
    ///
    /// 将指定授权记录的 `effective_until` 更新为新的失效时间。
    /// 新失效时间必须晚于原失效时间。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务上下文，包含认证信息用于审计。
    /// * `assignment_id` - 待延长的授权记录 ID。
    /// * `new_effective_until` - 新的失效时间，必须晚于原 `effective_until`。
    /// * `reason` - 延长原因（可选）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * 当授权记录不存在或已撤销时返回业务错误。
    /// * 当新失效时间不晚于原失效时间时返回业务错误。
    /// * 当数据库查询或更新失败时返回内部错误。
    async fn extend_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        new_effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::extend_temp_role - assignment: {}, new_until: {}",
            "IAM", assignment_id, new_effective_until
        );

        // 先查询原记录，校验状态和原 effective_until，同时获取 user_id
        let query_sql = r#"
            SELECT effective_until, user_id FROM cmx_user_role_assignment
            WHERE id = $1 AND status = 1 AND archived = 0
        "#;
        let params = Value::Array(vec![Value::String(assignment_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, query_sql, params, "query_assignment")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询临时授权失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let row = dataset.iter().next().ok_or_else(|| {
            TraitError::from(IamError::Business(format!(
                "临时授权记录不存在或已撤销: {assignment_id}"
            )))
        })?;
        let old_until = row
            .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "effective_until")
            .ok_or_else(|| {
                TraitError::from(IamError::Business(format!(
                    "临时授权记录不存在或已撤销: {assignment_id}"
                )))
            })?;
        let user_id = row
            .get_by_name_as::<String>(schema, "user_id")
            .unwrap_or_default();

        if new_effective_until <= old_until {
            return Err(TraitError::from(IamError::Business(
                "新有效期必须晚于原有效期".to_string(),
            )));
        }

        let update_sql = r#"
            UPDATE cmx_user_role_assignment
            SET effective_until = $2, update_time = NOW()
            WHERE id = $1
        "#;
        let params = Value::Array(vec![
            Value::String(assignment_id.to_string()),
            Value::String(new_effective_until.to_rfc3339()),
        ]);
        self.mm
            .execute_sql_with_json(&self.db_id, None, update_sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("延长临时授权失败: {e}")))
            })?;

        // 审计日志（target_id 为 user_id）
        let audit_detail = serde_json::json!({
            "assignment_id": assignment_id,
            "old_effective_until": old_until.to_rfc3339(),
            "new_effective_until": new_effective_until.to_rfc3339(),
            "reason": reason,
        });
        self.audit_write(svr_ctx, "extend_temp_role", "user", &user_id, &audit_detail)
            .await;

        // 失效用户缓存
        if let Some(checker) = &self.permission_checker {
            checker.invalidate_user_cache(&user_id).await;
        }

        Ok(())
    }

    /// 查询用户的临时授权列表。
    ///
    /// 根据状态过滤条件返回指定用户的临时角色授权记录。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    /// * `status_filter` - 状态过滤条件，参见 `TempAssignmentStatusFilter`。
    ///
    /// # Returns
    ///
    /// 成功时返回 `UserRoleAssignment` 列表，按授权记录顺序排列。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回内部错误。
    async fn get_user_temp_assignments(
        &self,
        user_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user_temp_assignments - user: {}, filter: {:?}",
            "IAM", user_id, status_filter
        );

        let mut where_clause = String::from(
            "a.user_id = $1 AND a.archived = 0 AND r.archived = 0",
        );
        match status_filter {
            TempAssignmentStatusFilter::All => {}
            TempAssignmentStatusFilter::Active => {
                where_clause.push_str(" AND a.status = 1 AND NOW() BETWEEN a.effective_from AND a.effective_until");
            }
            TempAssignmentStatusFilter::Expired => {
                where_clause.push_str(" AND a.status = 1 AND a.effective_until < NOW()");
            }
            TempAssignmentStatusFilter::Revoked => {
                where_clause.push_str(" AND a.status = 0");
            }
        }

        let sql = format!(
            r#"
            SELECT a.id, a.user_id, a.role_id, a.effective_from, a.effective_until,
                   a.reason, a.source, a.status, a.revoked_by, a.revoked_at, a.create_time,
                   r.code, r.name
            FROM cmx_user_role_assignment a
            INNER JOIN cmx_role r ON r.id = a.role_id
            WHERE {where_clause}
            ORDER BY a.create_time DESC
            "#
        );
        let params = Value::Array(vec![Value::String(user_id.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, &sql, params, "user_temp_assignments")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询用户临时授权失败: {e}")))
            })?;

        Ok(Self::extract_assignments(dataset))
    }

    /// 查询角色被授权的用户列表（临时授权）
    async fn get_role_temp_assigned_users(
        &self,
        role_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_role_temp_assigned_users - role: {}, filter: {:?}",
            "IAM", role_id, status_filter
        );

        let mut where_clause = String::from(
            "a.role_id = $1 AND a.archived = 0 AND r.archived = 0",
        );
        match status_filter {
            TempAssignmentStatusFilter::All => {}
            TempAssignmentStatusFilter::Active => {
                where_clause.push_str(" AND a.status = 1 AND NOW() BETWEEN a.effective_from AND a.effective_until");
            }
            TempAssignmentStatusFilter::Expired => {
                where_clause.push_str(" AND a.status = 1 AND a.effective_until < NOW()");
            }
            TempAssignmentStatusFilter::Revoked => {
                where_clause.push_str(" AND a.status = 0");
            }
        }

        let sql = format!(
            r#"
            SELECT a.id, a.user_id, a.role_id, a.effective_from, a.effective_until,
                   a.reason, a.source, a.status, a.revoked_by, a.revoked_at, a.create_time,
                   r.code, r.name
            FROM cmx_user_role_assignment a
            INNER JOIN cmx_role r ON r.id = a.role_id
            WHERE {where_clause}
            ORDER BY a.create_time DESC
            "#
        );
        let params = Value::Array(vec![Value::String(role_id.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, &sql, params, "role_temp_users")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询角色临时用户失败: {e}")))
            })?;

        Ok(Self::extract_assignments(dataset))
    }

    /// 查询用户有效权限（合并永久 + 临时授权）
    async fn get_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<EffectivePermissionsResponse, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_effective_permissions - user: {}",
            "IAM", user_id
        );

        // 1. 查询用户基本信息
        let user_sql = "SELECT id, username FROM cmx_user WHERE id = $1 AND archived = 0";
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, user_sql, params, "user_info")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let (uid, username) = dataset
            .iter()
            .next()
            .and_then(|row| {
                Some((
                    row.get_by_name_as::<String>(schema, "id")?,
                    row.get_by_name_as::<String>(schema, "username")?,
                ))
            })
            .ok_or_else(|| TraitError::from(IamError::UserNotFound(user_id.to_string())))?;

        // 2. 查询有效角色（永久 + 临时）
        let roles_sql = r#"
            SELECT r.id, r.code, r.name, r.description FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
            UNION
            SELECT r.id, r.code, r.name, r.description FROM cmx_role r
            INNER JOIN cmx_user_role_assignment ura ON r.id = ura.role_id
            WHERE ura.user_id = $1 AND ura.status = 1 AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND r.archived = 0 AND r.status = 1
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, roles_sql, params, "effective_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询有效角色失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let roles: Vec<RoleSummary> = dataset
            .iter()
            .filter_map(|row| {
                Some(RoleSummary {
                    id: row.get_by_name_as(schema, "id")?,
                    code: row.get_by_name_as(schema, "code")?,
                    name: row.get_by_name_as(schema, "name")?,
                    description: row.get_by_name_as(schema, "description"),
                })
            })
            .collect();

        // 3. 查询有效权限
        let perms_sql = r#"
            SELECT DISTINCT p.id, p.code, p.name, p.resource_type, p.description FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ur.role_id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1 AND r.archived = 0 AND r.status = 1
            UNION
            SELECT DISTINCT p.id, p.code, p.name, p.resource_type, p.description FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role_assignment ura ON ura.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ura.role_id
            WHERE ura.user_id = $1 AND ura.status = 1 AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND rp.archived = 0 AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, perms_sql, params, "effective_perms")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询有效权限失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let permissions: Vec<PermissionSummary> = dataset
            .iter()
            .filter_map(|row| {
                Some(PermissionSummary {
                    id: row.get_by_name_as(schema, "id")?,
                    code: row.get_by_name_as(schema, "code")?,
                    name: row.get_by_name_as(schema, "name")?,
                    resource_type: row.get_by_name_as(schema, "resource_type"),
                    description: row.get_by_name_as(schema, "description"),
                })
            })
            .collect();

        // 4. 统计临时角色
        let active_count_sql = r#"
            SELECT COUNT(*) as cnt FROM cmx_user_role_assignment
            WHERE user_id = $1 AND status = 1 AND archived = 0
              AND NOW() BETWEEN effective_from AND effective_until
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, active_count_sql, params, "active_count")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("统计临时角色失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let active_temp_roles = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<i64>(schema, "cnt"))
            .unwrap_or(0) as u32;

        let expired_count_sql = r#"
            SELECT COUNT(*) as cnt FROM cmx_user_role_assignment
            WHERE user_id = $1 AND status = 1 AND archived = 0
              AND effective_until < NOW()
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, expired_count_sql, params, "expired_count")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("统计过期角色失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let expired_temp_roles = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<i64>(schema, "cnt"))
            .unwrap_or(0) as u32;

        // 6. 统计7天内将过期的临时角色
        let upcoming_sql = r#"
            SELECT COUNT(*) as cnt FROM cmx_user_role_assignment
            WHERE user_id = $1 AND status = 1 AND archived = 0
              AND NOW() BETWEEN effective_from AND effective_until
              AND effective_until < NOW() + INTERVAL '7 days'
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, upcoming_sql, params, "upcoming_expirations")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("统计即将过期角色失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let upcoming_expirations = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<i64>(schema, "cnt"))
            .unwrap_or(0) as u32;

        Ok(EffectivePermissionsResponse {
            user_id: uid,
            username,
            roles,
            permissions,
            active_temp_roles,
            expired_temp_roles,
            upcoming_expirations,
        })
    }
}
