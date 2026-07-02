//! 临时角色授权
//!
//! 实现 [`crate::service_traits::UserService`] 的临时角色授权生命周期方法
//! （分配/撤销/批量撤销/延长）与有效权限聚合查询。

use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_traits::error::TraitError;
use cmx_utils::snowflake_id_str;
use tracing::debug;

use crate::audit_helper::AuditHelper;
use crate::error::IamError;
use crate::service_traits::{
    EffectivePermissionsResponse, PermissionSummary, RoleSummary, UserRoleAssignment,
};
use crate::user::service::UserServiceImpl;

impl UserServiceImpl {
    /// 分配临时角色（带有效期，[`crate::service_traits::UserService::assign_temp_role`] 的实现）。
    ///
    /// 支持有效期范围、来源标记和撤销原因。当配置了 SoD 规则执行器时，会先校验角色互斥约束。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn assign_temp_role(
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
        let params = vec![
            DataValue::String(assignment_id.clone()),
            DataValue::String(user_id.to_string()),
            DataValue::String(role_id.to_string()),
            DataValue::DateTime(effective_from),
            DataValue::DateTime(effective_until),
            reason
                .map(|s| DataValue::String(s.to_string()))
                .unwrap_or(DataValue::Null),
            DataValue::String(source.to_string()),
        ];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, None, insert_sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("分配临时角色失败: {e}"))))?;

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
        let params = vec![DataValue::String(assignment_id)];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, query_sql, params, "temp_assignment")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询临时授权失败: {e}"))))?;

        Self::extract_assignments(dataset)
            .into_iter()
            .next()
            .ok_or_else(|| {
                TraitError::from(IamError::Business("临时授权创建后查询失败".to_string()))
            })
    }

    /// 撤销临时角色（逻辑撤销 status=0，[`crate::service_traits::UserService::revoke_temp_role`] 的实现）。
    pub(super) async fn revoke_temp_role(
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
        let params = vec![
            DataValue::String(assignment_id.to_string()),
            DataValue::String(operator),
        ];
        let affected = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, update_sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("撤销临时角色失败: {e}"))))?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "临时授权记录不存在或已撤销: {assignment_id}"
            ))));
        }

        // 查询 assignment 对应的 user_id（用于审计 target_id 和缓存失效）
        let user_id = {
            let query_sql = "SELECT user_id FROM cmx_user_role_assignment WHERE id = $1";
            let params = vec![DataValue::String(assignment_id.to_string())];
            let dataset = self
                .mm
                .query_sql_with_datavalues(&self.db_id, None, query_sql, params, "revoke_get_user")
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("查询用户ID失败: {e}")))
                })?;
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

    /// 批量撤销临时角色（[`crate::service_traits::UserService::revoke_temp_roles_batch`] 的实现）。
    ///
    /// 单事务撤销多个授权记录，聚合审计日志，失效所有受影响用户的权限缓存。
    pub(super) async fn revoke_temp_roles_batch(
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
            let params = vec![DataValue::String(assignment_id.clone())];
            if let Ok(dataset) = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    query_sql,
                    params,
                    "batch_revoke_get_user",
                )
                .await
            {
                let schema = dataset.schema.as_ref();
                if let Some(uid) = dataset
                    .iter()
                    .next()
                    .and_then(|row| row.get_by_name_as::<String>(schema, "user_id"))
                    && !affected_user_ids.contains(&uid)
                {
                    affected_user_ids.push(uid);
                }
            }

            let update_sql = r#"
                UPDATE cmx_user_role_assignment
                SET status = 0, revoked_by = $2, revoked_at = NOW(), update_time = NOW()
                WHERE id = $1 AND status = 1 AND archived = 0
            "#;
            let params = vec![
                DataValue::String(assignment_id.clone()),
                DataValue::String(operator.clone()),
            ];
            let affected = self
                .mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), update_sql, params)
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

    /// 延长临时授权有效期（[`crate::service_traits::UserService::extend_temp_role`] 的实现）。
    ///
    /// 新失效时间必须晚于原失效时间。
    pub(super) async fn extend_temp_role(
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
        let params = vec![DataValue::String(assignment_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, query_sql, params, "query_assignment")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询临时授权失败: {e}"))))?;

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
        let params = vec![
            DataValue::String(assignment_id.to_string()),
            DataValue::DateTime(new_effective_until),
        ];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, None, update_sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("延长临时授权失败: {e}"))))?;

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

    /// 查询用户有效权限（合并永久 + 临时授权，[`crate::service_traits::UserService::get_effective_permissions`] 的实现）。
    pub(super) async fn get_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<EffectivePermissionsResponse, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_effective_permissions - user: {}",
            "IAM", user_id
        );

        // 1. 查询用户基本信息
        let user_sql = "SELECT id, username FROM cmx_user WHERE id = $1 AND archived = 0";
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, user_sql, params, "user_info")
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, roles_sql, params, "effective_roles")
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, perms_sql, params, "effective_perms")
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, active_count_sql, params, "active_count")
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                None,
                expired_count_sql,
                params,
                "expired_count",
            )
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                None,
                upcoming_sql,
                params,
                "upcoming_expirations",
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("统计即将过期角色失败: {e}")))
            })?;
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
