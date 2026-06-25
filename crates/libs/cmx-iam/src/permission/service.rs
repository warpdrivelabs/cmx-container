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
use crate::permission::{
    PermissionBmc, PermissionFilter, PermissionForCreate, PermissionForUpdate,
};
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
            .filter(|p| p.parent_id.as_deref() == Some(parent.id.as_str()))
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
        // 使用 IN + 动态占位符（驱动不支持 ANY($1) 数组绑定）
        let placeholders: Vec<String> = (1..=permission_ids.len())
            .map(|i| format!("${i}"))
            .collect();
        let in_clause = placeholders.join(",");
        let sql = format!(
            "SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id IN ({in_clause})"
        );
        let params: Vec<DataValue> = permission_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), &sql, params, "perm_affected_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询受影响角色失败: {e}"))))?;

        let schema = dataset.schema.as_ref();
        let role_ids: Vec<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "role_id"))
            .collect();
        Ok(role_ids)
    }

    /// 事务内查询父节点的 code/full_code_path/level（用于计算子节点路径字段）。
    async fn query_parent_meta_txn(
        &self,
        txn_id: &str,
        parent_id: &str,
    ) -> Result<Option<(String, String, i64)>, TraitError> {
        let sql = "SELECT code, full_code_path, level FROM cmx_permission WHERE id = $1";
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
    async fn collect_descendants_by_path_txn(
        &self,
        txn_id: &str,
        root_path: &str,
    ) -> Result<Vec<String>, TraitError> {
        let sql = "SELECT id FROM cmx_permission WHERE full_code_path = $1 OR full_code_path LIKE ($2 || '/%')";
        let params = vec![
            DataValue::String(root_path.to_string()),
            DataValue::String(root_path.to_string()),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), sql, params, "descendants_by_path")
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
    async fn check_usage_by_roles_txn(
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
             JOIN cmx_role_permission rp ON rp.permission_id = p.id \
             JOIN cmx_role r ON r.id = rp.role_id AND r.archived = 0 \
             WHERE p.id IN ({in_clause})"
        );
        let params: Vec<DataValue> = permission_ids
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), &sql, params, "check_perm_usage")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限使用情况失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let mut map: HashMap<String, BlockedPermissionInfo> = HashMap::new();
        for row in dataset.iter() {
            let pid = match row.get_by_name_as::<String>(schema, "pid") {
                Some(v) => v,
                None => continue,
            };
            let pcode = row.get_by_name_as::<String>(schema, "pcode").unwrap_or_default();
            let pname = row.get_by_name_as::<String>(schema, "pname").unwrap_or_default();
            let rid = match row.get_by_name_as::<String>(schema, "rid") {
                Some(v) => v,
                None => continue,
            };
            let rcode = row.get_by_name_as::<String>(schema, "rcode").unwrap_or_default();
            let rname = row.get_by_name_as::<String>(schema, "rname").unwrap_or_default();
            let entry = map.entry(pid.clone()).or_insert_with(|| BlockedPermissionInfo {
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
    async fn recompute_parent_is_leaf(&self, parent_id: &str) {
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
        let to_create: Vec<&PermissionDefinition> =
            definitions.iter().filter(|d| !db_codes.contains(&d.code)).collect();
        let to_update: Vec<&PermissionDefinition> =
            definitions.iter().filter(|d| db_codes.contains(&d.code)).collect();
        // let to_delete: Vec<String> = db_codes.difference(&file_codes).cloned().collect();

        // 3. 第一阶段：INSERT/UPDATE（parent_id 暂置 NULL）
        let mut code_to_id: HashMap<String, String> = db_map.clone();
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
                DataValue::String(def.resource_type.clone().unwrap_or_else(|| "api".to_string())),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::String(domain_code.to_string()),
                DataValue::String(app_code.to_string()),
                DataValue::String(module_code.to_string()),
                def.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
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
            let id = db_map
                .get(&def.code)
                .ok_or_else(|| TraitError::Business(format!("更新权限时找不到 id: {}", def.code)))?;
            // UPDATE 按 id 定位，parent_id 暂置 NULL；路径字段重置为根节点（第二阶段回填覆盖）
            let sql = "UPDATE cmx_permission SET name = $1, resource_type = $2, parent_id = NULL, \
                       parent_code = NULL, full_code_path = '/' || code, level = 1, is_leaf = 1, \
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
        let  affected_ids = to_update_ids.clone();
        // affected_ids.extend(to_update_ids);
        let affected_roles = self.query_affected_roles_txn(txn_id, &affected_ids).await?;

        let  deleted_count = 0u32;

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
        if  updated_count > 0
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

        Ok(PluginDataImportResult {
            success: true,
            // message: format!(
            //     "导入完成: 新增 {} / 更新 {} / 删除 {}",
            //     created_count, updated_count, deleted_count
            // ),
            message: format!(
                "导入完成: 新增 {} / 更新 {} ",
                created_count, updated_count
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
                .ok_or_else(|| TraitError::from(IamError::Business(format!("父权限不存在: {pid}"))))?;
            let (p_code, p_path, p_level) = meta;
            (Some(p_code), format!("{}/{}", p_path, data.code), p_level + 1)
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
            data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.parent_id.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Int(0)),
            data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.domain_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.app_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.module_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            data.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
            DataValue::Int(1),
            parent_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
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
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), upd_sql, vec![DataValue::String(pid.clone())])
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

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("开启事务失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 查询当前权限的 parent_id / full_code_path / level / code
        let meta_sql = "SELECT parent_id, full_code_path, level, code FROM cmx_permission WHERE id = $1";
        let meta_params = vec![DataValue::String(permission_id.to_string())];
        let meta_dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), meta_sql, meta_params, "update_perm_meta")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限元数据失败: {e}"))))?;
        let schema = meta_dataset.schema.as_ref();
        let row = meta_dataset.iter().next().ok_or_else(|| {
            TraitError::from(IamError::PermissionNotFound(permission_id.to_string()))
        })?;
        let old_parent_id = row.get_by_name_as::<String>(schema, "parent_id");
        let old_path = row.get_by_name_as::<String>(schema, "full_code_path").unwrap_or_default();
        let old_level = row.get_by_name_as::<i64>(schema, "level").unwrap_or(1);
        let perm_code = row.get_by_name_as::<String>(schema, "code").unwrap_or_default();

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
                (Some(p_code), format!("{}/{}", p_path, perm_code), p_level + 1)
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
                .map_err(|e| TraitError::from(IamError::Business(format!("级联更新路径失败: {e}"))))?;

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
                new_parent_norm.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null),
                new_parent_code.map(DataValue::String).unwrap_or(DataValue::Null),
                data.name.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Null),
                data.status.map(DataValue::Int).unwrap_or(DataValue::Null),
                data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.domain_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.app_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.module_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::String(permission_id.to_string()),
            ];
            let ds = self
                .mm
                .query_sql_with_datavalues(&self.db_id, Some(txn_id), upd_sql, params, "update_perm")
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
                data.name.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.resource_type.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.sort_order.map(DataValue::Int).unwrap_or(DataValue::Null),
                data.status.map(DataValue::Int).unwrap_or(DataValue::Null),
                data.description.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.domain_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.app_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.module_code.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                data.extension.clone().map(DataValue::String).unwrap_or(DataValue::Null),
                DataValue::String(permission_id.to_string()),
            ];
            self.mm
                .query_sql_with_datavalues(&self.db_id, Some(txn_id), upd_sql, params, "update_perm")
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("更新权限失败: {e}"))))?
        };

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 旧父重算 is_leaf（提交后，因为 recompute 使用 None txn_id）
        if parent_changed
            && let Some(ref old_pid) = old_parent_for_recompute
        {
            self.recompute_parent_is_leaf(old_pid).await;
        }

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

    /// 批量删除权限（含级联删除子树 + 角色使用检查）。
    ///
    /// 流程：
    /// 1. 事务内查询每个根权限的 `full_code_path` 和 `parent_id`
    /// 2. 按 `full_code_path` LIKE 收集自身及所有后代 ID（去重）
    /// 3. 检查是否有角色正在使用这些权限；若有则返回 `Blocked`
    /// 4. 物理删除角色关联 + 物理删除权限，提交事务
    /// 5. 提交后重算旧父 `is_leaf`、失效缓存、写审计
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `permission_ids` - 待删除的根权限 ID 列表；空数组返回空的 `Deleted`。
    ///
    /// # Errors
    ///
    /// * `IamError::Business` - 事务开启/提交失败，或 SQL 执行失败。
    async fn delete_permission(
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
            .query_sql_with_datavalues(&self.db_id, Some(txn_id), &meta_sql, meta_params, "delete_perm_meta")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询删除权限元数据失败: {e}"))))?;
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
        let del_placeholders: Vec<String> = (1..=all_ids_vec.len())
            .map(|i| format!("${i}"))
            .collect();
        let del_in_clause = del_placeholders.join(",");
        let del_rp_sql = format!(
            "DELETE FROM cmx_role_permission WHERE permission_id IN ({del_in_clause})"
        );
        let del_rp_params: Vec<DataValue> = all_ids_vec
            .iter()
            .map(|s| DataValue::String(s.clone()))
            .collect();
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), &del_rp_sql, del_rp_params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除角色权限关联失败: {e}"))))?;

        // 6. 物理删除权限
        let del_perm_sql = format!(
            "DELETE FROM cmx_permission WHERE id IN ({del_in_clause})"
        );
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
        self.audit_write(svr_ctx, "delete_permission", "permission", "batch", &audit_detail)
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
        list_options: ListOptions,
    ) -> Result<(Vec<Permission>, i64), TraitError> {
        debug!("{:<12} - PermissionServiceImpl::page_permissions", "IAM");

        let filters = Self::with_default_archived(filter);

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
        list_options: Option<ListOptions>,
    ) -> Result<Vec<Permission>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::list_permissions", "IAM");

        let filters = Self::with_default_archived(filter);

        let dataset = GenericCrudService::<PermissionBmc, PermissionFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            Some(vec![filters]),
            list_options,
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
