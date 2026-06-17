//! 用户服务实现 — UserServiceImpl

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{Role, User};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::AuthService;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::service_traits::UserService;
use crate::user::{
    UserBmc, UserFilter, UserForCreate, UserForInsert, UserForUpdate, UserForUpdateInsert,
};

/// 用户服务实现
pub struct UserServiceImpl {
    /// 数据库管理器
    mm: Arc<DatabaseManager>,
    /// 认证服务（用于密码哈希）
    auth: Arc<dyn AuthService>,
    /// 认证库 db_id
    db_id: String,
    /// IAM 配置
    config: IamConfig,
    /// 审计日志记录器（可选）
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
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
        }
    }

    /// 设置审计日志记录器（Builder 模式）
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
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

        let user = Self::extract_user(dataset).map_err(|e| TraitError::from(e))?;

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

    async fn get_user(&self, user_id: &str) -> Result<User, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user - {}",
            "IAM", user_id
        );

        let dataset = GenericCrudService::<UserBmc>::get(
            &self.mm,
            &self.db_id,
            None,
            Value::String(user_id.to_string()),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::UserNotFound(user_id.to_string())));
        }

        Self::extract_user(dataset).map_err(|e| TraitError::from(e))
    }

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

        let user = Self::extract_user(dataset).map_err(|e| TraitError::from(e))?;

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

    async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::assign_roles - user: {}, role_count: {}",
            "IAM", user_id, role_ids.len()
        );

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 物理删除旧关联
        let delete_sql = "DELETE FROM cmx_user_role WHERE user_id = $1";
        let delete_params = Value::Array(vec![Value::String(user_id.to_string())]);
        self.mm
            .execute_sql_with_json(&self.db_id, Some(txn_id), delete_sql, delete_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除旧角色关联失败: {e}"))))?;

        // 2. 批量插入新关联
        for role_id in role_ids {
            let ur_id = uuid::Uuid::new_v4().to_string();
            let insert_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived, status) \
                              VALUES ($1, $2, $3, 0, 1) ON CONFLICT (user_id, role_id) DO NOTHING";
            let params = Value::Array(vec![
                Value::String(ur_id),
                Value::String(user_id.to_string()),
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
            "user_id": user_id,
            "role_ids": role_ids,
        });
        self.audit_write(svr_ctx, "assign_roles", "user", user_id, &audit_detail)
            .await;

        info!(user_id = user_id, role_count = role_ids.len(), "用户角色分配成功");
        Ok(())
    }

    async fn get_user_roles(&self, user_id: &str) -> Result<Vec<Role>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user_roles - user: {}",
            "IAM", user_id
        );

        let sql = r#"
            SELECT r.id, r.code, r.name, r.data_scope, r.sort_order, r.status, r.description,
                   r.archived, r.create_time, r.update_time,
                   r.create_by, r.create_name, r.update_by, r.update_name
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "user_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户角色失败: {e}"))))?;

        Ok(Self::extract_roles(dataset))
    }
}
