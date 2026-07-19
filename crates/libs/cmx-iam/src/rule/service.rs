//! 互斥规则 Service 实现
//!
//! 提供互斥规则的 CRUD、启用/禁用、规则项管理、校验测试等功能。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::ParamsBuilder;
use cmx_core::SVRContext;
use cmx_core::model::cell::DataValue;
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use cmx_traits::error::TraitError;
use cmx_utils::snowflake_id_str;
use modql::filter::{ListOptions, OpValInt64, OpValString, OpValsInt64, OpValsString};
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::rule::entity::{
    CreateExclusionRuleRequest, ExclusionRule, ExclusionRuleItem, RuleViolationDetail,
    UpdateExclusionRuleRequest, ValidateRuleRequest, ValidateRuleResponse,
};
use crate::rule::{ExclusionRuleBmc, ExclusionRuleFilter};

/// 互斥规则 Service trait。
///
/// 定义互斥规则的 CRUD、启用/禁用、规则项管理、校验测试等操作。
/// 实现见 `ExclusionRuleServiceImpl`。
#[async_trait]
pub trait ExclusionRuleService: Send + Sync {
    /// 创建规则（含规则项）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `req` - 创建规则请求，包含规则主信息与互斥对象列表。
    ///
    /// # Returns
    ///
    /// 成功时返回创建后的 `ExclusionRule` 实例。
    ///
    /// # Errors
    ///
    /// 当 `subject_type` 非法、互斥对象列表为空、主要对象与互斥对象重复、
    /// 对象不存在或事务提交失败时返回错误。
    async fn create_rule(
        &self,
        svr_ctx: &SVRContext,
        req: CreateExclusionRuleRequest,
    ) -> Result<ExclusionRule, TraitError>;

    /// 更新规则基本信息。
    ///
    /// 不允许修改 `subject_type`；若修改 `primary_subject_id`，需校验其不在现有互斥列表中。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `rule_id` - 待更新的规则 ID。
    /// * `data` - 更新参数（全 `Option`，未提供字段不更新）。
    ///
    /// # Returns
    ///
    /// 成功时返回更新后的 `ExclusionRule` 实例。
    ///
    /// # Errors
    ///
    /// 当未提供任何更新字段、规则不存在、新主对象已存在于互斥列表或 SQL 执行失败时返回错误。
    async fn update_rule(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        data: UpdateExclusionRuleRequest,
    ) -> Result<ExclusionRule, TraitError>;

    /// 删除规则（软删除 `archived = 1`）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `rule_id` - 待删除的规则 ID。
    ///
    /// # Errors
    ///
    /// 当规则不存在或 SQL 执行失败时返回错误。
    async fn delete_rule(&self, svr_ctx: &SVRContext, rule_id: &str) -> Result<(), TraitError>;

    /// 查询规则详情（含规则项）。
    ///
    /// # Arguments
    ///
    /// * `rule_id` - 规则 ID。
    ///
    /// # Returns
    ///
    /// 元组 `(规则主记录, 规则项列表)`。
    ///
    /// # Errors
    ///
    /// 当规则不存在或 SQL 查询失败时返回错误。
    async fn get_rule(
        &self,
        rule_id: &str,
    ) -> Result<(ExclusionRule, Vec<ExclusionRuleItem>), TraitError>;

    /// 分页查询规则（支持过滤 + 跨表 subject_id）。
    ///
    /// 默认附加 `archived = 0` 过滤；无 `subject_id` 跨表条件时走
    /// `GenericCrudService`（单表索引），含 `subject_id` 时走 raw SQL
    /// （JOIN items + DISTINCT + 走 `subject_id` 索引）。
    ///
    /// # Arguments
    ///
    /// * `filters` - 规则查询过滤器数组（每个 filter 为一个 AND 组，组间 OR）。
    ///   `subject_id` 为跨表字段，触发 raw SQL 分支。
    /// * `list_options` - 分页与排序选项。
    ///
    /// # Returns
    ///
    /// 元组 `(规则列表, 总记录数)`，按 `priority` 降序、`create_time` 降序排列。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询或反序列化失败时返回错误。
    async fn page_rules(
        &self,
        filters: Option<Vec<crate::rule::ExclusionRuleFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<ExclusionRule>, i64), TraitError>;

    /// 切换规则状态（启用/禁用）。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `rule_id` - 目标规则 ID。
    /// * `status` - 新状态（1 启用 / 0 禁用）。
    ///
    /// # Errors
    ///
    /// 当规则不存在、已归档或 SQL 执行失败时返回错误。
    async fn toggle_rule_status(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        status: i64,
    ) -> Result<(), TraitError>;

    /// 添加规则项。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `rule_id` - 目标规则 ID。
    /// * `subject_ids` - 待添加的互斥对象 ID 列表。
    ///
    /// # Returns
    ///
    /// 实际新增的记录数（已存在的会被 `ON CONFLICT DO NOTHING` 跳过）。
    ///
    /// # Errors
    ///
    /// 当规则不存在、新增对象与主要对象相同、对象不存在或 SQL 执行失败时返回错误。
    async fn add_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        subject_ids: Vec<String>,
    ) -> Result<u64, TraitError>;

    /// 移除规则项。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于审计日志填充操作者信息。
    /// * `rule_id` - 目标规则 ID。
    /// * `item_ids` - 待移除的规则项 ID 列表。
    ///
    /// # Returns
    ///
    /// 实际删除的记录数。
    ///
    /// # Errors
    ///
    /// 当 SQL 执行失败时返回错误。
    async fn remove_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        item_ids: &[String],
    ) -> Result<u64, TraitError>;

    /// 规则校验测试（给定权限/角色组合，测试是否违反互斥规则）。
    ///
    /// # Arguments
    ///
    /// * `req` - 校验请求，包含待测权限/角色组合及可选的用户 ID（用于合并现有权限/角色）。
    ///
    /// # Returns
    ///
    /// 返回 `ValidateRuleResponse`，包含是否通过及违反详情列表。
    ///
    /// # Errors
    ///
    /// 当 SQL 查询失败时返回错误。
    async fn validate_rule(
        &self,
        req: ValidateRuleRequest,
    ) -> Result<ValidateRuleResponse, TraitError>;
}

/// 互斥规则 Service 实现。
///
/// 持有 `Arc<DatabaseManager>` 与 `db_id`，通过参数化 SQL 实现互斥规则的 CRUD 与校验。
pub struct ExclusionRuleServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,
    /// 认证库 `db_id`。
    db_id: String,
    /// IAM 配置（预留）。
    #[allow(dead_code)]
    config: IamConfig,
    /// 审计日志记录器（可选）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
}

impl ExclusionRuleServiceImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id`。
    ///
    /// # Returns
    ///
    /// 返回未注入审计记录器的新 `ExclusionRuleServiceImpl` 实例。
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
    /// 返回注入了审计记录器的新实例。
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 从 DataSet 提取 ExclusionRule（单条）
    fn extract_rule(dataset: cmx_core::model::data::dataset::DataSet) -> Option<ExclusionRule> {
        let schema = dataset.schema.as_ref();
        let row = dataset.iter().next()?;

        Some(ExclusionRule {
            id: row.get_by_name_as(schema, "id")?,
            code: row.get_by_name_as(schema, "code")?,
            name: row.get_by_name_as(schema, "name")?,
            subject_type: row.get_by_name_as(schema, "subject_type")?,
            primary_subject_id: row.get_by_name_as(schema, "primary_subject_id")?,
            violation_message: row.get_by_name_as(schema, "violation_message"),
            priority: row.get_by_name_as::<i64>(schema, "priority").unwrap_or(0),
            description: row.get_by_name_as(schema, "description"),
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
            archived: row.get_by_name_as::<i64>(schema, "archived").unwrap_or(0),
            create_time: row
                .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "create_time")
                .unwrap_or_else(chrono::Utc::now),
            update_time: row
                .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "update_time")
                .unwrap_or_else(chrono::Utc::now),
            create_by: row.get_by_name_as(schema, "create_by"),
            create_name: row.get_by_name_as(schema, "create_name"),
            update_by: row.get_by_name_as(schema, "update_by"),
            update_name: row.get_by_name_as(schema, "update_name"),
        })
    }

    /// 从 DataSet 提取 ExclusionRuleItem 列表
    fn extract_items(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<ExclusionRuleItem> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                Some(ExclusionRuleItem {
                    id: row.get_by_name_as(schema, "id")?,
                    rule_id: row.get_by_name_as(schema, "rule_id")?,
                    subject_id: row.get_by_name_as(schema, "subject_id")?,
                })
            })
            .collect()
    }

    /// 从 DataSet 提取 `ExclusionRule` 列表（`to_json_value` + serde 反序列化模式）。
    ///
    /// 与 `role::extract_roles` / `role_group::extract_role_groups` 一致，
    /// 依赖 `SELECT` 列名与 `ExclusionRule` 字段名对齐。
    fn extract_rules(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<ExclusionRule> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<ExclusionRule>(json_val).ok()
            })
            .collect()
    }

    /// 构造带 `archived = 0` 默认过滤的 `ExclusionRuleFilter`。
    fn with_default_archived(mut filter: ExclusionRuleFilter) -> ExclusionRuleFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }

    /// 从 `OpValsString` 中提取首个 `Eq` / `Ilike` / `Contains` 等操作符的字面量值。
    ///
    /// 用于跨表 raw SQL 分支：前端通常传 `{"Eq": "xxx"}`，取首个操作符的值即可。
    /// 复杂操作符（`In` 等）返回 `None`（raw SQL 路径仅支持等值/模糊）。
    fn first_string_opval(opvals: &OpValsString) -> Option<String> {
        opvals.0.first().and_then(|op| match op {
            OpValString::Eq(s)
            | OpValString::Ilike(s)
            | OpValString::Contains(s)
            | OpValString::ContainsCi(s)
            | OpValString::StartsWith(s)
            | OpValString::EndsWith(s) => Some(s.clone()),
            _ => None,
        })
    }

    /// 从 `OpValsInt64` 中提取首个 `Eq` 的数值。
    fn first_int_opval(opvals: &OpValsInt64) -> Option<i64> {
        opvals.0.first().and_then(|op| match op {
            OpValInt64::Eq(n) => Some(*n),
            _ => None,
        })
    }

    /// 查询单条规则
    async fn query_rule(&self, rule_id: &str) -> Result<ExclusionRule, IamError> {
        let sql = r#"
            SELECT id, code, name, subject_type, primary_subject_id, violation_message,
                   priority, description, status, archived, create_time, update_time,
                   create_by, create_name, update_by, update_name
            FROM cmx_exclusion_rule
            WHERE id = $1 AND archived = 0
        "#;
        let params = vec![DataValue::String(rule_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "query_rule")
            .await
            .map_err(|e| IamError::Business(format!("查询规则失败: {e}")))?;

        Self::extract_rule(dataset)
            .ok_or_else(|| IamError::Business(format!("规则不存在: {rule_id}")))
    }

    /// 查询规则项
    async fn query_rule_items(&self, rule_id: &str) -> Result<Vec<ExclusionRuleItem>, IamError> {
        let sql = r#"
            SELECT id, rule_id, subject_id
            FROM cmx_exclusion_rule_item
            WHERE rule_id = $1
        "#;
        let params = vec![DataValue::String(rule_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "query_rule_items")
            .await
            .map_err(|e| IamError::Business(format!("查询规则项失败: {e}")))?;

        Ok(Self::extract_items(dataset))
    }

    /// 校验对象（权限/角色）在对应表中存在
    async fn check_subjects_exist(
        &self,
        subject_type: &str,
        ids: &[String],
    ) -> Result<(), IamError> {
        let table = match subject_type {
            "permission" => "cmx_permission",
            "role" => "cmx_role",
            _ => {
                return Err(IamError::Business(format!(
                    "无效的对象类型: {subject_type}（应为 permission 或 role）"
                )));
            }
        };
        let sql = format!(
            "SELECT id FROM {} WHERE id = ANY($1) AND archived = 0",
            table
        );
        let params = vec![DataValue::Array(
            ids.iter().map(|i| DataValue::String(i.clone())).collect(),
        )];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, &sql, params, "check_subjects_exist")
            .await
            .map_err(|e| IamError::Business(format!("校验对象存在性失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let found: HashSet<String> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as(schema, "id"))
            .collect();

        for id in ids {
            if !found.contains(id) {
                return Err(IamError::Business(format!("{table} 中对象不存在: {id}")));
            }
        }
        Ok(())
    }

    /// 跨表分页查询：主体作为主主体或排除项参与的规则。
    ///
    /// 当 filters 含 `subject_id` 时由 `page_rules` 调用。动态拼 WHERE：
    /// `r.archived = 0` + 其他单表条件（subject_type/status/code/name）+
    /// `(r.primary_subject_id = $N OR i.subject_id = $N)`，LEFT JOIN items 表，
    /// DISTINCT 去重，按 `priority DESC, create_time DESC` 分页。
    ///
    /// §十合规：所有 raw SQL 参数用 `cmx_core::dv!` 宏构造。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `db_id` - 认证库 ID。
    /// * `filters` - 过滤器数组（提取其中 subject_type/status/code/name 等单表条件）。
    /// * `list_options` - 分页与排序选项。
    /// * `subject_id` - 跨表匹配的主体 ID。
    ///
    /// # Returns
    ///
    /// 元组 `(规则列表, 总记录数)`。
    async fn page_rules_cross_table(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<ExclusionRuleFilter>>,
        list_options: ListOptions,
        subject_id: String,
    ) -> Result<(Vec<ExclusionRule>, i64), TraitError> {
        // 动态拼 WHERE 条件与参数（占位符从 $1 起）
        let mut conds: Vec<String> = vec!["r.archived = 0".to_string()];
        let mut pvals: Vec<DataValue> = Vec::new();

        // 从首个 filter 组提取单表条件（跨表 filter 通常只含一个 AND 组）
        if let Some(fs) = filters.as_ref() {
            for f in fs {
                if let Some(v) = f.subject_type.as_ref().and_then(Self::first_string_opval) {
                    let idx = pvals.len() + 1;
                    conds.push(format!("r.subject_type = ${idx}"));
                    pvals.extend(cmx_core::dv![v]);
                }
                if let Some(v) = f.status.as_ref().and_then(Self::first_int_opval) {
                    let idx = pvals.len() + 1;
                    conds.push(format!("r.status = ${idx}"));
                    pvals.extend(cmx_core::dv![v]);
                }
                if let Some(v) = f.code.as_ref().and_then(Self::first_string_opval) {
                    let idx = pvals.len() + 1;
                    conds.push(format!("r.code ILIKE ${idx}"));
                    pvals.extend(cmx_core::dv![format!("%{v}%")]);
                }
                if let Some(v) = f.name.as_ref().and_then(Self::first_string_opval) {
                    let idx = pvals.len() + 1;
                    conds.push(format!("r.name ILIKE ${idx}"));
                    pvals.extend(cmx_core::dv![format!("%{v}%")]);
                }
            }
        }

        // 跨表条件：(r.primary_subject_id = $N OR i.subject_id = $N)
        let join = " LEFT JOIN cmx_exclusion_rule_item i ON i.rule_id = r.id AND i.archived = 0 ";
        let subj_idx = pvals.len() + 1;
        conds.push(format!(
            "(r.primary_subject_id = ${subj_idx} OR i.subject_id = ${subj_idx})"
        ));
        pvals.extend(cmx_core::dv![subject_id]);

        let where_sql = conds.join(" AND ");

        // count 查询（DISTINCT 去重后计数）
        let count_sql = format!(
            "SELECT COUNT(DISTINCT r.id) AS cnt FROM cmx_exclusion_rule r {join} WHERE {where_sql}"
        );
        let count_ds = mm
            .query_sql_with_datavalues(db_id, None, &count_sql, pvals.clone(), "rule_count_x")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("规则计数失败: {e}"))))?;
        let schema = count_ds.schema.as_ref();
        let total: i64 = count_ds
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<i64>(schema, "cnt"))
            .unwrap_or(0);

        // page 查询：追加 limit/offset 参数
        let limit = list_options.limit.unwrap_or(10);
        let offset = list_options.offset.unwrap_or(0);
        let limit_idx = subj_idx + 1;
        let offset_idx = limit_idx + 1;

        let page_sql = format!(
            r#"SELECT DISTINCT r.id, r.code, r.name, r.subject_type, r.primary_subject_id,
                      r.violation_message, r.priority, r.description, r.status, r.archived,
                      r.create_time, r.update_time,
                      r.create_by, r.create_name, r.update_by, r.update_name
               FROM cmx_exclusion_rule r {join}
               WHERE {where_sql}
               ORDER BY r.priority DESC, r.create_time DESC
               LIMIT ${limit_idx} OFFSET ${offset_idx}"#
        );
        let mut page_pvals = pvals;
        page_pvals.extend(cmx_core::dv![limit, offset]);
        let ds = mm
            .query_sql_with_datavalues(db_id, None, &page_sql, page_pvals, "rule_page_x")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("规则分页失败: {e}"))))?;

        Ok((Self::extract_rules(ds), total))
    }
}

impl AuditHelper for ExclusionRuleServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl ExclusionRuleService for ExclusionRuleServiceImpl {
    async fn create_rule(
        &self,
        svr_ctx: &SVRContext,
        req: CreateExclusionRuleRequest,
    ) -> Result<ExclusionRule, TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::create_rule - code: {}, subject_type: {}",
            "IAM-RULE", req.code, req.subject_type
        );

        // 1. 校验 subject_type 合法性
        if req.subject_type != "permission" && req.subject_type != "role" {
            return Err(TraitError::from(IamError::Business(format!(
                "无效的对象类型: {}（应为 permission 或 role）",
                req.subject_type
            ))));
        }

        // 2. 校验 excluded_subject_ids 非空
        if req.excluded_subject_ids.is_empty() {
            return Err(TraitError::from(IamError::Business(
                "互斥对象列表不能为空".to_string(),
            )));
        }

        // 3. 校验 primary_subject_id 不在 excluded_subject_ids 中
        if req
            .excluded_subject_ids
            .iter()
            .any(|id| id == &req.primary_subject_id)
        {
            return Err(TraitError::from(IamError::Business(
                "主要对象不能出现在互斥对象列表中".to_string(),
            )));
        }

        // 4. 对 excluded_subject_ids 去重
        let mut excluded: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for id in &req.excluded_subject_ids {
            if seen.insert(id.as_str()) {
                excluded.push(id.clone());
            }
        }

        // 5. 校验 primary_subject_id 和 excluded_subject_ids 在对应表中存在
        let mut all_ids = vec![req.primary_subject_id.clone()];
        all_ids.extend(excluded.iter().cloned());
        self.check_subjects_exist(&req.subject_type, &all_ids)
            .await
            .map_err(TraitError::from)?;

        let rule_id = snowflake_id_str();
        let priority = req.priority.unwrap_or(0);

        // 6. 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 插入规则主记录
        let insert_sql = r#"
            INSERT INTO cmx_exclusion_rule
                (id, code, name, subject_type, primary_subject_id, violation_message,
                 priority, description, status, archived)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, 0)
        "#;
        let params = vec![
            DataValue::String(rule_id.clone()),
            DataValue::String(req.code.clone()),
            DataValue::String(req.name.clone()),
            DataValue::String(req.subject_type.clone()),
            DataValue::String(req.primary_subject_id.clone()),
            req.violation_message
                .clone()
                .map(DataValue::String)
                .unwrap_or(DataValue::Null),
            DataValue::Int(priority),
            req.description
                .clone()
                .map(DataValue::String)
                .unwrap_or(DataValue::Null),
        ];
        self.mm
            .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("创建规则失败: {e}"))))?;

        // 批量插入规则项
        for subject_id in &excluded {
            let item_id = snowflake_id_str();
            let insert_item_sql = r#"
                INSERT INTO cmx_exclusion_rule_item
                    (id, rule_id, subject_id)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
            "#;
            let params = vec![
                DataValue::String(item_id),
                DataValue::String(rule_id.clone()),
                DataValue::String(subject_id.clone()),
            ];
            self.mm
                .execute_sql_with_datavalues(&self.db_id, Some(txn_id), insert_item_sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("创建规则项失败: {e}")))
                })?;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务提交失败: {e}"))))?;

        // 7. 审计
        let audit_detail = serde_json::json!({
            "rule_id": rule_id,
            "code": req.code,
            "name": req.name,
            "subject_type": req.subject_type,
            "primary_subject_id": req.primary_subject_id,
            "excluded_count": excluded.len(),
        });
        self.audit_write(
            svr_ctx,
            "create_rule",
            "exclusion_rule",
            &rule_id,
            &audit_detail,
        )
        .await;

        let rule = self.query_rule(&rule_id).await.map_err(TraitError::from)?;
        info!(
            rule_id = %rule_id,
            code = %req.code,
            subject_type = %req.subject_type,
            "互斥规则创建成功"
        );
        Ok(rule)
    }

    async fn update_rule(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        data: UpdateExclusionRuleRequest,
    ) -> Result<ExclusionRule, TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::update_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        // 若修改 primary_subject_id，需校验
        if let Some(new_primary) = &data.primary_subject_id {
            // 查询规则的 subject_type
            let existing = self.query_rule(rule_id).await.map_err(TraitError::from)?;
            let subject_type = existing.subject_type;

            // 校验新值在对应表中存在
            self.check_subjects_exist(&subject_type, std::slice::from_ref(new_primary))
                .await
                .map_err(TraitError::from)?;

            // 查询现有 excluded subject_ids，校验新值不在其中
            let items = self
                .query_rule_items(rule_id)
                .await
                .map_err(TraitError::from)?;
            if items.iter().any(|item| item.subject_id == *new_primary) {
                return Err(TraitError::from(IamError::Business(
                    "新的主要对象已存在于互斥对象列表中".to_string(),
                )));
            }
        }

        // 构造动态 UPDATE（不允许修改 subject_type）
        // 使用 ParamsBuilder 自动管理占位符编号,SET 从 $1 起,WHERE id 放最后
        let mut b = ParamsBuilder::new(0);
        b.set_opt("name", data.name)
            .set_opt("primary_subject_id", data.primary_subject_id)
            .set_opt("violation_message", data.violation_message)
            .set_opt("priority", data.priority)
            .set_opt("description", data.description)
            .set_opt("status", data.status);
        let (set_clause, mut params) = b.build();

        if set_clause.is_empty() {
            return Err(TraitError::from(IamError::Business(
                "未提供任何更新字段".to_string(),
            )));
        }

        // WHERE id 参数放最后,占位符编号 = SET 参数数 + 1
        let where_idx = params.len() + 1;
        params.push(DataValue::String(rule_id.to_string()));
        let sql = format!(
            "UPDATE cmx_exclusion_rule SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
        );

        self.mm
            .execute_sql_with_datavalues(&self.db_id, None, &sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("更新规则失败: {e}"))))?;

        // 审计
        self.audit_write(
            svr_ctx,
            "update_rule",
            "exclusion_rule",
            rule_id,
            &serde_json::json!({}),
        )
        .await;

        self.query_rule(rule_id).await.map_err(TraitError::from)
    }

    async fn delete_rule(&self, svr_ctx: &SVRContext, rule_id: &str) -> Result<(), TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::delete_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        let sql = "UPDATE cmx_exclusion_rule SET archived = 1, update_time = NOW() WHERE id = $1";
        let params = vec![DataValue::String(rule_id.to_string())];
        let affected = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("删除规则失败: {e}"))))?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "规则不存在: {rule_id}"
            ))));
        }

        self.audit_write(
            svr_ctx,
            "delete_rule",
            "exclusion_rule",
            rule_id,
            &serde_json::json!({}),
        )
        .await;

        Ok(())
    }

    async fn get_rule(
        &self,
        rule_id: &str,
    ) -> Result<(ExclusionRule, Vec<ExclusionRuleItem>), TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::get_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        let rule = self.query_rule(rule_id).await.map_err(TraitError::from)?;
        let items = self
            .query_rule_items(rule_id)
            .await
            .map_err(TraitError::from)?;
        Ok((rule, items))
    }

    async fn page_rules(
        &self,
        filters: Option<Vec<ExclusionRuleFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<ExclusionRule>, i64), TraitError> {
        debug!("{:<12} - ExclusionRuleServiceImpl::page_rules", "IAM-RULE");

        // 检测是否含跨表 subject_id 过滤（取首个 filter 组中首个 subject_id 等值操作符值）。
        let cross_subject_id: Option<String> = filters.as_ref().and_then(|fs| {
            fs.iter()
                .find_map(|f| f.subject_id.as_ref().and_then(Self::first_string_opval))
        });

        if let Some(subject_id) = cross_subject_id {
            // 路径 B：跨表 raw SQL（JOIN items + DISTINCT + 走 subject_id 索引）
            Self::page_rules_cross_table(&self.mm, &self.db_id, filters, list_options, subject_id)
                .await
        } else {
            // 路径 A：标准 GenericCrudService（与 role/role_group 一致，单表索引）
            // 注入默认 archived=0，并将 subject_id 置 None（避免生成无效列谓词）
            // filters=None 时构造默认 filter，确保归档数据不泄露
            let filters = Some(match filters {
                Some(fs) => fs
                    .into_iter()
                    .map(|mut f| {
                        f.subject_id = None;
                        Self::with_default_archived(f)
                    })
                    .collect::<Vec<_>>(),
                None => {
                    let mut f = Self::with_default_archived(ExclusionRuleFilter::default());
                    f.subject_id = None;
                    vec![f]
                }
            });
            let (dataset, total) =
                GenericCrudService::<ExclusionRuleBmc, ExclusionRuleFilter>::page(
                    &self.mm,
                    &self.db_id,
                    None,
                    filters,
                    list_options,
                )
                .await
                .map_err(|e| TraitError::from(IamError::Business(format!("规则分页失败: {e}"))))?;
            Ok((Self::extract_rules(dataset), total))
        }
    }

    async fn toggle_rule_status(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        status: i64,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::toggle_rule_status - rule_id: {}, status: {}",
            "IAM-RULE", rule_id, status
        );

        let sql = "UPDATE cmx_exclusion_rule SET status = $2, update_time = NOW() WHERE id = $1 AND archived = 0";
        let params = vec![
            DataValue::String(rule_id.to_string()),
            DataValue::Int(status),
        ];
        let affected = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("切换规则状态失败: {e}"))))?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "规则不存在或已归档: {rule_id}"
            ))));
        }

        self.audit_write(
            svr_ctx,
            "toggle_rule_status",
            "exclusion_rule",
            rule_id,
            &serde_json::json!({ "new_status": status }),
        )
        .await;

        Ok(())
    }

    async fn add_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        subject_ids: Vec<String>,
    ) -> Result<u64, TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::add_rule_items - rule_id: {}, count: {}",
            "IAM-RULE",
            rule_id,
            subject_ids.len()
        );

        // 查询规则的 primary_subject_id 和 subject_type
        let rule = self.query_rule(rule_id).await.map_err(TraitError::from)?;

        // 校验新增 subject_id 不等于 primary_subject_id
        for sid in &subject_ids {
            if sid == &rule.primary_subject_id {
                return Err(TraitError::from(IamError::Business(format!(
                    "不能添加与主要对象相同的互斥对象: {sid}"
                ))));
            }
        }

        // 校验新增 subject_id 在对应表中存在
        self.check_subjects_exist(&rule.subject_type, &subject_ids)
            .await
            .map_err(TraitError::from)?;

        // 批量插入规则项
        let mut count: u64 = 0;
        for subject_id in subject_ids {
            let item_id = snowflake_id_str();
            let sql = r#"
                INSERT INTO cmx_exclusion_rule_item
                    (id, rule_id, subject_id)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
            "#;
            let params = vec![
                DataValue::String(item_id),
                DataValue::String(rule_id.to_string()),
                DataValue::String(subject_id),
            ];
            let affected = self
                .mm
                .execute_sql_with_datavalues(&self.db_id, None, sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("添加规则项失败: {e}")))
                })?;
            count += affected as u64;
        }

        self.audit_write(
            svr_ctx,
            "add_rule_items",
            "exclusion_rule",
            rule_id,
            &serde_json::json!({ "added_count": count }),
        )
        .await;

        Ok(count)
    }

    async fn remove_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        item_ids: &[String],
    ) -> Result<u64, TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::remove_rule_items - rule_id: {}, count: {}",
            "IAM-RULE",
            rule_id,
            item_ids.len()
        );

        let sql = "DELETE FROM cmx_exclusion_rule_item WHERE id = ANY($1) AND rule_id = $2";
        let params = vec![
            DataValue::Array(
                item_ids
                    .iter()
                    .map(|i| DataValue::String(i.clone()))
                    .collect(),
            ),
            DataValue::String(rule_id.to_string()),
        ];
        let affected = self
            .mm
            .execute_sql_with_datavalues(&self.db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("移除规则项失败: {e}"))))?;

        let total = affected as u64;
        self.audit_write(
            svr_ctx,
            "remove_rule_items",
            "exclusion_rule",
            rule_id,
            &serde_json::json!({ "removed_count": total }),
        )
        .await;

        Ok(total)
    }

    async fn validate_rule(
        &self,
        req: ValidateRuleRequest,
    ) -> Result<ValidateRuleResponse, TraitError> {
        debug!(
            "{:<12} - ExclusionRuleServiceImpl::validate_rule - perm_count: {}, role_count: {}, user: {:?}",
            "IAM-RULE",
            req.permission_ids.len(),
            req.role_ids.len(),
            req.user_id
        );

        // 1. 构建权限集合
        let mut perm_set: HashSet<String> = req.permission_ids.iter().cloned().collect();

        // 2. 构建角色集合
        let mut role_set: HashSet<String> = req.role_ids.iter().cloned().collect();

        // 可选：合并用户已有权限/角色
        if let Some(uid) = &req.user_id {
            // 查询用户已有权限
            let user_perm_sql = r#"
                SELECT DISTINCT rp.permission_id
                FROM cmx_role_permission rp
                INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
                WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0
                UNION
                SELECT DISTINCT rp.permission_id
                FROM cmx_role_permission rp
                INNER JOIN cmx_user_role_assignment ura ON ura.role_id = rp.role_id
                WHERE ura.user_id = $1 AND ura.status = 1 AND ura.archived = 0
                  AND NOW() BETWEEN ura.effective_from AND ura.effective_until
                  AND rp.archived = 0
            "#;
            let params = vec![DataValue::String(uid.clone())];
            let dataset = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    None,
                    user_perm_sql,
                    params,
                    "validate_user_perms",
                )
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("查询用户权限失败: {e}")))
                })?;
            let schema = dataset.schema.as_ref();
            for row in dataset.iter() {
                if let Some(pid) = row.get_by_name_as::<String>(schema, "permission_id") {
                    perm_set.insert(pid);
                }
            }

            // 查询用户已有角色
            let user_role_sql = r#"
                SELECT DISTINCT role_id
                FROM cmx_user_role
                WHERE user_id = $1 AND archived = 0
                UNION
                SELECT DISTINCT role_id
                FROM cmx_user_role_assignment
                WHERE user_id = $1 AND status = 1 AND archived = 0
                  AND NOW() BETWEEN effective_from AND effective_until
            "#;
            let params = vec![DataValue::String(uid.clone())];
            let dataset = self
                .mm
                .query_sql_with_datavalues(
                    &self.db_id,
                    None,
                    user_role_sql,
                    params,
                    "validate_user_roles",
                )
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("查询用户角色失败: {e}")))
                })?;
            let schema = dataset.schema.as_ref();
            for row in dataset.iter() {
                if let Some(rid) = row.get_by_name_as::<String>(schema, "role_id") {
                    role_set.insert(rid);
                }
            }
        }

        // 3. 加载全部启用规则（含规则项）
        let load_sql = r#"
            SELECT r.id, r.code, r.name, r.subject_type, r.primary_subject_id,
                   r.violation_message, i.subject_id
            FROM cmx_exclusion_rule r
            INNER JOIN cmx_exclusion_rule_item i ON i.rule_id = r.id
            WHERE r.status = 1 AND r.archived = 0
            ORDER BY r.priority DESC, r.id
        "#;
        let params: Vec<DataValue> = vec![];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, load_sql, params, "validate_load_rules")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("加载规则失败: {e}"))))?;

        let schema = dataset.schema.as_ref();
        // 按规则聚合：rule_id -> (code, name, subject_type, primary_subject_id, violation_message, Vec<subject_id>)
        let mut rules_map: HashMap<
            String,
            (String, String, String, String, Option<String>, Vec<String>),
        > = HashMap::new();

        for row in dataset.iter() {
            let rule_id: String = row.get_by_name_as(schema, "id").unwrap_or_default();
            let entry = rules_map.entry(rule_id.clone()).or_insert_with(|| {
                (
                    row.get_by_name_as(schema, "code").unwrap_or_default(),
                    row.get_by_name_as(schema, "name").unwrap_or_default(),
                    row.get_by_name_as(schema, "subject_type")
                        .unwrap_or_default(),
                    row.get_by_name_as(schema, "primary_subject_id")
                        .unwrap_or_default(),
                    row.get_by_name_as(schema, "violation_message"),
                    Vec::new(),
                )
            });
            if let Some(sid) = row.get_by_name_as::<String>(schema, "subject_id") {
                entry.5.push(sid);
            }
        }

        // 4. 按 subject_type 分别校验（收集所有 violations，不快速失败）
        let mut violations = Vec::new();
        for (
            rule_id,
            (code, name, subject_type, primary_subject_id, violation_message, excluded_ids),
        ) in &rules_map
        {
            let target_set = match subject_type.as_str() {
                "permission" => &perm_set,
                "role" => &role_set,
                _ => continue,
            };

            // 仅当 primary_subject_id 在集合中时，检查互斥对象是否也在集合中
            if target_set.contains(primary_subject_id) {
                for excluded_id in excluded_ids {
                    if target_set.contains(excluded_id) {
                        violations.push(RuleViolationDetail {
                            rule_id: rule_id.clone(),
                            rule_code: code.clone(),
                            rule_name: name.clone(),
                            subject_type: subject_type.clone(),
                            violation_message: violation_message
                                .clone()
                                .unwrap_or_else(|| format!("对象组合违反互斥规则 [{}]", code)),
                            primary_subject_id: primary_subject_id.clone(),
                            conflicting_subject_id: excluded_id.clone(),
                        });
                    }
                }
            }
        }

        // 5. 返回结果
        let passed = violations.is_empty();
        Ok(ValidateRuleResponse { passed, violations })
    }
}
