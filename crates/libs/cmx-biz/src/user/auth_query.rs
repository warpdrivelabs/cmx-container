//! UserAuthQuery trait 实现
//!
//! 实现 cmx_traits::auth::UserAuthQuery trait，提供用户认证数据查询。
//! 通过自定义 SQL JOIN 查询获取用户的角色编码和权限编码。

use async_trait::async_trait;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::auth::{OAuth2UserInfo, UserAuthData, UserAuthQuery};
use cmx_traits::error::TraitError;
use modql::filter::{ListOptions, OpValString, OpValsString};
use tracing::{debug, info};
use uuid::Uuid;

use super::{UserBmc, UserFilter, UserForCreate, UserForUpdate};

/// UserAuthQuery 实现
pub struct UserAuthQueryImpl;

impl UserAuthQueryImpl {
    /// 从 DataSet 第一行提取 UserAuthData
    fn extract_user(dataset: DataSet) -> Option<UserAuthData> {
        let schema = dataset.schema.as_ref();
        let row = dataset.iter().next()?;

        Some(UserAuthData {
            user_id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            username: row.get_by_name_as(schema, "username").unwrap_or_default(),
            password_hash: row.get_by_name_as(schema, "password_hash"),
            status: row
                .get_by_name_as::<i64>(schema, "status")
                .unwrap_or(1),
            nickname: row.get_by_name_as(schema, "nickname"),
            email: row.get_by_name_as(schema, "email"),
        })
    }

    /// 获取数据库管理器（使用默认实例）
    fn get_db_manager() -> &'static DatabaseManager {
        cmx_database::get_default_db_manager()
    }

    /// 默认 db_id（从 DatabaseManager 动态获取，不写死）
    async fn default_db_id() -> String {
        Self::get_db_manager().get_default_db_id().await
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
            "AUTH", username
        );

        let filters = vec![UserFilter {
            username: Some(OpValsString(vec![OpValString::Eq(username.to_string())])),
            nickname: None,
            status: None,
            archived: None,
        }];

        let list_options = ListOptions {
            limit: Some(1),
            offset: None,
            order_bys: None,
        };

        let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            Some(filters),
            Some(list_options),
        )
        .await
        .map_err(|e| TraitError::Internal(format!("查询用户失败: {}", e)))?;

        Ok(Self::extract_user(dataset))
    }

    async fn get_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<UserAuthData>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_by_id - {}",
            "AUTH", user_id
        );

        let dataset = GenericCrudService::<UserBmc>::get(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            user_id.into(),
        )
        .await
        .map_err(|e| TraitError::Internal(format!("查询用户失败: {}", e)))?;

        Ok(Self::extract_user(dataset))
    }

    async fn get_user_role_codes(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_role_codes - {}",
            "AUTH", user_id
        );

        let sql = r#"
            SELECT r.code 
            FROM cmx_role r 
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id 
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
        "#;

        let dataset = Self::get_db_manager()
            .query_sql(&Self::default_db_id().await,
                       None, sql, "user_role_codes")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户角色失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(roles)
    }

    async fn get_user_permissions(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_permissions - {}",
            "AUTH", user_id
        );

        let sql = r#"
            SELECT DISTINCT p.code 
            FROM cmx_permission p 
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id 
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id 
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0 AND p.archived = 0 AND p.status = 1
        "#;

        let dataset = Self::get_db_manager()
            .query_sql(&Self::default_db_id().await, None, sql, "user_permissions")
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
            "AUTH", user_id
        );

        let data = UserForUpdate {
            password_hash: Some(new_hash.to_string()),
            nickname: None,
            email: None,
            phone: None,
            status: None,
        };

        GenericCrudService::<UserBmc>::update(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            user_id.into(),
            data,
        )
        .await
        .map_err(|e| TraitError::Internal(format!("更新密码哈希失败: {}", e)))?;

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
            "AUTH", username
        );

        // 1. 创建用户
        let user_id = Uuid::new_v4().to_string();
        let data = UserForCreate {
            username: username.to_string(),
            password_hash: Some(password_hash.to_string()),
            nickname: Some("Super Admin".to_string()),
            email: email.map(|e| e.to_string()),
            phone: None,
            org_id: None,
            status: Some(1),
        };

        GenericCrudService::<UserBmc>::create(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            data,
        )
        .await
        .map_err(|e| TraitError::Internal(format!("创建超管用户失败: {}", e)))?;

        // 2. 查询刚创建的用户以获取其 ID
        let filters = vec![UserFilter {
            username: Some(OpValsString(vec![OpValString::Eq(username.to_string())])),
            nickname: None,
            status: None,
            archived: None,
        }];
        let list_options = ListOptions {
            limit: Some(1),
            offset: None,
            order_bys: None,
        };
        let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            Some(filters),
            Some(list_options),
        )
        .await
        .map_err(|e| TraitError::Internal(format!("查询超管用户失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let created_user_id: Option<String> = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as(schema, "id"));

        let user_id = created_user_id.unwrap_or(user_id);

        // 3. 关联角色：通过角色编码查找角色 ID，插入 cmx_user_role
        if !roles.is_empty() {
            let role_codes: Vec<String> = roles.iter().map(|r| format!("'{}'", r.replace('\'', "''"))).collect();
            let role_codes_str = role_codes.join(",");
            let sql = format!(
                "SELECT id FROM cmx_role WHERE code IN ({}) AND archived = 0 AND status = 1",
                role_codes_str
            );

            let role_dataset = Self::get_db_manager()
                .query_sql(&Self::default_db_id().await, None, &sql, "role_ids")
                .await
                .map_err(|e| TraitError::Internal(format!("查询角色失败: {}", e)))?;

            let role_schema = role_dataset.schema.as_ref();
            for row in role_dataset.iter() {
                if let Some(role_id) = row.get_by_name_as::<String>(role_schema, "id") {
                    let ur_id = Uuid::new_v4().to_string();
                    let insert_sql = format!(
                        "INSERT INTO cmx_user_role (id, user_id, role_id, archived, status) VALUES ('{}', '{}', '{}', 0, 1) ON CONFLICT (id) DO NOTHING",
                        ur_id, user_id, role_id
                    );
                    Self::get_db_manager()
                        .execute_sql(&Self::default_db_id().await,
                                                 None, &insert_sql)
                        .await
                        .map_err(|e| TraitError::Internal(format!("关联超管角色失败: {}", e)))?;
                }
            }
        }

        info!(username = username, "超管账号创建成功");
        Ok(())
    }

    async fn update_last_login(
        &self,
        user_id: &str,
        ip: &str,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::update_last_login - user: {}",
            "AUTH", user_id
        );

        let sql = "UPDATE cmx_user SET last_login_at = NOW(), last_login_ip = $1, update_time = NOW() WHERE id = $2";
        let params = serde_json::Value::Array(vec![
            serde_json::Value::String(ip.to_string()),
            serde_json::Value::String(user_id.to_string()),
        ]);

        Self::get_db_manager()
            .execute_sql_with_json(&Self::default_db_id().await, None, sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("更新登录时间失败: {}", e)))?;

        Ok(())
    }

    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<UserAuthData>, TraitError> {
        debug!(
            "{:<12} - UserAuthQueryImpl::get_user_by_email - {}",
            "AUTH", email
        );

        let _filters = vec![UserFilter {
            username: None,
            nickname: None,
            status: None,
            archived: None,
        }];

        let sql = format!(
            "SELECT id, username, password_hash, status, nickname, email \
             FROM cmx_user WHERE email = '{}' AND archived = 0",
            email.replace('\'', "''")
        );

        let dataset = Self::get_db_manager()
            .query_sql(&Self::default_db_id().await, None, &sql, "user_by_email")
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
            "AUTH", provider
        );

        let user_id = Uuid::new_v4().to_string();
        let data = UserForCreate {
            username: user_info.username.clone().unwrap_or_else(|| {
                format!("{}_{}", provider, &user_info.provider_user_id[..8.min(user_info.provider_user_id.len())])
            }),
            password_hash: None,
            nickname: user_info.display_name.clone(),
            email: user_info.email.clone(),
            phone: None,
            org_id: None,
            status: Some(1),
        };

        let result = GenericCrudService::<UserBmc>::create(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            data,
        )
        .await
        .map_err(|e| TraitError::Internal(format!("OAuth2 自动注册用户失败: {}", e)))?;

        // 从 create 返回值中获取生成的 ID，避免二次查询
        let schema = result.schema.as_ref();
        let created_user_id = result.iter()
            .next()
            .and_then(|row| row.get_by_name_as::<String>(schema, "id"))
            .unwrap_or(user_id);
        let user_id = created_user_id;

        // 关联默认角色
        if let Some(ref role_code) = user_info.default_role {
            // N-7 修复：使用参数化查询替代 SQL 拼接，防止注入风险
            let role_sql = "SELECT id FROM cmx_role WHERE code = $1 AND archived = 0 AND status = 1";
            let role_dataset = Self::get_db_manager()
                .query_sql_with_json(
                    &Self::default_db_id().await,
                    None,
                    role_sql,
                    serde_json::Value::Array(vec![serde_json::Value::String(role_code.clone())]),
                    "role_by_code",
                )
                .await
                .map_err(|e| TraitError::Internal(format!("查询角色失败: {}", e)))?;

            let role_schema = role_dataset.schema.as_ref();
            if let Some(role_row) = role_dataset.iter().next() {
                if let Some(role_id) = role_row.get_by_name_as::<String>(role_schema, "id") {
                    let ur_id = Uuid::new_v4().to_string();
                    // N-7 修复：参数化插入，ON CONFLICT (user_id, role_id) 正确处理重复关联
                    let insert_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived, status) VALUES ($1, $2, $3, 0, 1) ON CONFLICT (user_id, role_id) DO NOTHING";
                    Self::get_db_manager()
                        .execute_sql_with_json(
                            &Self::default_db_id().await,
                            None,
                            insert_sql,
                            serde_json::Value::Array(vec![
                                serde_json::Value::String(ur_id),
                                serde_json::Value::String(user_id.clone()),
                                serde_json::Value::String(role_id),
                            ]),
                        )
                        .await
                        .map_err(|e| TraitError::Internal(format!("关联默认角色失败: {}", e)))?;
                    info!(user_id = %user_id, role_code = %role_code, "OAuth2 自动注册用户已关联默认角色");
                }
            }
        }

        info!(provider = provider, user_id = %user_id, "OAuth2 自动注册用户成功");
        Ok(user_id)
    }
}
