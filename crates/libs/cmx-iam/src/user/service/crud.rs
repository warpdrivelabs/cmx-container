//! 用户 CRUD
//!
//! 实现 [`crate::service_traits::UserService`] 的创建/查询/更新/删除方法。
//! 方法体集中在本固有方法中，trait 实现在 `mod.rs` 中逐方法委托。

use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_database::crud::GenericCrudService;
use cmx_traits::error::TraitError;
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::error::IamError;
use crate::user::service::UserServiceImpl;
use crate::user::{UserBmc, UserForCreate, UserForInsert, UserForUpdate, UserForUpdateInsert};

impl UserServiceImpl {
    /// 创建用户（[`crate::service_traits::UserService::create_user`] 的实现）。
    ///
    /// 校验用户名唯一性、密码长度，调用 `AuthService` 哈希密码后写入数据库，并写审计日志。
    pub(super) async fn create_user(
        &self,
        svr_ctx: &SVRContext,
        data: UserForCreate,
    ) -> Result<cmx_core::model::iam::User, TraitError> {
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
        let check_params = vec![DataValue::String(data.username.clone())];
        let existing = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, check_sql, check_params, "check_username")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户名失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::UsernameExists(
                data.username.clone(),
            )));
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

    /// 获取单个用户（按 username 查询，[`crate::service_traits::UserService::get_user`] 的实现）。
    pub(super) async fn get_user(
        &self,
        username: &str,
    ) -> Result<cmx_core::model::iam::User, TraitError> {
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
        let params = vec![DataValue::String(username.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "get_user_by_username")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户失败: {e}"))))?;

        if dataset.iter().next().is_none() {
            return Err(TraitError::from(IamError::UserNotFound(
                username.to_string(),
            )));
        }

        Self::extract_user(dataset).map_err(TraitError::from)
    }

    /// 更新用户（[`crate::service_traits::UserService::update_user`] 的实现）。
    ///
    /// 支持可选密码更新（提供时触发哈希 + 写入），并写审计日志。
    pub(super) async fn update_user(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        data: UserForUpdate,
    ) -> Result<cmx_core::model::iam::User, TraitError> {
        debug!("{:<12} - UserServiceImpl::update_user - {}", "IAM", user_id);

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

    /// 批量删除用户（[`crate::service_traits::UserService::delete_user`] 的实现）。
    ///
    /// 事务保证软删除 `cmx_user`（archived=1）+ 物理删除 `cmx_user_role` 与
    /// `cmx_user_role_assignment` 关联的原子性。
    pub(super) async fn delete_user(
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

        // 使用事务保证删除+物理删除的原子性
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 软删除 cmx_user（archived=1，保留记录供审计追溯，配合部分唯一索引允许同名重建）
        for user_id in user_ids {
            let sql = "UPDATE cmx_user SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = vec![DataValue::String(user_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("删除用户失败: {e}")))
                })?;
        }

        // 2. 物理删除 cmx_user_role 关联
        for user_id in user_ids {
            let sql = "DELETE FROM cmx_user_role WHERE user_id = $1";
            let params = vec![DataValue::String(user_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("删除用户角色关联失败: {e}")))
                })?;
        }

        // 3. 物理删除 cmx_user_role_assignment 临时授权关联
        for user_id in user_ids {
            let sql = "DELETE FROM cmx_user_role_assignment WHERE user_id = $1";
            let params = vec![DataValue::String(user_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("删除用户临时授权失败: {e}")))
                })?;
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
}
