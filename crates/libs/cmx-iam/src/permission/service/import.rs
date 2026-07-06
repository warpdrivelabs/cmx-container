//! 插件权限导入与清理
//!
//! 包含 `PermissionServiceImpl` 的固有方法 `import_permissions` / `cleanup_permissions`
//! 与 ZIP 解析校验 helper，以及插件权限文件解析结构体 `PermissionDefinition` / `PermissionFile`。
//!
//! 这些方法不属于 `PermissionService` trait，而是供插件导入处理器调用的固有 API。

use std::collections::HashSet;
use std::io::Read;

use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_traits::error::TraitError;
use cmx_traits::resource::ResourceDataImportResult;
use tracing::{info, instrument, warn};

use crate::audit_helper::AuditHelper;
use crate::error::IamError;
use crate::permission::service::PermissionServiceImpl;

// 权限定义/文件契约结构体统一定义在 cmx_core::model::iam,
// 此处从父模块(super,已 re-export cmx_core)引入,保持本文件内引用不变。
use super::{PermissionDefinition, PermissionFile};

impl PermissionServiceImpl {
    /// 解压 ZIP 并解析、校验所有 JSON 文件，合并返回权限定义列表。
    ///
    /// fail-fast：任何 ZIP/JSON 解析错误或校验失败立即返回错误。
    pub(super) fn parse_and_validate_permission_zip(
        zip_data: &[u8],
    ) -> Result<Vec<PermissionDefinition>, TraitError> {
        let cursor = std::io::Cursor::new(zip_data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| TraitError::Business(format!("ZIP 解压失败: {e}")))?;

        // ZIP 炸弹防护：限制条目数量和单个文件解压大小
        const MAX_ENTRIES: usize = 100;
        const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 单文件 10MB
        if archive.len() > MAX_ENTRIES {
            return Err(TraitError::Business(format!(
                "ZIP 条目数 {} 超过上限 {}，拒绝执行",
                archive.len(),
                MAX_ENTRIES
            )));
        }

        let mut all_definitions: Vec<PermissionDefinition> = Vec::new();
        let mut seen_codes: HashSet<String> = HashSet::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| TraitError::Business(format!("读取 ZIP 条目失败: {e}")))?;

            let name = file.name().to_string();
            // 仅处理 .json 文件，跳过目录
            if file.is_dir() || !name.ends_with(".json") {
                continue;
            }

            // 检查解压后大小，防止 ZIP 炸弹
            if file.size() > MAX_FILE_SIZE {
                return Err(TraitError::Business(format!(
                    "文件 {name} 解压后大小 {} 超过上限 {}，拒绝执行",
                    file.size(),
                    MAX_FILE_SIZE
                )));
            }

            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| TraitError::Business(format!("读取文件 {name} 失败: {e}")))?;

            let perm_file: PermissionFile = serde_json::from_str(&content)
                .map_err(|e| TraitError::Business(format!("文件 {name} JSON 解析失败: {e}")))?;

            for def in perm_file.permissions {
                // 校验：code 非空且含 ":" 分隔符
                if def.code.is_empty() {
                    return Err(TraitError::Business(format!(
                        "文件 {name} 中存在空权限 code"
                    )));
                }
                if !def.code.contains(':') {
                    return Err(TraitError::Business(format!(
                        "权限 code 格式不合规（需包含模块前缀 ':'）: {}",
                        def.code
                    )));
                }
                // 校验：resource_type ∈ {api,menu,button}（未指定时默认 api）
                let rt = def.resource_type.as_deref().unwrap_or("api");
                if !matches!(rt, "api" | "menu" | "button") {
                    return Err(TraitError::Business(format!(
                        "权限 resource_type 非法: {} (code={})",
                        rt, def.code
                    )));
                }
                // 校验：同一批次内 code 不重复
                if !seen_codes.insert(def.code.clone()) {
                    return Err(TraitError::Business(format!(
                        "重复的权限 code: {}",
                        def.code
                    )));
                }
                all_definitions.push(def);
            }
        }

        Ok(all_definitions)
    }

    /// 导入权限数据（从 ZIP 解压、解析、比对 DB、事务写入）。
    ///
    /// 完整流程：
    /// 1. 解压 ZIP 并解析校验 JSON
    /// 2. 事务内查询 DB 已有权限，计算新增/更新/删除集合
    /// 3. 第一阶段：INSERT/UPDATE（parent_id 暂置 NULL）
    /// 4. 第二阶段：回填 parent_id
    /// 5. 物理删除多余权限及其角色关联
    /// 6. 提交事务，写审计日志，失效缓存
    #[instrument(target = "cmx_iam_import", skip(self, zip_data), fields(domain = %domain_code, app = %app_code, module = %module_code))]
    pub async fn import_permissions(
        &self,
        svr_ctx: &SVRContext,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        zip_data: &[u8],
    ) -> Result<ResourceDataImportResult, TraitError> {
        // 1. 解压 + 解析 + 校验 JSON
        let definitions = Self::parse_and_validate_permission_zip(zip_data)?;

        // 安全校验：空 ZIP 或空 permissions 数组时拒绝执行，
        // 否则会导致该作用域下全部权限被误删（to_delete = db_codes - empty = db_codes 全集）
        if definitions.is_empty() {
            return Err(TraitError::from(IamError::Business(
                "ZIP 中未包含任何权限定义，拒绝执行导入（防止误删现有权限）".to_string(),
            )));
        }

        let _file_codes: HashSet<String> = definitions.iter().map(|d| d.code.clone()).collect();

        // 2. 开启事务（查询和写入在同一事务内，避免并发竞态）
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 2.1 事务内查询 DB 已有权限（按三元组）
        let db_map = self
            .query_permission_ids_by_scope_txn(txn_id, domain_code, app_code, module_code)
            .await?;
        let db_codes: HashSet<String> = db_map.keys().cloned().collect();

        // 2.2 比对
        let to_create: Vec<&PermissionDefinition> = definitions
            .iter()
            .filter(|d| !db_codes.contains(&d.code))
            .collect();
        let to_update: Vec<&PermissionDefinition> = definitions
            .iter()
            .filter(|d| db_codes.contains(&d.code))
            .collect();
        // let to_delete: Vec<String> = db_codes.difference(&file_codes).cloned().collect();

        // 3. 第一阶段：INSERT/UPDATE（parent_id 暂置 NULL）
        let mut code_to_id: std::collections::HashMap<String, String> = db_map.clone();
        let mut created_count = 0u32;

        for def in &to_create {
            let id = cmx_utils::id::snowflake_id_str();
            let full_code_path = format!("/{}", def.code);
            let sql = "INSERT INTO cmx_permission \
                       (id, code, name, resource_type, parent_id, sort_order, description, \
                        domain_code, app_code, module_code, extension, status, archived, \
                        parent_code, full_code_path, is_leaf, level) \
                       VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, $11, 0, NULL, $12, 1, 1)";
            let params = vec![
                DataValue::String(id.clone()),
                DataValue::String(def.code.clone()),
                DataValue::String(def.name.clone()),
                DataValue::String(
                    def.resource_type
                        .clone()
                        .unwrap_or_else(|| "api".to_string()),
                ),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().into(),
                DataValue::String(domain_code.to_string()),
                DataValue::String(app_code.to_string()),
                DataValue::String(module_code.to_string()),
                def.extension.clone().into(),
                DataValue::Int(def.status.unwrap_or(1)),
                DataValue::String(full_code_path),
            ];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!(
                        "新增权限失败 (code={}): {e}（可能权限 code 已被其他模块占用）",
                        def.code
                    )))
                })?;
            code_to_id.insert(def.code.clone(), id);
            created_count += 1;
        }

        let mut updated_count = 0u32;
        for def in &to_update {
            let id = db_map.get(&def.code).ok_or_else(|| {
                TraitError::Business(format!("更新权限时找不到 id: {}", def.code))
            })?;
            // UPDATE 按 id 定位，parent_id 暂置 NULL；路径字段重置为根节点（第二阶段回填覆盖）
            let sql = "UPDATE cmx_permission SET name = $1, resource_type = $2, parent_id = NULL, \
                       parent_code = NULL, full_code_path = '/' || code, level = 1, is_leaf = 1, \
                       sort_order = $3, description = $4, extension = $5, status = $6, update_time = NOW() \
                       WHERE id = $7";
            let params = vec![
                DataValue::String(def.name.clone()),
                DataValue::String(
                    def.resource_type
                        .clone()
                        .unwrap_or_else(|| "api".to_string()),
                ),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().into(),
                def.extension.clone().into(),
                DataValue::Int(def.status.unwrap_or(1)),
                DataValue::String(id.clone()),
            ];
            let rows = self
                .mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("更新权限失败 (id={id}): {e}")))
                })?;
            // 仅在实际更新行时计数（rows_affected > 0）
            if rows > 0 {
                updated_count += 1;
            }
        }

        // 4. 第二阶段：回填 parent_id（合并 db_map + 新增的映射）
        for def in &definitions {
            if let Some(parent_code) = &def.parent_code {
                let id_opt = code_to_id.get(&def.code).cloned();
                let parent_id_opt = code_to_id.get(parent_code).cloned();
                if let (Some(id), Some(parent_id)) = (id_opt, parent_id_opt) {
                    // 查父 meta（code/full_code_path/level），计算子节点路径
                    let parent_meta = self.query_parent_meta_txn(txn_id, &parent_id).await?;
                    match parent_meta {
                        Some((p_code, p_path, p_level)) => {
                            let new_path = format!("{}/{}", p_path, def.code);
                            let sql = "UPDATE cmx_permission SET parent_id = $1, parent_code = $2, \
                                       full_code_path = $3, level = $4 WHERE id = $5";
                            let params = vec![
                                DataValue::String(parent_id),
                                DataValue::String(p_code),
                                DataValue::String(new_path),
                                DataValue::Int(p_level + 1),
                                DataValue::String(id),
                            ];
                            self.mm
                                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                                .await
                                .map_err(|e| {
                                    TraitError::from(IamError::Business(format!(
                                        "回填 parent_id 失败 (code={}): {e}",
                                        def.code
                                    )))
                                })?;
                        }
                        None => {
                            warn!(code = %def.code, parent_code = %parent_code, "父权限 meta 未找到，降级为无父节点");
                        }
                    }
                } else {
                    warn!(code = %def.code, parent_code = %parent_code, "parent_code 未找到，降级为无父节点");
                }
            }
        }

        // 5. 删除前查询受影响角色（用于缓存失效）
        // 同时收集被更新的权限 ID，因为更新也可能影响缓存（name/status/parent_id 变更）
        // let to_delete_ids: Vec<String> = to_delete
        //     .iter()
        //     .filter_map(|c| db_map.get(c).cloned())
        //     .collect();
        let to_update_ids: Vec<String> = to_update
            .iter()
            .filter_map(|d| db_map.get(&d.code).cloned())
            .collect();
        // let mut affected_ids = to_delete_ids.clone();
        let affected_ids = to_update_ids.clone();
        // affected_ids.extend(to_update_ids);
        let affected_roles = self.query_affected_roles_txn(txn_id, &affected_ids).await?;

        let deleted_count = 0u32;

        // 5.1 物理删除权限 + 物理删除角色关联（按 id 定位）
        // let mut deleted_count = 0u32;
        // for code in &to_delete {
        //     let id = db_map
        //         .get(code)
        //         .ok_or_else(|| TraitError::Business(format!("删除权限时找不到 id: {code}")))?;
        //     let del_perm_sql = "DELETE FROM cmx_permission WHERE id = $1";
        //     let del_perm_params = vec![DataValue::String(id.clone())];
        //     let rows = self.mm
        //         .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_perm_sql, del_perm_params)
        //         .await
        //         .map_err(|e| {
        //             TraitError::from(IamError::Business(format!("删除权限失败 (id={id}): {e}")))
        //         })?;
        //
        //     // 仅在实际删除行时计数和清理角色关联
        //     if rows > 0 {
        //         deleted_count += 1;
        //         let del_rp_sql = "DELETE FROM cmx_role_permission WHERE permission_id = $1";
        //         let del_rp_params = vec![DataValue::String(id.clone())];
        //         self.mm
        //             .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_rp_sql, del_rp_params)
        //             .await
        //             .map_err(|e| {
        //                 TraitError::from(IamError::Business(format!(
        //                     "删除角色权限关联失败 (permission_id={id}): {e}"
        //                 )))
        //             })?;
        //     }
        // }

        // 6. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 6.1 全量重算 is_leaf（先全置1，再有子的置0）
        let leaf_sql1 = "UPDATE cmx_permission SET is_leaf = 1 \
                         WHERE domain_code = $1 AND app_code = $2 AND module_code = $3";
        let leaf_params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        let _ = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, leaf_sql1, leaf_params.clone())
            .await;
        let leaf_sql2 = "UPDATE cmx_permission SET is_leaf = 0 WHERE id IN \
                         (SELECT DISTINCT parent_id FROM cmx_permission \
                         WHERE parent_id IS NOT NULL \
                         AND domain_code = $1 AND app_code = $2 AND module_code = $3)";
        let _ = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, leaf_sql2, leaf_params)
            .await;

        // 7. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "domain_code": domain_code,
            "app_code": app_code,
            "module_code": module_code,
            "created": created_count,
            "updated": updated_count,
            // "deleted": deleted_count,
            "created_codes": to_create.iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
            "updated_codes": to_update.iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
            // "deleted_codes": to_delete,
        });
        self.audit_write(
            svr_ctx,
            "import_permissions",
            "permission",
            "batch",
            &audit_detail,
        )
        .await;

        // 8. 精准缓存失效（删除或更新都会影响权限树，需失效受影响角色）
        // if (deleted_count > 0 || updated_count > 0)
        if updated_count > 0
            && !affected_roles.is_empty()
            && let Some(ref checker) = self.iam_checker
        {
            for role_id in &affected_roles {
                checker.invalidate_role_cache(role_id).await;
            }
        }

        info!(
            target: "cmx_iam_import",
            created = created_count,
            updated = updated_count,
            // deleted = deleted_count,
            "权限导入完成"
        );

        Ok(ResourceDataImportResult {
            success: true,
            message: format!("导入完成: 新增 {} / 更新 {} ", created_count, updated_count),
            created_count,
            updated_count,
            deleted_count,
        })
    }

    /// 清理指定三元组作用域下的所有权限及其角色关联（物理删除）。
    ///
    /// 流程：
    /// 1. 开启事务
    /// 2. 查询受影响角色（用于缓存失效）
    /// 3. 物理删除角色-权限关联（用子查询避免 IN 列表过长）
    /// 4. 物理删除权限
    /// 5. 提交事务，精准失效缓存
    #[instrument(target = "cmx_iam_import", skip(self), fields(domain = %domain_code, app = %app_code, module = %module_code))]
    pub async fn cleanup_permissions(
        &self,
        svr_ctx: &SVRContext,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<ResourceDataImportResult, TraitError> {
        // 1. 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1.1 查询受影响角色（用子查询避免依赖额外参数）
        let affected_roles_sql = "SELECT DISTINCT role_id FROM cmx_role_permission \
             WHERE permission_id IN (SELECT id FROM cmx_permission \
             WHERE domain_code = $1 AND app_code = $2 AND module_code = $3)";
        let affected_roles_params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                affected_roles_sql,
                affected_roles_params,
                "cleanup_affected_roles",
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询受影响角色失败: {e}")))
            })?;
        let schema = dataset.schema.as_ref();
        let affected_roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "role_id"))
            .collect();

        // 1.2 物理删除角色关联（子查询避免 IN 列表过长）
        let del_rp_sql = "DELETE FROM cmx_role_permission WHERE permission_id IN (\
             SELECT id FROM cmx_permission \
             WHERE domain_code = $1 AND app_code = $2 AND module_code = $3)";
        let scope_params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        self.mm
            .execute_sql_with_datavalues(
                &self.db_id,
                Some(txn_id),
                del_rp_sql,
                scope_params.clone(),
            )
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("删除角色权限关联失败: {e}")))
            })?;

        // 1.3 物理删除权限
        let del_perm_sql = "DELETE FROM cmx_permission \
             WHERE domain_code = $1 AND app_code = $2 AND module_code = $3";
        let deleted = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_perm_sql, scope_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除权限失败: {e}"))))?;

        // 2. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        let deleted_count = u32::try_from(deleted).unwrap_or(u32::MAX);

        // 3. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "domain_code": domain_code,
            "app_code": app_code,
            "module_code": module_code,
            "deleted": deleted_count,
            "affected_roles": affected_roles,
        });
        self.audit_write(
            svr_ctx,
            "cleanup_permissions",
            "permission",
            "batch",
            &audit_detail,
        )
        .await;

        // 4. 精准缓存失效
        if !affected_roles.is_empty()
            && let Some(ref checker) = self.iam_checker
        {
            for role_id in &affected_roles {
                checker.invalidate_role_cache(role_id).await;
            }
        }

        info!(
            target: "cmx_iam_import",
            deleted = deleted_count,
            "权限清理完成"
        );

        Ok(ResourceDataImportResult {
            success: true,
            message: format!("清理完成: 删除 {} 条权限", deleted_count),
            created_count: 0,
            updated_count: 0,
            deleted_count,
        })
    }
}
