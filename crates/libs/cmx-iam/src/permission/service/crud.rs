//! 权限 CRUD
//!
//! 实现 [`crate::service_traits::PermissionService`] 的创建/查询/更新/删除方法。
//! 方法体集中在本固有方法中，trait 实现在 `mod.rs` 中逐方法委托。

use std::collections::HashSet;

use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_core::model::iam::Permission;
use cmx_database::crud::GenericCrudService;
use cmx_traits::error::TraitError;
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::error::IamError;
use crate::permission::service::PermissionServiceImpl;
use crate::permission::{PermissionBmc, PermissionForCreate, PermissionForUpdate};

impl PermissionServiceImpl {
    /// 创建权限（[`crate::service_traits::PermissionService::create_permission`] 的实现）。
    ///
    /// 校验权限编码唯一性后写入数据库，计算路径字段，更新父 is_leaf，并写审计日志。
    pub(super) async fn create_permission(
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
        let check_params = vec![DataValue::String(data.code.clone())];
        let existing = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                None,
                check_sql,
                check_params,
                "check_perm_code",
            )
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限编码失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::PermissionCodeExists(
                data.code.clone(),
            )));
        }

        // 开启事务（INSERT + 父 is_leaf 更新需原子）
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 计算路径字段（full_code_path / level / parent_code）
        let (parent_code, full_code_path, level) = if let Some(ref pid) = data.parent_id {
            let meta = self
                .query_parent_meta_txn(txn_id, pid)
                .await?
                .ok_or_else(|| {
                    TraitError::from(IamError::Business(format!("父权限不存在: {pid}")))
                })?;
            let (p_code, p_path, p_level) = meta;
            (
                Some(p_code),
                format!("{}/{}", p_path, data.code),
                p_level + 1,
            )
        } else {
            (None, format!("/{}", data.code), 1i64)
        };

        // INSERT（含4新字段），RETURNING * 取回完整行
        let id = cmx_utils::id::snowflake_id_str();
        let sql = "INSERT INTO cmx_permission \
                   (id, code, name, resource_type, parent_id, sort_order, description, \
                    domain_code, app_code, module_code, extension, status, archived, \
                    parent_code, full_code_path, is_leaf, level) \
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 0, $13, $14, 1, $15) \
                   RETURNING *";
        let params = vec![
            DataValue::String(id),
            DataValue::String(data.code.clone()),
            DataValue::String(data.name.clone()),
            data.resource_type.clone().into(),
            data.parent_id.clone().into(),
            data.sort_order.unwrap_or(0).into(),
            data.description.clone().into(),
            data.domain_code.clone().into(),
            data.app_code.clone().into(),
            data.module_code.clone().into(),
            data.extension.clone().into(),
            DataValue::Int(1),
            parent_code.clone().into(),
            DataValue::String(full_code_path),
            DataValue::Int(level),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "create_perm")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("新增权限失败: {e}"))))?;

        // 父节点 is_leaf = 0（新子节点已挂载）
        if let Some(ref pid) = data.parent_id {
            let upd_sql = "UPDATE cmx_permission SET is_leaf = 0 WHERE id = $1";
            let _ = self
                .mm
                .execute_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    upd_sql,
                    vec![DataValue::String(pid.clone())],
                )
                .await;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        let permission = Self::extract_permission(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "code": &data.code,
            "name": &data.name,
        });
        self.audit_write(
            svr_ctx,
            "create_permission",
            "permission",
            &permission.id,
            &audit_detail,
        )
        .await;

        info!(permission_id = %permission.id, code = %data.code, "权限创建成功");
        Ok(permission)
    }

    /// 获取单个权限（[`crate::service_traits::PermissionService::get_permission`] 的实现）。
    pub(super) async fn get_permission(
        &self,
        permission_id: &str,
    ) -> Result<Permission, TraitError> {
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
            return Err(TraitError::from(IamError::PermissionNotFound(
                permission_id.to_string(),
            )));
        }

        Self::extract_permission(dataset).map_err(TraitError::from)
    }

    /// 更新权限（[`crate::service_traits::PermissionService::update_permission`] 的实现）。
    ///
    /// 当 `parent_id` 变更时级联重算 `full_code_path` / `level`，并更新新旧父 is_leaf。
    pub(super) async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::update_permission - {}",
            "IAM", permission_id
        );

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 查询当前权限的 parent_id / full_code_path / level / code
        let meta_sql =
            "SELECT parent_id, full_code_path, level, code FROM cmx_permission WHERE id = $1";
        let meta_params = vec![DataValue::String(permission_id.to_string())];
        let meta_dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                meta_sql,
                meta_params,
                "update_perm_meta",
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询权限元数据失败: {e}")))
            })?;
        let schema = meta_dataset.schema.as_ref();
        let row = meta_dataset.iter().next().ok_or_else(|| {
            TraitError::from(IamError::PermissionNotFound(permission_id.to_string()))
        })?;
        let old_parent_id = row.get_by_name_as::<String>(schema, "parent_id");
        let old_path = row
            .get_by_name_as::<String>(schema, "full_code_path")
            .unwrap_or_default();
        let old_level = row.get_by_name_as::<i64>(schema, "level").unwrap_or(1);
        let perm_code = row
            .get_by_name_as::<String>(schema, "code")
            .unwrap_or_default();

        // 规范化：空字符串视为 None（根节点）
        let old_parent_norm = old_parent_id.as_deref().filter(|s| !s.is_empty());
        let new_parent_norm = data.parent_id.as_deref().filter(|s| !s.is_empty());
        // 仅当 data.parent_id 显式提供且与旧值不同时才级联
        let parent_changed = data.parent_id.is_some() && new_parent_norm != old_parent_norm;
        let old_parent_for_recompute = old_parent_norm.map(|s| s.to_string());

        let dataset = if parent_changed {
            // ---- parent_id 变更：级联重算 ----
            // 查新父 meta，计算新 path / level / parent_code
            let (new_parent_code, new_path, new_level) = if let Some(new_pid) = new_parent_norm {
                let meta = self
                    .query_parent_meta_txn(txn_id, new_pid)
                    .await?
                    .ok_or_else(|| {
                        TraitError::from(IamError::Business(format!("新父权限不存在: {new_pid}")))
                    })?;
                let (p_code, p_path, p_level) = meta;
                (
                    Some(p_code),
                    format!("{}/{}", p_path, perm_code),
                    p_level + 1,
                )
            } else {
                (None, format!("/{perm_code}"), 1i64)
            };

            // 级联更新 N 及后代 full_code_path / level
            let cascade_sql = "UPDATE cmx_permission SET \
                full_code_path = $2 || SUBSTRING(full_code_path FROM $3), \
                level = level + ($4 - $5) \
                WHERE full_code_path = $1 OR full_code_path LIKE ($1 || '/%')";
            let cascade_params = vec![
                DataValue::String(old_path.clone()),
                DataValue::String(new_path.clone()),
                DataValue::Int(old_path.len() as i64 + 1),
                DataValue::Int(new_level),
                DataValue::Int(old_level),
            ];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), cascade_sql, cascade_params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("级联更新路径失败: {e}")))
                })?;

            // 更新节点 parent_id / parent_code + 普通字段，RETURNING *
            let upd_sql = "UPDATE cmx_permission SET \
                parent_id = $1, parent_code = $2, \
                name = COALESCE($3, name), \
                resource_type = COALESCE($4, resource_type), \
                sort_order = COALESCE($5, sort_order), \
                status = COALESCE($6, status), \
                description = COALESCE($7, description), \
                domain_code = COALESCE($8, domain_code), \
                app_code = COALESCE($9, app_code), \
                module_code = COALESCE($10, module_code), \
                extension = COALESCE($11, extension), \
                update_time = NOW() \
                WHERE id = $12 RETURNING *";
            let params = vec![
                new_parent_norm.map(|s| s.to_string()).into(),
                new_parent_code.into(),
                data.name.clone().into(),
                data.resource_type.clone().into(),
                data.sort_order.into(),
                data.status.into(),
                data.description.clone().into(),
                data.domain_code.clone().into(),
                data.app_code.clone().into(),
                data.module_code.clone().into(),
                data.extension.clone().into(),
                DataValue::String(permission_id.to_string()),
            ];
            let ds = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    upd_sql,
                    params,
                    "update_perm",
                )
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("更新权限失败: {e}"))))?;

            // 新父 is_leaf = 0
            if let Some(new_pid) = new_parent_norm {
                let leaf_sql = "UPDATE cmx_permission SET is_leaf = 0 WHERE id = $1";
                let _ = self
                    .mm
                    .execute_sql_with_datavalues(
                        &self.db_id,
                        Some(txn_id),
                        leaf_sql,
                        vec![DataValue::String(new_pid.to_string())],
                    )
                    .await;
            }
            ds
        } else {
            // ---- parent_id 未变更：仅更新普通字段 ----
            let upd_sql = "UPDATE cmx_permission SET \
                name = COALESCE($1, name), \
                resource_type = COALESCE($2, resource_type), \
                sort_order = COALESCE($3, sort_order), \
                status = COALESCE($4, status), \
                description = COALESCE($5, description), \
                domain_code = COALESCE($6, domain_code), \
                app_code = COALESCE($7, app_code), \
                module_code = COALESCE($8, module_code), \
                extension = COALESCE($9, extension), \
                update_time = NOW() \
                WHERE id = $10 RETURNING *";
            let params = vec![
                data.name.clone().into(),
                data.resource_type.clone().into(),
                data.sort_order.into(),
                data.status.into(),
                data.description.clone().into(),
                data.domain_code.clone().into(),
                data.app_code.clone().into(),
                data.module_code.clone().into(),
                data.extension.clone().into(),
                DataValue::String(permission_id.to_string()),
            ];
            self.mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    Some(txn_id),
                    upd_sql,
                    params,
                    "update_perm",
                )
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("更新权限失败: {e}"))))?
        };

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 旧父重算 is_leaf（提交后，因为 recompute 使用 None txn_id）
        if parent_changed && let Some(ref old_pid) = old_parent_for_recompute {
            self.recompute_parent_is_leaf(old_pid).await;
        }

        let permission = Self::extract_permission(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "description": &data.description,
        });
        self.audit_write(
            svr_ctx,
            "update_permission",
            "permission",
            permission_id,
            &audit_detail,
        )
        .await;

        info!(permission_id = permission_id, "权限更新成功");
        Ok(permission)
    }

    /// 批量删除权限（含级联删除子树 + 角色使用检查，[`crate::service_traits::PermissionService::delete_permission`] 的实现）。
    ///
    /// 流程：查根 meta → 按 path 收集后代 → 检查角色使用（有则 Blocked）→ 物理删除关联+权限 → 重算旧父 is_leaf。
    pub(super) async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<crate::permission::DeletePermissionOutcome, TraitError> {
        use crate::permission::{DeletePermissionBlocked, DeletePermissionResult};

        debug!(
            "{:<12} - PermissionServiceImpl::delete_permission - count: {}",
            "IAM",
            permission_ids.len()
        );

        if permission_ids.is_empty() {
            return Ok(crate::permission::DeletePermissionOutcome::Deleted {
                result: DeletePermissionResult {
                    deleted_permission_ids: vec![],
                    deleted_count: 0,
                },
            });
        }

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 查询每个根权限的 full_code_path 和 parent_id
        let placeholders: Vec<String> = (1..=permission_ids.len())
            .map(|i| format!("${i}"))
            .collect();
        let in_clause = placeholders.join(",");
        let meta_sql = format!(
            "SELECT id, full_code_path, parent_id FROM cmx_permission WHERE id IN ({in_clause})"
        );
        let meta_params: Vec<DataValue> = permission_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        let meta_dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                &meta_sql,
                meta_params,
                "delete_perm_meta",
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询删除权限元数据失败: {e}")))
            })?;
        let schema = meta_dataset.schema.as_ref();

        // 2. 收集每个根的所有后代 ID（含自身），用 HashSet 去重
        let mut all_ids: HashSet<String> = HashSet::new();
        let mut parent_ids_to_recompute: Vec<String> = Vec::new();
        for row in meta_dataset.iter() {
            let path = match row.get_by_name_as::<String>(schema, "full_code_path") {
                Some(p) => p,
                None => continue,
            };
            if let Some(pid) = row.get_by_name_as::<String>(schema, "parent_id")
                && !pid.is_empty()
            {
                parent_ids_to_recompute.push(pid);
            }
            let descendants = self.collect_descendants_by_path_txn(txn_id, &path).await?;
            all_ids.extend(descendants);
        }

        let all_ids_vec: Vec<String> = all_ids.iter().cloned().collect();

        // 3. 检查角色使用情况
        let blocked = self.check_usage_by_roles_txn(txn_id, &all_ids_vec).await?;
        if !blocked.is_empty() {
            // guard drop 自动结束只读事务
            return Ok(crate::permission::DeletePermissionOutcome::Blocked {
                detail: DeletePermissionBlocked {
                    blocked_permissions: blocked,
                },
            });
        }

        // 4. 查询受影响角色（删除前，用于缓存失效）
        let affected_roles = self.query_affected_roles_txn(txn_id, &all_ids_vec).await?;

        // 5. 物理删除角色关联
        let del_placeholders: Vec<String> =
            (1..=all_ids_vec.len()).map(|i| format!("${i}")).collect();
        let del_in_clause = del_placeholders.join(",");
        let del_rp_sql =
            format!("DELETE FROM cmx_role_permission WHERE permission_id IN ({del_in_clause})");
        let del_rp_params: Vec<DataValue> = all_ids_vec
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), &del_rp_sql, del_rp_params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("删除角色权限关联失败: {e}")))
            })?;

        // 6. 物理删除权限
        let del_perm_sql = format!("DELETE FROM cmx_permission WHERE id IN ({del_in_clause})");
        let del_perm_params: Vec<DataValue> = all_ids_vec
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), &del_perm_sql, del_perm_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除权限失败: {e}"))))?;

        // 7. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 8. 提交后重算旧父 is_leaf
        for pid in &parent_ids_to_recompute {
            self.recompute_parent_is_leaf(pid).await;
        }

        // 9. 审计日志
        let deleted_count = all_ids_vec.len() as u64;
        let audit_detail = serde_json::json!({
            "root_permission_ids": permission_ids,
            "deleted_permission_ids": all_ids_vec,
            "deleted_count": deleted_count,
        });
        self.audit_write(
            svr_ctx,
            "delete_permission",
            "permission",
            "batch",
            &audit_detail,
        )
        .await;

        // 10. 精准缓存失效
        if !affected_roles.is_empty()
            && let Some(ref checker) = self.iam_checker
        {
            for role_id in &affected_roles {
                checker.invalidate_role_cache(role_id).await;
            }
        }

        info!(count = deleted_count, "权限删除成功");
        Ok(crate::permission::DeletePermissionOutcome::Deleted {
            result: DeletePermissionResult {
                deleted_permission_ids: all_ids.iter().cloned().collect(),
                deleted_count,
            },
        })
    }
}
