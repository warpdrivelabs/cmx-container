//! IAM 权限校验器 — IamChecker
//!
//! 实现 cmx_traits::iam::PermissionChecker trait，通过数据库 EXISTS 查询进行权限/角色校验。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::iam::{DataScope, PermissionChecker};
use serde_json::Value;
use tracing::debug;

use crate::config::IamConfig;

/// IAM 权限校验器实现。
///
/// 通过数据库 `EXISTS` 查询进行权限/角色校验，支持 `system:all` 超级权限短路。
pub struct IamChecker {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证库 `db_id`。
    db_id: String,

    /// IAM 配置（预留：未来用于本地缓存 TTL 等扩展）。
    #[allow(dead_code)]
    config: IamConfig,
}

impl IamChecker {
    /// 构造函数
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self { mm, db_id, config }
    }

    /// 执行 EXISTS 查询，返回布尔值
    async fn exists_check(&self, sql: &str, params: Value, label: &str) -> Result<bool, TraitError> {
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, label)
            .await
            .map_err(|e| TraitError::Internal(format!("权限查询失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let exists = dataset
            .iter()
            .next()
            .and_then(|row| {
                // EXISTS 查询返回单列单行 bool，取第一列
                row.get(0).and_then(|v| match v {
                    cmx_core::model::cell::DataValue::Bool(b) => Some(*b),
                    cmx_core::model::cell::DataValue::Int(i) => Some(*i != 0),
                    _ => None,
                })
            })
            .unwrap_or(false);

        let _ = schema; // 避免 schema 未使用告警
        Ok(exists)
    }
}

#[async_trait]
impl PermissionChecker for IamChecker {
    async fn has_permission(
        &self,
        user_id: &str,
        permission_code: &str,
    ) -> Result<bool, TraitError> {
        debug!(
            "{:<12} - IamChecker::has_permission - user: {}, code: {}",
            "IAM", user_id, permission_code
        );

        // 1. 先检查 system:all 超级权限（短路）
        let system_all_sql = r#"
            SELECT EXISTS(
              SELECT 1 FROM cmx_permission p
              INNER JOIN cmx_role_permission rp ON p.id = rp.permission_id
              INNER JOIN cmx_user_role ur ON rp.role_id = ur.role_id
              INNER JOIN cmx_role r ON r.id = ur.role_id
              WHERE ur.user_id = $1 AND p.code = 'system:all' AND p.status = 1
                AND ur.archived = 0 AND rp.archived = 0 AND p.archived = 0
                AND r.archived = 0 AND r.status = 1
            )
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        if self.exists_check(system_all_sql, params, "check_system_all").await? {
            return Ok(true);
        }

        // 2. 精确检查目标权限码
        let target_sql = r#"
            SELECT EXISTS(
              SELECT 1 FROM cmx_permission p
              INNER JOIN cmx_role_permission rp ON p.id = rp.permission_id
              INNER JOIN cmx_user_role ur ON rp.role_id = ur.role_id
              INNER JOIN cmx_role r ON r.id = ur.role_id
              WHERE ur.user_id = $1 AND p.code = $2 AND p.status = 1
                AND ur.archived = 0 AND rp.archived = 0 AND p.archived = 0
                AND r.archived = 0 AND r.status = 1
            )
        "#;
        let params = Value::Array(vec![
            Value::String(user_id.to_string()),
            Value::String(permission_code.to_string()),
        ]);
        self.exists_check(target_sql, params, "check_permission").await
    }

    async fn has_role(&self, user_id: &str, role_code: &str) -> Result<bool, TraitError> {
        debug!(
            "{:<12} - IamChecker::has_role - user: {}, code: {}",
            "IAM", user_id, role_code
        );

        let sql = r#"
            SELECT EXISTS(
              SELECT 1 FROM cmx_role r
              INNER JOIN cmx_user_role ur ON ur.role_id = r.id
              WHERE ur.user_id = $1 AND r.code = $2 AND r.status = 1
                AND ur.archived = 0 AND r.archived = 0
            )
        "#;
        let params = Value::Array(vec![
            Value::String(user_id.to_string()),
            Value::String(role_code.to_string()),
        ]);
        self.exists_check(sql, params, "check_role").await
    }

    async fn get_user_permissions(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - IamChecker::get_user_permissions - user: {}",
            "IAM", user_id
        );

        let sql = r#"
            SELECT DISTINCT p.code
            FROM cmx_permission p
            INNER JOIN cmx_role_permission rp ON rp.permission_id = p.id
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
            INNER JOIN cmx_role r ON r.id = ur.role_id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0
              AND p.archived = 0 AND p.status = 1
              AND r.archived = 0 AND r.status = 1
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "user_permissions")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户权限失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let permissions: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(permissions)
    }

    async fn get_user_role_codes(&self, user_id: &str) -> Result<Vec<String>, TraitError> {
        debug!(
            "{:<12} - IamChecker::get_user_role_codes - user: {}",
            "IAM", user_id
        );

        let sql = r#"
            SELECT DISTINCT r.code
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);

        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "user_role_codes")
            .await
            .map_err(|e| TraitError::Internal(format!("查询用户角色失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "code"))
            .collect();

        Ok(roles)
    }

    /// 获取用户的数据权限范围（默认返回 All，待后续实现）
    async fn get_data_scope(&self, _user_id: &str) -> Result<DataScope, TraitError> {
        Ok(DataScope::All)
    }
}
