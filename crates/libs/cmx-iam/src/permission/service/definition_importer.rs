//! `PermissionDefinitionImporter` trait 的实现。
//!
//! 将模块导入场景的两阶段权限 upsert 逻辑收敛到 cmx-iam,
//! 消除 cmx-plugin::module_install 中重复的手写 SQL 实现。
//!
//! 与 `import_permissions`(面向 ZIP + diff + 审计 + 缓存)的区别:
//! 本实现仅做结构体列表 → 两阶段 upsert,无 diff/删除/审计/缓存失效,
//! 语义对齐模块导入(幂等 upsert,不清理旧权限)。

use async_trait::async_trait;
use cmx_core::model::cell::DataValue;
use cmx_core::model::iam::PermissionDefinition;
use cmx_traits::error::TraitError;
use cmx_traits::iam::PermissionDefinitionImporter;
use tracing::{info, warn};

use crate::permission::service::PermissionServiceImpl;

#[async_trait]
impl PermissionDefinitionImporter for PermissionServiceImpl {
    /// 两阶段 upsert 权限定义到 cmx_permission。
    ///
    /// 1. 第一阶段:ON CONFLICT(code) upsert,parent_id 暂置 NULL,full_code_path = '/' + code
    /// 2. 第二阶段:回填 parent_id / parent_code / full_code_path / level,父节点 is_leaf = 0
    async fn apply_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[PermissionDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }

        // 1. 第一阶段:upsert 所有权限(parent_id 暂置 NULL,full_code_path='/'+code)
        let mut code_to_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for def in definitions {
            let id = cmx_utils::id::snowflake_id_str();
            let resource_type = def.resource_type.clone().unwrap_or_else(|| "api".to_string());
            let full_path = format!("/{}", def.code);

            // upsert:ON CONFLICT (code) DO UPDATE,RETURNING id 取实际入库 id
            let sql = "INSERT INTO cmx_permission \
                       (id, code, name, resource_type, parent_id, sort_order, description, \
                        domain_code, app_code, module_code, extension, status, archived, \
                        parent_code, full_code_path, is_leaf, level) \
                       VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, $11, 0, NULL, $12, 1, 1) \
                       ON CONFLICT (code) DO UPDATE SET \
                       name = EXCLUDED.name, resource_type = EXCLUDED.resource_type, \
                       sort_order = EXCLUDED.sort_order, description = EXCLUDED.description, \
                       extension = EXCLUDED.extension, status = EXCLUDED.status, \
                       domain_code = EXCLUDED.domain_code, app_code = EXCLUDED.app_code, \
                       module_code = EXCLUDED.module_code, \
                       full_code_path = EXCLUDED.full_code_path, \
                       update_time = CURRENT_TIMESTAMP \
                       RETURNING id";
            let params: Vec<DataValue> = vec![
                DataValue::String(id.clone()),
                DataValue::String(def.code.clone()),
                DataValue::String(def.name.clone()),
                DataValue::String(resource_type),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().into(),
                DataValue::String(domain_code.to_string()),
                DataValue::String(app_code.to_string()),
                DataValue::String(module_code.to_string()),
                def.extension.clone().into(),
                DataValue::Int(def.status.unwrap_or(1)),
                DataValue::String(full_path),
            ];
            match self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    None,
                    sql,
                    params,
                    "apply_perm_upsert",
                )
                .await
            {
                Ok(ds) => {
                    let json = serde_json::to_value(&ds).unwrap_or_default();
                    let returned_id = json
                        .get("rows")
                        .and_then(|r| r.as_array())
                        .and_then(|rows| rows.first())
                        .and_then(|row| row.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    code_to_id.insert(def.code.clone(), returned_id);
                }
                Err(e) => {
                    warn!(perm_code = %def.code, error = %e, "权限 upsert 失败");
                }
            }
        }

        // 2. 第二阶段:回填 parent_id / parent_code / full_code_path / level
        for def in definitions {
            let Some(parent_code) = &def.parent_code else {
                continue;
            };
            let Some(parent_id) = code_to_id.get(parent_code) else {
                warn!(perm_code = %def.code, parent_code = %parent_code, "父权限未找到,跳过回填");
                continue;
            };
            let Some(child_id) = code_to_id.get(&def.code) else {
                continue;
            };
            // 查父节点 full_code_path/level
            let parent_sql = "SELECT full_code_path, level FROM cmx_permission WHERE id = $1";
            let parent_ds = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    None,
                    parent_sql,
                    vec![DataValue::String(parent_id.clone())],
                    "apply_perm_parent",
                )
                .await;
            if let Ok(pds) = parent_ds {
                let pjson = serde_json::to_value(&pds).unwrap_or_default();
                let p_path = pjson
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("full_code_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let p_level = pjson
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("level"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                let new_path = format!("{p_path}/{}", def.code);
                let new_level = p_level + 1;
                // 更新子节点的 parent 引用
                let upd_sql = "UPDATE cmx_permission SET parent_id = $1, parent_code = $2, \
                               full_code_path = $3, level = $4 WHERE id = $5";
                let _ = self
                    .mm
                    .execute_sql_with_datavalues(
                        &self.db_id,
                        None,
                        upd_sql,
                        vec![
                            DataValue::String(parent_id.clone()),
                            DataValue::String(parent_code.clone()),
                            DataValue::String(new_path),
                            DataValue::Int(new_level),
                            DataValue::String(child_id.clone()),
                        ],
                    )
                    .await;
                // 父节点 is_leaf = 0
                let leaf_sql = "UPDATE cmx_permission SET is_leaf = 0 WHERE id = $1";
                let _ = self
                    .mm
                    .execute_sql_with_datavalues(
                        &self.db_id,
                        None,
                        leaf_sql,
                        vec![DataValue::String(parent_id.clone())],
                    )
                    .await;
            }
        }
        info!(count = definitions.len(), "权限定义 upsert 完成");
        Ok(definitions.len())
    }
}
