//! UserAuthQuery trait 实现
//!
//! 实现 cmx_traits::auth::UserAuthQuery trait，提供用户认证数据查询。
//! 通过参数化 SQL 查询获取用户数据，使用事务保证超管创建和 OAuth2 注册的原子性。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_traits::auth::{OAuth2UserInfo, UserAuthData, UserAuthQuery};
use cmx_traits::error::TraitError;
use tracing::{debug, info};

use crate::config::IamConfig;

/// `UserAuthQuery` 实现。
///
/// 持有 `Arc<DatabaseManager>` 与 `db_id`，通过参数化查询避免 SQL 注入，
/// 供 `cmx-auth` 在认证流程中查询用户/角色/权限信息。
pub struct UserAuthQueryImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证库 `db_id`（来自配置或 `DatabaseManager` 默认值）。
    db_id: String,
}

impl UserAuthQueryImpl {
    /// 从 DataSet 第一行提取 UserAuthData
    fn extract_user(dataset: DataSet) -> Option<UserAuthData> {
        let schema = dataset.schema.as_ref();
        let row = dataset.iter().next()?;

        Some(UserAuthData {
            user_id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            username: row.get_by_name_as(schema, "username").unwrap_or_default(),
            password_hash: row.get_by_name_as(schema, "password_hash"),
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
            nickname: row.get_by_name_as(schema, "nickname"),
            email: row.get_by_name_as(schema, "email"),
        })
    }

    /// 构造函数
    ///
    /// 使用 config.auth_db_id 或回退到 DatabaseManager 默认 db_id
    pub async fn new(mm: Arc<DatabaseManager>, config: &IamConfig) -> Result<Self, TraitError> {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Ok(Self { mm, db_id })
    }
}

#[async_trait]
impl UserAuthQuery for UserAuthQueryImpl {
    async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAuthData>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_by_username - {}",
            "IAM", username
        );

        let sql = "SELECT id, username, password_hash, status, nickname, email \
                   FROM cmx_user WHERE username = $1 AND archived = 0";
        let params: Vec<DataValue> = vec![DataValue::String(username.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_by_username")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户失败: {}", e)))?;

        Ok(Self::extract_user(dataset))
    }

    async fn get_user_by_id(&self, user_id: &str) -> Result<Option<UserAuthData>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_by_id - {}",
            "IAM", user_id
        );

        let sql = "SELECT id, username, password_hash, status, nickname, email \
                   FROM cmx_user WHERE id = $1 AND archived = 0";
        let params: Vec<DataValue> = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_by_id")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户失败: {}", e)))?;

        Ok(Self::extract_user(dataset))
    }

    async fn get_user_role_codes(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_role_codes - {}",
            "IAM", user_id
        );

        // 合并查询永久角色（cmx_user_role）与临时有效角色（cmx_user_role_assignment）
        let sql = r#"
            SELECT DISTINCT r.code
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1

            UNION

            SELECT DISTINCT r.code
            FROM cmx_role r
            INNER JOIN cmx_user_role_assignment ura ON r.id = ura.role_id
            WHERE ura.user_id = $1
              AND ura.status = 1
              AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND r.archived = 0 AND r.status = 1
        "#;
        let params: Vec<DataValue> = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_role_codes")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户角色失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(roles)
    }

    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_permissions - {}",
            "IAM", user_id
        );

        // 合并查询永久角色权限与临时角色权限
        let sql = r#"
            SELECT DISTINCT p.code
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ur.role_id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1

            UNION

            SELECT DISTINCT p.code
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role_assignment ura ON ura.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ura.role_id
            WHERE ura.user_id = $1
              AND ura.status = 1
              AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1
        "#;
        let params: Vec<DataValue> = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_permissions")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户权限失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let permissions: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(permissions)
    }

    async fn update_password_hash(
        &self,
        user_id: &str,
        new_hash: &str,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::update_password_hash - user_id: {}",
            "IAM", user_id
        );

        let sql = "UPDATE cmx_user SET password_hash = $1, update_time = NOW() WHERE id = $2";
        let params: Vec<DataValue> = vec![
            DataValue::String(new_hash.to_string()),
            DataValue::String(user_id.to_string()),
        ];

        self.mm
            .execute_sql_with_datavalues(&self.db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("更新密码哈希失败: {}", e)))?;

        Ok(())
    }

    async fn update_last_login(&self, user_id: &str, ip: &str) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::update_last_login - user_id: {}",
            "IAM", user_id
        );

        let sql = "UPDATE cmx_user SET last_login_at = NOW(), last_login_ip = $2, update_time = NOW() WHERE id = $1";
        let params: Vec<DataValue> = vec![
            DataValue::String(user_id.to_string()),
            DataValue::String(ip.to_string()),
        ];

        self.mm
            .execute_sql_with_datavalues(&self.db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("更新最后登录信息失败: {}", e)))?;

        Ok(())
    }

    async fn create_super_admin(
        &self,
        username: &str,
        password_hash: &str,
        email: Option<&str>,
        roles: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::create_super_admin - username: {}",
            "IAM", username
        );

        // 开始事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::Internal(format!("开启事务失败: {}", e)))?;
        let txn_id = guard.txn_id();

        // 1. 创建用户
        let user_id = cmx_utils::snowflake_id_str();
        let insert_user_sql = "INSERT INTO cmx_user (id, username, password_hash, nickname, email, status, archived) \
                               VALUES ($1, $2, $3, $4, $5, 1, 0)";
        let email_val = email
            .map(|e| DataValue::String(e.to_string()))
            .unwrap_or(DataValue::Null);
        let params: Vec<DataValue> = vec![
            DataValue::String(user_id.clone()),
            DataValue::String(username.to_string()),
            DataValue::String(password_hash.to_string()),
            DataValue::String("Super Admin".to_string()),
            email_val,
        ];

        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_user_sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("创建超管用户失败: {}", e)))?;

        // 2. 关联角色
        for role_code in roles {
            let role_sql =
                "SELECT id FROM cmx_role WHERE code = $1 AND archived = 0 AND status = 1";
            let role_params: Vec<DataValue> = vec![DataValue::String(role_code.clone())];

            let role_dataset = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    role_sql,
                    role_params,
                    "role_ids",
                )
                .await
                .map_err(|e| TraitError::Internal(format!("查询角色失败: {}", e)))?;

            let role_schema = role_dataset.schema.as_ref();
            for row in role_dataset.iter() {
                if let Some(role_id) = row.get_by_name_as::<String>(role_schema, "id") {
                    let ur_id = cmx_utils::snowflake_id_str();
                    let insert_ur_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived) \
                                         VALUES ($1, $2, $3, 0) ON CONFLICT (user_id, role_id) DO NOTHING";
                    let ur_params: Vec<DataValue> = vec![
                        DataValue::String(ur_id),
                        DataValue::String(user_id.clone()),
                        DataValue::String(role_id),
                    ];

                    self.mm
                        .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_ur_sql, ur_params)
                        .await
                        .map_err(|e| TraitError::Internal(format!("关联超管角色失败: {}", e)))?;
                }
            }
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::Internal(format!("提交事务失败: {}", e)))?;

        info!(username = username, "超管账号创建成功");
        Ok(())
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<UserAuthData>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_by_email - {}",
            "IAM", email
        );

        let sql = "SELECT id, username, password_hash, status, nickname, email \
                   FROM cmx_user WHERE email = $1 AND archived = 0";
        let params: Vec<DataValue> = vec![DataValue::String(email.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_by_email")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户失败: {}", e)))?;

        Ok(Self::extract_user(dataset))
    }

    async fn create_user_from_oauth2(
        &self,
        provider: &str,
        user_info: &OAuth2UserInfo,
    ) -> Result<String, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::create_user_from_oauth2 - provider: {}",
            "IAM", provider
        );

        // 开始事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::Internal(format!("开启事务失败: {}", e)))?;
        let txn_id = guard.txn_id();

        // 1. 创建用户（OAuth2 用户无密码）
        let user_id = cmx_utils::snowflake_id_str();
        let username = user_info.username.clone().unwrap_or_else(|| {
            format!(
                "{}_{}",
                provider,
                &user_info.provider_user_id[..8.min(user_info.provider_user_id.len())]
            )
        });
        let insert_user_sql = "INSERT INTO cmx_user (id, username, password_hash, nickname, email, status, archived) \
                               VALUES ($1, $2, $3, $4, $5, 1, 0)";
        let nickname_val = user_info
            .display_name
            .clone()
            .map(DataValue::String)
            .unwrap_or(DataValue::Null);
        let email_val = user_info
            .email
            .clone()
            .map(DataValue::String)
            .unwrap_or(DataValue::Null);
        let params: Vec<DataValue> = vec![
            DataValue::String(user_id.clone()),
            DataValue::String(username),
            DataValue::Null,
            nickname_val,
            email_val,
        ];

        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_user_sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("OAuth2 自动注册用户失败: {}", e)))?;

        // 2. 关联默认角色
        if let Some(ref role_code) = user_info.default_role {
            let role_sql =
                "SELECT id FROM cmx_role WHERE code = $1 AND archived = 0 AND status = 1";
            let role_params: Vec<DataValue> = vec![DataValue::String(role_code.clone())];

            let role_dataset = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    role_sql,
                    role_params,
                    "role_by_code",
                )
                .await
                .map_err(|e| TraitError::Internal(format!("查询角色失败: {}", e)))?;

            let role_schema = role_dataset.schema.as_ref();
            if let Some(role_row) = role_dataset.iter().next() {
                if let Some(role_id) = role_row.get_by_name_as::<String>(role_schema, "id") {
                    let ur_id = cmx_utils::snowflake_id_str();
                    let insert_ur_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived) \
                                         VALUES ($1, $2, $3, 0) ON CONFLICT (user_id, role_id) DO NOTHING";
                    let ur_params: Vec<DataValue> = vec![
                        DataValue::String(ur_id),
                        DataValue::String(user_id.clone()),
                        DataValue::String(role_id),
                    ];

                    self.mm
                        .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_ur_sql, ur_params)
                        .await
                        .map_err(|e| TraitError::Internal(format!("关联默认角色失败: {}", e)))?;
                    info!(user_id = %user_id, role_code = %role_code, "OAuth2 自动注册用户已关联默认角色");
                }
            }
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::Internal(format!("提交事务失败: {}", e)))?;

        info!(provider = provider, user_id = %user_id, "OAuth2 自动注册用户成功");
        Ok(user_id)
    }
}
