//! 权限服务实现 — `PermissionServiceImpl`。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use cmx_core::SVRContext;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::plugin::PluginDataImportResult;
use modql::filter::{ListOptions, OpValInt64, OpValsInt64};
use cmx_core::model::cell::DataValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, instrument, warn};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::iam_checker::IamChecker;
use crate::permission::{PermissionBmc, PermissionFilter, PermissionForCreate, PermissionForUpdate};
use crate::service_traits::PermissionService;

/// 权限服务实现。
pub struct PermissionServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,
    /// 认证库 `db_id`。
    db_id: String,
    /// IAM 配置（预留：用于权限缓存 TTL 等扩展）。
    #[allow(dead_code)]
    config: IamConfig,
    /// 审计日志记录器（可选）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
    /// IAM 权限校验器（可选，用于精准缓存失效）。
    iam_checker: Option<Arc<IamChecker>>,
}

impl PermissionServiceImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id`。
    ///
    /// # Returns
    ///
    /// 返回 `PermissionServiceImpl` 实例，未设置审计记录器。
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self {
            mm,
            db_id,
            config,
            audit: None,
            iam_checker: None,
        }
    }

    /// 设置审计日志记录器（Builder 模式）。
    ///
    /// # Arguments
    ///
    /// * `audit` - 审计日志记录器。
    ///
    /// # Returns
    ///
    /// 返回 `Self`，便于链式调用。
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 设置 IAM 权限校验器（Builder 模式）。
    ///
    /// 注入后，导入/清理权限操作完成时会触发精准缓存失效。
    ///
    /// # Arguments
    ///
    /// * `checker` - IAM 权限校验器。
    ///
    /// # Returns
    ///
    /// 返回 `Self`，便于链式调用。
    pub fn with_iam_checker(mut self, checker: Arc<IamChecker>) -> Self {
        self.iam_checker = Some(checker);
        self
    }

    /// 从 DataSet 第一行提取 `Permission`。
    fn extract_permission(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<Permission, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::PermissionNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<Permission>(json_val)
            .map_err(|e| IamError::Business(format!("权限反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 `Permission` 列表。
    fn extract_permissions(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<Permission> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<Permission>(json_val).ok()
            })
            .collect()
    }

    /// 构造带 `archived = 0` 默认过滤的 `PermissionFilter`。
    fn with_default_archived(mut filter: PermissionFilter) -> PermissionFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }

    /// 将扁平权限列表组装为树形结构（按 `parent_id` 递归）。
    fn build_tree(permissions: Vec<Permission>) -> Vec<PermissionTreeNode> {
        // 找出根节点（parent_id 为 None 或空字符串）
        let roots: Vec<Permission> = permissions
            .iter()
            .filter(|p| p.parent_id.as_ref().map(|s| s.is_empty()).unwrap_or(true))
            .cloned()
            .collect();

        // 递归构建子树
        roots
            .into_iter()
            .map(|root| Self::build_subtree(root, &permissions))
            .collect()
    }

    /// 递归构建子树。
    fn build_subtree(parent: Permission, all: &[Permission]) -> PermissionTreeNode {
        let children: Vec<PermissionTreeNode> = all
            .iter()
            .filter(|p| p.parent_id.as_deref() == Some(&parent.id))
            .cloned()
            .map(|child| Self::build_subtree(child, all))
            .collect();

        PermissionTreeNode {
            permission: parent,
            children,
        }
    }

    // ============================================================
    // 插件权限导入相关固有方法（非 trait 方法）
    // ============================================================

    /// 解压 ZIP 并解析、校验所有 JSON 文件，合并返回权限定义列表。
    ///
    /// fail-fast：任何 ZIP/JSON 解析错误或校验失败立即返回错误。
    fn parse_and_validate_permission_zip(zip_data: &[u8]) -> Result<Vec<PermissionDefinition>, TraitError> {
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

            let perm_file: PermissionFile = serde_json::from_str(&content).map_err(|e| {
                TraitError::Business(format!("文件 {name} JSON 解析失败: {e}"))
            })?;

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

    /// 事务内查询指定三元组作用域下的权限集合（code → id）。
    ///
    /// 不限定 archived，物理删除场景需感知所有历史记录。
    async fn query_permission_ids_by_scope_txn(
        &self,
        txn_id: &str,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<HashMap<String, String>, TraitError> {
        let sql = "SELECT id, code FROM cmx_permission \
                   WHERE domain_code = $1 AND app_code = $2 AND module_code = $3";
        let params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "perm_scope_ids")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询作用域权限失败: {e}"))))?;

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
    async fn query_affected_roles_txn(
        &self,
        txn_id: &str,
        permission_ids: &[String],
    ) -> Result<Vec<String>, TraitError> {
        if permission_ids.is_empty() {
            return Ok(Vec::new());
        }
        // 使用 ANY($1) 数组参数，避免 IN 列表过长
        let sql = "SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id = ANY($1)";
        let arr = DataValue::Array(
            permission_ids
                .iter()
                .map(|s| DataValue::String(s.clone()))
                .collect(),
        );
        let params = vec![arr];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "perm_affected_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询受影响角色失败: {e}"))))?;

        let schema = dataset.schema.as_ref();
        let role_ids: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "role_id"))
            .collect();
        Ok(role_ids)
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
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文（用于审计日志填充操作者信息）。
    /// * `domain_code` - 域编码（作用域三元组之一）。
    /// * `app_code` - 应用编码（作用域三元组之一）。
    /// * `module_code` - 模块编码（作用域三元组之一）。
    /// * `zip_data` - ZIP 文件二进制内容（内含一个或多个 JSON 文件）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `PluginDataImportResult`，包含新增/更新/删除计数与汇总消息。
    ///
    /// # Errors
    ///
    /// 整体 fail-fast：任何错误回滚整个事务。可能的错误包括：
    /// - ZIP 解压失败、JSON 解析失败、code 校验失败
    /// - 唯一约束冲突（code 被其他模块占用）
    /// - 事务开启/提交失败、SQL 执行失败
    #[instrument(target = "cmx_iam_import", skip(self, zip_data), fields(domain = %domain_code, app = %app_code, module = %module_code))]
    pub async fn import_permissions(
        &self,
        svr_ctx: &SVRContext,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        zip_data: &[u8],
    ) -> Result<PluginDataImportResult, TraitError> {
        // 1. 解压 + 解析 + 校验 JSON
        let definitions = Self::parse_and_validate_permission_zip(zip_data)?;

        // 安全校验：空 ZIP 或空 permissions 数组时拒绝执行，
        // 否则会导致该作用域下全部权限被误删（to_delete = db_codes - empty = db_codes 全集）
        if definitions.is_empty() {
            return Err(TraitError::from(IamError::Business(
                "ZIP 中未包含任何权限定义，拒绝执行导入（防止误删现有权限）".to_string(),
            )));
        }

        let file_codes: HashSet<String> = definitions.iter().map(|d| d.code.clone()).collect();

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
        let to_create: Vec<&PermissionDefinition> =
            definitions.iter().filter(|d| !db_codes.contains(&d.code)).collect();
        let to_update: Vec<&PermissionDefinition> =
            definitions.iter().filter(|d| db_codes.contains(&d.code)).collect();
        let to_delete: Vec<String> = db_codes.difference(&file_codes).cloned().collect();

        // 3. 第一阶段：INSERT/UPDATE（parent_id 暂置 NULL）
        let mut code_to_id: HashMap<String, String> = db_map.clone();
        let mut created_count = 0u32;

        for def in &to_create {
            let id = cmx_utils::id::snowflake_id_str();
            let sql = "INSERT INTO cmx_permission \
                       (id, code, name, resource_type, parent_id, sort_order, description, \
                        domain_code, app_code, module_code, extension, status, archived) \
                       VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, $11, 0)";
            let params = vec![
                DataValue::String(id.clone()),
                DataValue::String(def.code.clone()),
                DataValue::String(def.name.clone()),
                DataValue::String(def.resource_type.clone().unwrap_or_else(|| "api".to_string())),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::String(domain_code.to_string()),
                DataValue::String(app_code.to_string()),
                DataValue::String(module_code.to_string()),
                def.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::Int(def.status.unwrap_or(1)),
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
            let id = db_map
                .get(&def.code)
                .ok_or_else(|| TraitError::Business(format!("更新权限时找不到 id: {}", def.code)))?;
            // UPDATE 按 id 定位，parent_id 暂置 NULL（第二阶段回填）
            let sql = "UPDATE cmx_permission SET name = $1, resource_type = $2, parent_id = NULL, \
                       sort_order = $3, description = $4, extension = $5, status = $6, update_time = NOW() \
                       WHERE id = $7";
            let params = vec![
                DataValue::String(def.name.clone()),
                DataValue::String(def.resource_type.clone().unwrap_or_else(|| "api".to_string())),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                def.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::Int(def.status.unwrap_or(1)),
                DataValue::String(id.clone()),
            ];
            let rows = self.mm
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
                    let sql = "UPDATE cmx_permission SET parent_id = $1 WHERE id = $2";
                    let params = vec![
                        DataValue::String(parent_id),
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
                } else {
                    warn!(code = %def.code, parent_code = %parent_code, "parent_code 未找到，降级为无父节点");
                }
            }
        }

        // 5. 删除前查询受影响角色（用于缓存失效）
        // 同时收集被更新的权限 ID，因为更新也可能影响缓存（name/status/parent_id 变更）
        let to_delete_ids: Vec<String> = to_delete
            .iter()
            .filter_map(|c| db_map.get(c).cloned())
            .collect();
        let to_update_ids: Vec<String> = to_update
            .iter()
            .filter_map(|d| db_map.get(&d.code).cloned())
            .collect();
        let mut affected_ids = to_delete_ids.clone();
        affected_ids.extend(to_update_ids);
        let affected_roles = self.query_affected_roles_txn(txn_id, &affected_ids).await?;

        // 5.1 物理删除权限 + 物理删除角色关联（按 id 定位）
        let mut deleted_count = 0u32;
        for code in &to_delete {
            let id = db_map
                .get(code)
                .ok_or_else(|| TraitError::Business(format!("删除权限时找不到 id: {code}")))?;
            let del_perm_sql = "DELETE FROM cmx_permission WHERE id = $1";
            let del_perm_params = vec![DataValue::String(id.clone())];
            let rows = self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_perm_sql, del_perm_params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("删除权限失败 (id={id}): {e}")))
                })?;

            // 仅在实际删除行时计数和清理角色关联
            if rows > 0 {
                deleted_count += 1;
                let del_rp_sql = "DELETE FROM cmx_role_permission WHERE permission_id = $1";
                let del_rp_params = vec![DataValue::String(id.clone())];
                self.mm
                    .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_rp_sql, del_rp_params)
                    .await
                    .map_err(|e| {
                        TraitError::from(IamError::Business(format!(
                            "删除角色权限关联失败 (permission_id={id}): {e}"
                        )))
                    })?;
            }
        }

        // 6. 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 7. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "domain_code": domain_code,
            "app_code": app_code,
            "module_code": module_code,
            "created": created_count,
            "updated": updated_count,
            "deleted": deleted_count,
            "created_codes": to_create.iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
            "updated_codes": to_update.iter().map(|d| d.code.clone()).collect::<Vec<_>>(),
            "deleted_codes": to_delete,
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
        if (deleted_count > 0 || updated_count > 0)
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
            deleted = deleted_count,
            "权限导入完成"
        );

        Ok(PluginDataImportResult {
            success: true,
            message: format!(
                "导入完成: 新增 {} / 更新 {} / 删除 {}",
                created_count, updated_count, deleted_count
            ),
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
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文（用于审计日志）。
    /// * `domain_code` - 域编码。
    /// * `app_code` - 应用编码。
    /// * `module_code` - 模块编码。
    ///
    /// # Returns
    ///
    /// 成功时返回 `PluginDataImportResult`，仅 `deleted_count` 有值。
    ///
    /// # Errors
    ///
    /// 事务开启/提交失败、SQL 执行失败时返回错误并回滚事务。
    #[instrument(target = "cmx_iam_import", skip(self), fields(domain = %domain_code, app = %app_code, module = %module_code))]
    pub async fn cleanup_permissions(
        &self,
        svr_ctx: &SVRContext,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<PluginDataImportResult, TraitError> {
        // 1. 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1.1 查询受影响角色（用子查询避免依赖额外参数）
        let affected_roles_sql =
            "SELECT DISTINCT role_id FROM cmx_role_permission \
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
            .map_err(|e| TraitError::from(IamError::Business(format!("查询受影响角色失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let affected_roles: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "role_id"))
            .collect();

        // 1.2 物理删除角色关联（子查询避免 IN 列表过长）
        let del_rp_sql =
            "DELETE FROM cmx_role_permission WHERE permission_id IN (\
             SELECT id FROM cmx_permission \
             WHERE domain_code = $1 AND app_code = $2 AND module_code = $3)";
        let scope_params = vec![
            DataValue::String(domain_code.to_string()),
            DataValue::String(app_code.to_string()),
            DataValue::String(module_code.to_string()),
        ];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), del_rp_sql, scope_params.clone())
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("删除角色权限关联失败: {e}")))
            })?;

        // 1.3 物理删除权限
        let del_perm_sql =
            "DELETE FROM cmx_permission \
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

        Ok(PluginDataImportResult {
            success: true,
            message: format!("清理完成: 删除 {} 条权限", deleted_count),
            created_count: 0,
            updated_count: 0,
            deleted_count,
        })
    }
}

impl AuditHelper for PermissionServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl PermissionService for PermissionServiceImpl {
    /// 创建权限。
    ///
    /// 校验权限编码唯一性后写入数据库，并写入审计日志。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `data` - 权限创建参数。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// * `IamError::PermissionCodeExists` - 权限编码已存在。
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn create_permission(
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
            .query_sql_with_datavalues(&self.db_id, None, check_sql, check_params, "check_perm_code")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限编码失败: {e}"))))?;
        if existing.iter().next().is_some() {
            return Err(TraitError::from(IamError::PermissionCodeExists(data.code.clone())));
        }

        let dataset =
            GenericCrudService::<PermissionBmc>::create(&self.mm, &self.db_id, None, data.clone())
                .await
                .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permission = Self::extract_permission(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "code": &data.code,
            "name": &data.name,
        });
        self.audit_write(svr_ctx, "create_permission", "permission", &permission.id, &audit_detail)
            .await;

        info!(permission_id = %permission.id, code = %data.code, "权限创建成功");
        Ok(permission)
    }

    /// 获取单个权限。
    ///
    /// # Arguments
    ///
    /// * `permission_id` - 权限唯一标识。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// * `IamError::PermissionNotFound` - 权限不存在。
    /// * `IamError::Crud` - 数据库查询失败。
    async fn get_permission(&self, permission_id: &str) -> Result<Permission, TraitError> {
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
            return Err(TraitError::from(IamError::PermissionNotFound(permission_id.to_string())));
        }

        Self::extract_permission(dataset).map_err(TraitError::from)
    }

    /// 更新权限。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_id` - 目标权限 ID。
    /// * `data` - 更新参数（全 `Option`，未提供字段不更新）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `Permission` 实例。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库 CRUD 操作失败。
    async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::update_permission - {}",
            "IAM", permission_id
        );

        let dataset = GenericCrudService::<PermissionBmc>::update(
            &self.mm,
            &self.db_id,
            None,
            Value::String(permission_id.to_string()),
            data.clone(),
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permission = Self::extract_permission(dataset).map_err(TraitError::from)?;

        // 审计日志
        let audit_detail = serde_json::json!({
            "name": &data.name,
            "description": &data.description,
        });
        self.audit_write(svr_ctx, "update_permission", "permission", permission_id, &audit_detail)
            .await;

        info!(permission_id = permission_id, "权限更新成功");
        Ok(permission)
    }

    /// 批量删除权限（事务保证软删除 + 角色关联清理的原子性）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_ids` - 待删除的权限 ID 列表；空数组直接返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::delete_permission - count: {}",
            "IAM",
            permission_ids.len()
        );

        if permission_ids.is_empty() {
            return Ok(());
        }

        // 使用事务保证软删除+物理删除的原子性
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 1. 软删除 cmx_permission
        for permission_id in permission_ids {
            let sql = "UPDATE cmx_permission SET archived = 1, update_time = NOW() WHERE id = $1";
            let params = vec![DataValue::String(permission_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("软删除权限失败: {e}"))))?;
        }

        // 2. 物理删除 cmx_role_permission 关联
        for permission_id in permission_ids {
            let sql = "DELETE FROM cmx_role_permission WHERE permission_id = $1";
            let params = vec![DataValue::String(permission_id.clone())];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params)
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("删除权限角色关联失败: {e}"))))?;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 3. 审计日志（事务提交后）
        let audit_detail = serde_json::json!({
            "permission_ids": permission_ids,
            "count": permission_ids.len(),
        });
        self.audit_write(svr_ctx, "delete_permission", "permission", "batch", &audit_detail)
            .await;

        info!(count = permission_ids.len(), "权限删除成功");
        Ok(())
    }

    /// 分页查询权限。
    ///
    /// 默认附加 `archived = 0` 过滤；`current` 从 1 开始。
    ///
    /// # Arguments
    ///
    /// * `filter` - 权限查询过滤器。
    /// * `current` - 当前页码（从 1 开始）。
    /// * `size` - 每页记录数。
    ///
    /// # Returns
    ///
    /// 元组 `(权限列表, 总记录数)`。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库分页查询失败。
    async fn page_permissions(
        &self,
        filter: PermissionFilter,
        current: u64,
        size: u64,
    ) -> Result<(Vec<Permission>, i64), TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::page_permissions - current: {}, size: {}",
            "IAM", current, size
        );

        let filters = Self::with_default_archived(filter);
        let offset = current.saturating_sub(1) * size;
        let list_options = ListOptions::from_offset_limit(offset as i64, size as i64);

        let (dataset, total) =
            GenericCrudService::<PermissionBmc, PermissionFilter>::page(
                &self.mm,
                &self.db_id,
                None,
                Some(vec![filters]),
                list_options,
            )
            .await
            .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permissions = Self::extract_permissions(dataset);
        Ok((permissions, total))
    }

    /// 列表查询权限。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    ///
    /// # Arguments
    ///
    /// * `filter` - 权限查询过滤器。
    ///
    /// # Returns
    ///
    /// 匹配的权限列表。
    ///
    /// # Errors
    ///
    /// * `IamError::Crud` - 数据库查询失败。
    async fn list_permissions(
        &self,
        filter: PermissionFilter,
    ) -> Result<Vec<Permission>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::list_permissions", "IAM");

        let filters = Self::with_default_archived(filter);

        let dataset = GenericCrudService::<PermissionBmc, PermissionFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            Some(vec![filters]),
            None,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_permissions(dataset))
    }

    /// 获取权限树（递归结构，支持按域/应用/模块过滤）。
    ///
    /// 一次性加载所有有效权限（`archived = 0 AND status = 1`），
    /// 在内存中按 `parent_id` 递归构建树形结构。
    /// 当指定 `domain_code`/`app_code`/`module_code` 时，通过参数化 SQL WHERE 子句过滤。
    ///
    /// # Returns
    ///
    /// 树根列表（每个根节点包含嵌套的 `children`）。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - SQL 查询失败。
    async fn get_permission_tree(
        &self,
        domain_code: Option<&str>,
        app_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<PermissionTreeNode>, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::get_permission_tree - domain: {:?}, app: {:?}, module: {:?}",
            "IAM", domain_code, app_code, module_code
        );

        // 动态构建带过滤条件的 SQL
        let mut sql = String::from(
            "SELECT id, code, name, resource_type, parent_id, sort_order, status, description, \
             domain_code, app_code, module_code, extension \
             FROM cmx_permission WHERE archived = 0 AND status = 1",
        );
        let mut params: Vec<DataValue> = Vec::new();
        let mut param_idx = 1;

        if let Some(dc) = domain_code {
            sql.push_str(&format!(" AND domain_code = ${param_idx}"));
            params.push(DataValue::String(dc.to_string()));
            param_idx += 1;
        }
        if let Some(ac) = app_code {
            sql.push_str(&format!(" AND app_code = ${param_idx}"));
            params.push(DataValue::String(ac.to_string()));
            param_idx += 1;
        }
        if let Some(mc) = module_code {
            sql.push_str(&format!(" AND module_code = ${param_idx}"));
            params.push(DataValue::String(mc.to_string()));
        }
        sql.push_str(" ORDER BY sort_order ASC NULLS LAST, code ASC");

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, &sql, params, "permission_tree")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限树失败: {e}"))))?;

        let permissions = Self::extract_permissions(dataset);

        Ok(Self::build_tree(permissions))
    }

    /// 统计每个权限被多少角色使用。
    ///
    /// # Returns
    ///
    /// 成功时返回 `PermissionUsageStat` 列表，按 `role_count` 降序排列。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `IamError::Business`。
    async fn get_permission_usage_stat(
        &self,
    ) -> Result<Vec<crate::service_traits::PermissionUsageStat>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::get_permission_usage_stat", "IAM");

        let sql = r#"
            SELECT p.id, p.code, p.name,
                   COUNT(DISTINCT rp.role_id) AS role_count,
                   COUNT(DISTINCT ur.user_id) AS user_count,
                   MAX(rp.create_time) AS last_assigned_at
            FROM cmx_permission p
            LEFT JOIN cmx_role_permission rp ON rp.permission_id = p.id AND rp.archived = 0
            LEFT JOIN cmx_user_role ur ON ur.role_id = rp.role_id AND ur.archived = 0
            WHERE p.archived = 0 AND p.status = 1
            GROUP BY p.id, p.code, p.name
            ORDER BY role_count DESC, p.sort_order, p.code
        "#;
        let params: Vec<DataValue> = vec![];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "perm_usage_stat")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询权限使用统计失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let stats: Vec<crate::service_traits::PermissionUsageStat> = dataset
            .iter()
            .filter_map(|row| {
                Some(crate::service_traits::PermissionUsageStat {
                    permission_id: row.get_by_name_as(schema, "id")?,
                    permission_code: row.get_by_name_as(schema, "code")?,
                    permission_name: row.get_by_name_as(schema, "name")?,
                    role_count: row
                        .get_by_name_as::<i64>(schema, "role_count")
                        .unwrap_or(0) as u32,
                    user_count: row
                        .get_by_name_as::<i64>(schema, "user_count")
                        .unwrap_or(0) as u32,
                    last_assigned_at: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "last_assigned_at"),
                })
            })
            .collect();

        Ok(stats)
    }
}

// ============================================================
// 插件权限文件解析相关结构体
// ============================================================

/// 权限定义（对应 JSON 文件中的单条权限条目）。
///
/// 用于从插件 `permdata/*.json` 文件反序列化，与入库的 `Permission` 实体解耦。
/// `parent_code` 在第二阶段被解析为 `parent_id`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDefinition {
    /// 权限编码（必须含 `:` 分隔符，如 `user:list`）。
    pub code: String,
    /// 权限名称。
    pub name: String,
    /// 资源类型（`api`/`menu`/`button`，未指定默认 `api`）。
    #[serde(default)]
    pub resource_type: Option<String>,
    /// 父权限编码（用 code 引用，接收端解析为 parent_id）。
    #[serde(default)]
    pub parent_code: Option<String>,
    /// 排序序号（默认 0）。
    #[serde(default)]
    pub sort_order: Option<i64>,
    /// 权限描述。
    #[serde(default)]
    pub description: Option<String>,
    /// 扩展配置（JSON 字符串）。
    #[serde(default)]
    pub extension: Option<String>,
    /// 状态（1-启用，0-禁用，默认 1）。
    #[serde(default)]
    pub status: Option<i64>,
}

/// 权限文件（对应 `permdata/` 目录下的单个 JSON 文件）。
///
/// `name`/`version`/`description` 为元数据，不入库；
/// `permissions` 为实际权限定义列表，合并后统一处理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionFile {
    /// 文件描述名称（元数据，不入库）。
    #[serde(default)]
    #[allow(dead_code)]
    pub name: String,
    /// 文件版本（元数据，不入库）。
    #[serde(default)]
    #[allow(dead_code)]
    pub version: String,
    /// 文件描述（元数据，不入库）。
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    /// 权限定义列表。
    pub permissions: Vec<PermissionDefinition>,
}
