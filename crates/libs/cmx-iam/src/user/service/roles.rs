//! 永久角色授权
//!
//! 实现 [`crate::service_traits::UserService::assign_roles`]，按 username 查询用户后
//! 全量替换其角色关联（事务保证原子性），含 SoD 校验与缓存失效。

use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_traits::error::TraitError;
use cmx_utils::snowflake_id_str;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::error::IamError;
use crate::user::service::UserServiceImpl;

impl UserServiceImpl {
    /// 为用户分配角色（全量替换，[`crate::service_traits::UserService::assign_roles`] 的实现）。
    ///
    /// 事务保证原子性，按 username 查询。空 `role_ids` 表示清空所有角色。
    pub(super) async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        username: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::assign_roles - username: {}, role_count: {}",
            "IAM",
            username,
            role_ids.len()
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
        let resolve_params = vec![DataValue::String(username.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                resolve_sql,
                resolve_params,
                "resolve_user_id",
            )
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
        let delete_params = vec![DataValue::String(user_id.clone())];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), delete_sql, delete_params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("删除旧角色关联失败: {e}")))
            })?;

        // 2. 批量插入新关联
        for role_id in role_ids {
            let ur_id = snowflake_id_str();
            let insert_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived) \
                              VALUES ($1, $2, $3, 0) ON CONFLICT (user_id, role_id) WHERE archived = 0 DO NOTHING";
            let params = vec![
                DataValue::String(ur_id),
                DataValue::String(user_id.clone()),
                DataValue::String(role_id.clone()),
            ];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("插入用户角色关联失败: {e}")))
                })?;
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
}
