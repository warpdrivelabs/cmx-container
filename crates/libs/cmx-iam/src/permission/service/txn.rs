//! 权限服务事务内查询 helper
//!
//! 提供 `PermissionServiceImpl` 在事务内使用的辅助查询方法：
//! 作用域权限查询、受影响角色查询、父节点 meta 查询、后代 ID 收集、
//! 角色使用检查，以及旧父 `is_leaf` 重算。供导入/CRUD 子模块复用。

use std::collections::HashMap;

use cmx_core::model::cell::DataValue;
use cmx_traits::error::TraitError;

use crate::error::IamError;
use crate::permission::service::PermissionServiceImpl;

impl PermissionServiceImpl {
    /// 事务内查询指定三元组作用域下的权限集合（code -> id）。
    pub(super) async fn query_permission_ids_by_scope_txn(
        &self,
        txn_id: &str,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<HashMap<String, String>, TraitError> {
        let sql = "SELECT id, code FROM cmx_permission \
                   WHERE domain_code = $1 AND app_code = $2 AND module_code = $3 AND archived = 0";
        let params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "perm_scope_ids")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询作用域权限失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let mut map: HashMap<String, String> = HashMap::new();
        for row in dataset.iter() {
            let id = row.get_by_name_as::<String>(schema, "id");
            let code = row.get_by_name_as::<String>(schema, "code");
            if let (Some(id), Some(code)) = (id, code) {
                map.insert(code, id);
            }
        }
        Ok(map)
    }

    /// 事务内查询受权限删除影响的 role_id 列表（用于精准缓存失效）。
    pub(super) async fn query_affected_roles_txn(
        &self,
        txn_id: &str,
        permission_ids: &[String],
    ) -> Result<Vec<String>, TraitError> {
        if permission_ids.is_empty() {
            return Ok(Vec::new());
        }
        // 使用 IN + 动态占位符（驱动不支持 ANY($1) 数组绑定）
        let placeholders: Vec<String> = (1..=permission_ids.len())
            .map(|i| format!("${i}"))
            .collect();
        let in_clause = placeholders.join(",");
        let sql = format!(
            "SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id IN ({in_clause}) AND archived = 0"
        );
        let params: Vec<DataValue> = permission_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                &sql,
                params,
                "perm_affected_roles",
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询受影响角色失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let role_ids: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "role_id"))
            .collect();
        Ok(role_ids)
    }

    /// 事务内查询父节点的 code/full_code_path/level（用于计算子节点路径字段）。
    pub(super) async fn query_parent_meta_txn(
        &self,
        txn_id: &str,
        parent_id: &str,
    ) -> Result<Option<(String, String, i64)>, TraitError> {
        let sql =
            "SELECT code, full_code_path, level FROM cmx_permission WHERE id = $1 AND archived = 0";
        let params = vec![DataValue::String(parent_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "parent_meta")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询父权限失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        if let Some(row) = dataset.iter().next() {
            let code = row.get_by_name_as::<String>(schema, "code");
            let path = row.get_by_name_as::<String>(schema, "full_code_path");
            let level = row.get_by_name_as::<i64>(schema, "level").unwrap_or(1);
            if let (Some(c), Some(p)) = (code, path) {
                return Ok(Some((c, p, level)));
            }
        }
        Ok(None)
    }

    /// 事务内按 full_code_path LIKE 查询节点自身及所有后代 ID。
    pub(super) async fn collect_descendants_by_path_txn(
        &self,
        txn_id: &str,
        root_path: &str,
    ) -> Result<Vec<String>, TraitError> {
        let sql = "SELECT id FROM cmx_permission WHERE (full_code_path = $1 OR full_code_path LIKE ($2 || '/%')) AND archived = 0";
        let params = vec![
            DataValue::String(root_path.to_string()),
            DataValue::String(root_path.to_string()),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                sql,
                params,
                "descendants_by_path",
            )
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询子权限失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let ids: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
            .collect();
        Ok(ids)
    }

    /// 事务内查询权限被哪些角色使用，返回阻止详情（空则无阻止）。
    pub(super) async fn check_usage_by_roles_txn(
        &self,
        txn_id: &str,
        permission_ids: &[String],
    ) -> Result<Vec<crate::permission::BlockedPermissionInfo>, TraitError> {
        use crate::permission::{BlockedPermissionInfo, BlockedRoleInfo};
        if permission_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> = (1..=permission_ids.len())
            .map(|i| format!("${i}"))
            .collect();
        let in_clause = placeholders.join(",");
        let sql = format!(
            "SELECT p.id AS pid, p.code AS pcode, p.name AS pname, \
             r.id AS rid, r.code AS rcode, r.name AS rname \
             FROM cmx_permission p \
             JOIN cmx_role_permission rp ON rp.permission_id = p.id AND rp.archived = 0 \
             JOIN cmx_role r ON r.id = rp.role_id AND r.archived = 0 \
             WHERE p.id IN ({in_clause}) AND p.archived = 0"
        );
        let params: Vec<DataValue> = permission_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), &sql, params, "check_perm_usage")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询权限使用情况失败: {e}")))
            })?;
        let schema = dataset.schema.as_ref();
        let mut map: HashMap<String, BlockedPermissionInfo> = HashMap::new();
        for row in dataset.iter() {
            let pid = match row.get_by_name_as::<String>(schema, "pid") {
                Some(v) => v,
                None => continue,
            };
            let pcode = row
                .get_by_name_as::<String>(schema, "pcode")
                .unwrap_or_default();
            let pname = row
                .get_by_name_as::<String>(schema, "pname")
                .unwrap_or_default();
            let rid = match row.get_by_name_as::<String>(schema, "rid") {
                Some(v) => v,
                None => continue,
            };
            let rcode = row
                .get_by_name_as::<String>(schema, "rcode")
                .unwrap_or_default();
            let rname = row
                .get_by_name_as::<String>(schema, "rname")
                .unwrap_or_default();
            let entry = map
                .entry(pid.clone())
                .or_insert_with(|| BlockedPermissionInfo {
                    permission_id: pid,
                    permission_code: pcode,
                    permission_name: pname,
                    roles: vec![],
                });
            entry.roles.push(BlockedRoleInfo {
                role_id: rid,
                role_code: rcode,
                role_name: rname,
            });
        }
        Ok(map.into_values().collect())
    }

    /// 重算给定 parent_id 的 is_leaf（若无子节点置1，否则保持0）。供 delete/update 旧父用。
    pub(super) async fn recompute_parent_is_leaf(&self, parent_id: &str) {
        let sql = "UPDATE cmx_permission SET is_leaf = 1 WHERE id = $1 \
                   AND NOT EXISTS (SELECT 1 FROM cmx_permission c WHERE c.parent_id = $1)";
        let _ = self
            .mm
            .execute_sql_with_datavalues(
                &self.db_id,
                None,
                sql,
                vec![DataValue::String(parent_id.to_string())],
            )
            .await;
    }
}
