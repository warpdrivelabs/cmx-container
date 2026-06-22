//! 权限规则 Service 实现
//!
//! 提供规则的 CRUD、启用/禁用、规则项管理、校验测试等功能。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_utils::snowflake_id_str;
use serde_json::Value;
use tracing::{debug, info};

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::error::IamError;
use crate::rule::entity::{
    CreatePermissionRuleRequest, PermissionRule, PermissionRuleForUpdate,
    PermissionRuleItem, RuleItemInput, RuleViolationDetail, ValidateRuleResponse,
};

/// 权限规则 Service trait
#[async_trait]
pub trait PermissionRuleService: Send + Sync {
    /// 创建规则（含规则项）
    async fn create_rule(
        &self,
        svr_ctx: &SVRContext,
        req: CreatePermissionRuleRequest,
    ) -> Result<PermissionRule, TraitError>;

    /// 更新规则基本信息
    async fn update_rule(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        data: PermissionRuleForUpdate,
    ) -> Result<PermissionRule, TraitError>;

    /// 删除规则（软删除 archived=1）
    async fn delete_rule(&self, svr_ctx: &SVRContext, rule_id: &str) -> Result<(), TraitError>;

    /// 查询规则详情（含规则项）
    async fn get_rule(&self, rule_id: &str) -> Result<(PermissionRule, Vec<PermissionRuleItem>), TraitError>;

    /// 分页查询规则
    async fn page_rules(
        &self,
        current: u64,
        size: u64,
    ) -> Result<(Vec<PermissionRule>, i64), TraitError>;

    /// 切换规则状态（启用/禁用）
    async fn toggle_rule_status(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        status: i64,
    ) -> Result<(), TraitError>;

    /// 添加规则项
    async fn add_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        items: Vec<RuleItemInput>,
    ) -> Result<u64, TraitError>;

    /// 移除规则项
    async fn remove_rule_items(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        item_ids: &[String],
    ) -> Result<u64, TraitError>;

    /// 规则校验测试（给定权限组合，测试是否违反规则）
    async fn validate_rule(
        &self,
        permission_ids: &[String],
        user_id: Option<&str>,
    ) -> Result<ValidateRuleResponse, TraitError>;
}

/// 权限规则 Service 实现
pub struct PermissionRuleServiceImpl {
    mm: Arc<DatabaseManager>,
    db_id: String,
    #[allow(dead_code)]
    config: IamConfig,
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
}

impl PermissionRuleServiceImpl {
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

    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 从 DataSet 提取 PermissionRule（单条）
    fn extract_rule(dataset: cmx_core::model::data::dataset::DataSet) -> Option<PermissionRule> {
        let schema = dataset.schema.as_ref();
        let row = dataset.iter().next()?;

        Some(PermissionRule {
            id: row.get_by_name_as(schema, "id")?,
            code: row.get_by_name_as(schema, "code")?,
            name: row.get_by_name_as(schema, "name")?,
            rule_type: row.get_by_name_as(schema, "rule_type")?,
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

    /// 从 DataSet 提取 PermissionRule 列表
    fn extract_rules_from_dataset(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Vec<PermissionRule> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                Some(PermissionRule {
                    id: row.get_by_name_as(schema, "id")?,
                    code: row.get_by_name_as(schema, "code")?,
                    name: row.get_by_name_as(schema, "name")?,
                    rule_type: row.get_by_name_as(schema, "rule_type")?,
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
            })
            .collect()
    }

    /// 从 DataSet 提取 PermissionRuleItem 列表
    fn extract_items(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<PermissionRuleItem> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                Some(PermissionRuleItem {
                    id: row.get_by_name_as(schema, "id")?,
                    rule_id: row.get_by_name_as(schema, "rule_id")?,
                    group_seq: row.get_by_name_as::<i64>(schema, "group_seq").unwrap_or(1),
                    permission_id: row.get_by_name_as(schema, "permission_id")?,
                })
            })
            .collect()
    }

    /// 查询单条规则
    async fn query_rule(&self, rule_id: &str) -> Result<PermissionRule, IamError> {
        let sql = r#"
            SELECT id, code, name, rule_type, violation_message, priority, description,
                   status, archived, create_time, update_time,
                   create_by, create_name, update_by, update_name
            FROM cmx_permission_rule
            WHERE id = $1
        "#;
        let params = Value::Array(vec![Value::String(rule_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "query_rule")
            .await
            .map_err(|e| IamError::Business(format!("查询规则失败: {e}")))?;

        Self::extract_rule(dataset).ok_or_else(|| IamError::Business(format!("规则不存在: {rule_id}")))
    }

    /// 查询规则项
    async fn query_rule_items(&self, rule_id: &str) -> Result<Vec<PermissionRuleItem>, IamError> {
        let sql = r#"
            SELECT id, rule_id, group_seq, permission_id
            FROM cmx_permission_rule_item
            WHERE rule_id = $1
        "#;
        let params = Value::Array(vec![Value::String(rule_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "query_rule_items")
            .await
            .map_err(|e| IamError::Business(format!("查询规则项失败: {e}")))?;

        Ok(Self::extract_items(dataset))
    }
}

impl AuditHelper for PermissionRuleServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

#[async_trait]
impl PermissionRuleService for PermissionRuleServiceImpl {
    async fn create_rule(
        &self,
        svr_ctx: &SVRContext,
        req: CreatePermissionRuleRequest,
    ) -> Result<PermissionRule, TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::create_rule - code: {}",
            "IAM-RULE", req.code
        );

        // 校验 rule_type
        if req.rule_type != "mutual_exclusion" && req.rule_type != "dependency" {
            return Err(TraitError::from(IamError::Business(format!(
                "无效的规则类型: {}（应为 mutual_exclusion 或 dependency）",
                req.rule_type
            ))));
        }

        let rule_id = snowflake_id_str();
        let priority = req.priority.unwrap_or(0);

        // 开启事务
        let txn_ctx = self.mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("事务开始失败: {e}"))))?;
        let txn_id = guard.txn_id();

        // 插入规则
        let insert_sql = r#"
            INSERT INTO cmx_permission_rule
                (id, code, name, rule_type, violation_message, priority, description, status, archived)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 0)
        "#;
        let params = Value::Array(vec![
            Value::String(rule_id.clone()),
            Value::String(req.code.clone()),
            Value::String(req.name.clone()),
            Value::String(req.rule_type.clone()),
            req.violation_message
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
            Value::Number(priority.into()),
            req.description
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ]);
        self.mm
            .execute_sql_with_json(&self.db_id, Some(txn_id), insert_sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("创建规则失败: {e}")))
            })?;

        // 插入规则项
        for item in &req.items {
            let item_id = snowflake_id_str();
            let insert_item_sql = r#"
                INSERT INTO cmx_permission_rule_item
                    (id, rule_id, group_seq, permission_id)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (rule_id, group_seq, permission_id) DO NOTHING
            "#;
            let params = Value::Array(vec![
                Value::String(item_id),
                Value::String(rule_id.clone()),
                Value::Number(item.group_seq.into()),
                Value::String(item.permission_id.clone()),
            ]);
            self.mm
                .execute_sql_with_json(&self.db_id, Some(txn_id), insert_item_sql, params)
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

        // 审计
        let audit_detail = serde_json::json!({
            "rule_id": rule_id,
            "code": req.code,
            "name": req.name,
            "rule_type": req.rule_type,
            "items_count": req.items.len(),
        });
        self.audit_write(svr_ctx, "create_rule", "permission_rule", &rule_id, &audit_detail)
            .await;

        let rule = self.query_rule(&rule_id).await.map_err(TraitError::from)?;
        info!(rule_id = %rule_id, code = %req.code, "权限规则创建成功");
        Ok(rule)
    }

    async fn update_rule(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        data: PermissionRuleForUpdate,
    ) -> Result<PermissionRule, TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::update_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        // 构造动态 UPDATE
        let mut sets: Vec<String> = Vec::new();
        let mut params: Vec<Value> = vec![Value::String(rule_id.to_string())];
        let mut idx = 2;

        if let Some(name) = data.name {
            sets.push(format!("name = ${idx}"));
            params.push(Value::String(name));
            idx += 1;
        }
        if let Some(vm) = data.violation_message {
            sets.push(format!("violation_message = ${idx}"));
            params.push(Value::String(vm));
            idx += 1;
        }
        if let Some(priority) = data.priority {
            sets.push(format!("priority = ${idx}"));
            params.push(Value::Number(priority.into()));
            idx += 1;
        }
        if let Some(desc) = data.description {
            sets.push(format!("description = ${idx}"));
            params.push(Value::String(desc));
            idx += 1;
        }
        if let Some(status) = data.status {
            sets.push(format!("status = ${idx}"));
            params.push(Value::Number(status.into()));
            idx += 1;
        }

        if sets.is_empty() {
            return Err(TraitError::from(IamError::Business(
                "未提供任何更新字段".to_string(),
            )));
        }

        sets.push("update_time = NOW()".to_string());
        let sql = format!(
            "UPDATE cmx_permission_rule SET {} WHERE id = $1",
            sets.join(", ")
        );

        self.mm
            .execute_sql_with_json(&self.db_id, None, &sql, Value::Array(params))
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("更新规则失败: {e}")))
            })?;

        // 审计
        self.audit_write(svr_ctx, "update_rule", "permission_rule", rule_id, &serde_json::json!({}))
            .await;

        self.query_rule(rule_id).await.map_err(TraitError::from)
    }

    async fn delete_rule(&self, svr_ctx: &SVRContext, rule_id: &str) -> Result<(), TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::delete_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        let sql = "UPDATE cmx_permission_rule SET archived = 1, update_time = NOW() WHERE id = $1";
        let params = Value::Array(vec![Value::String(rule_id.to_string())]);
        let affected = self
            .mm
            .execute_sql_with_json(&self.db_id, None, sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("删除规则失败: {e}")))
            })?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "规则不存在: {rule_id}"
            ))));
        }

        self.audit_write(svr_ctx, "delete_rule", "permission_rule", rule_id, &serde_json::json!({}))
            .await;

        Ok(())
    }

    async fn get_rule(
        &self,
        rule_id: &str,
    ) -> Result<(PermissionRule, Vec<PermissionRuleItem>), TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::get_rule - rule_id: {}",
            "IAM-RULE", rule_id
        );

        let rule = self.query_rule(rule_id).await.map_err(TraitError::from)?;
        let items = self.query_rule_items(rule_id).await.map_err(TraitError::from)?;
        Ok((rule, items))
    }

    async fn toggle_rule_status(
        &self,
        svr_ctx: &SVRContext,
        rule_id: &str,
        status: i64,
    ) -> Result<(), TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::toggle_rule_status - rule_id: {}, status: {}",
            "IAM-RULE", rule_id, status
        );

        let sql = "UPDATE cmx_permission_rule SET status = $2, update_time = NOW() WHERE id = $1 AND archived = 0";
        let params = Value::Array(vec![
            Value::String(rule_id.to_string()),
            Value::Number(status.into()),
        ]);
        let affected = self
            .mm
            .execute_sql_with_json(&self.db_id, None, sql, params)
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("切换规则状态失败: {e}")))
            })?;

        if affected == 0 {
            return Err(TraitError::from(IamError::Business(format!(
                "规则不存在或已归档: {rule_id}"
            ))));
        }

        self.audit_write(
            svr_ctx,
            "toggle_rule_status",
            "permission_rule",
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
        items: Vec<RuleItemInput>,
    ) -> Result<u64, TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::add_rule_items - rule_id: {}, count: {}",
            "IAM-RULE", rule_id, items.len()
        );

        let mut count: u64 = 0;
        for item in items {
            let item_id = snowflake_id_str();
            let sql = r#"
                INSERT INTO cmx_permission_rule_item
                    (id, rule_id, group_seq, permission_id)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (rule_id, group_seq, permission_id) DO NOTHING
            "#;
            let params = Value::Array(vec![
                Value::String(item_id),
                Value::String(rule_id.to_string()),
                Value::Number(item.group_seq.into()),
                Value::String(item.permission_id),
            ]);
            let affected = self
                .mm
                .execute_sql_with_json(&self.db_id, None, sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("添加规则项失败: {e}")))
                })?;
            count += affected as u64;
        }

        self.audit_write(
            svr_ctx,
            "add_rule_items",
            "permission_rule",
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
            "{:<12} - PermissionRuleServiceImpl::remove_rule_items - rule_id: {}, count: {}",
            "IAM-RULE", rule_id, item_ids.len()
        );

        let mut total: u64 = 0;
        for item_id in item_ids {
            let sql = "DELETE FROM cmx_permission_rule_item WHERE id = $1 AND rule_id = $2";
            let params = Value::Array(vec![
                Value::String(item_id.clone()),
                Value::String(rule_id.to_string()),
            ]);
            let affected = self
                .mm
                .execute_sql_with_json(&self.db_id, None, sql, params)
                .await
                .map_err(|e| {
                    TraitError::from(IamError::Business(format!("移除规则项失败: {e}")))
                })?;
            total += affected as u64;
        }

        self.audit_write(
            svr_ctx,
            "remove_rule_items",
            "permission_rule",
            rule_id,
            &serde_json::json!({ "removed_count": total }),
        )
        .await;

        Ok(total)
    }

    async fn page_rules(
        &self,
        current: u64,
        size: u64,
    ) -> Result<(Vec<PermissionRule>, i64), TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::page_rules - current: {}, size: {}",
            "IAM-RULE", current, size
        );

        let offset = if current > 0 {
            ((current - 1) * size) as i64
        } else {
            return Err(TraitError::from(IamError::Business(
                "current 必须 >= 1".to_string(),
            )));
        };

        // 查询总数
        let count_sql = "SELECT COUNT(*) as cnt FROM cmx_permission_rule WHERE archived = 0";
        let params = Value::Array(vec![]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, count_sql, params, "rule_count")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询规则总数失败: {e}"))))?;
        let schema = dataset.schema.as_ref();
        let total: i64 = dataset
            .iter()
            .next()
            .and_then(|row| row.get_by_name_as::<i64>(schema, "cnt"))
            .unwrap_or(0);

        // 查询分页数据
        let page_sql = r#"
            SELECT id, code, name, rule_type, violation_message, priority, description,
                   status, archived, create_time, update_time,
                   create_by, create_name, update_by, update_name
            FROM cmx_permission_rule
            WHERE archived = 0
            ORDER BY priority DESC, create_time DESC
            LIMIT $1 OFFSET $2
        "#;
        let params = Value::Array(vec![
            Value::Number((size as i64).into()),
            Value::Number(offset.into()),
        ]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, page_sql, params, "rule_page")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("分页查询规则失败: {e}"))))?;

        let rules: Vec<PermissionRule> = Self::extract_rules_from_dataset(dataset);
        Ok((rules, total))
    }

    async fn validate_rule(
        &self,
        permission_ids: &[String],
        user_id: Option<&str>,
    ) -> Result<ValidateRuleResponse, TraitError> {
        debug!(
            "{:<12} - PermissionRuleServiceImpl::validate_rule - perm_count: {}, user: {:?}",
            "IAM-RULE", permission_ids.len(), user_id
        );

        // 加载启用规则
        let load_sql = r#"
            SELECT r.id, r.code, r.name, r.rule_type, r.violation_message,
                   i.group_seq, i.permission_id
            FROM cmx_permission_rule r
            INNER JOIN cmx_permission_rule_item i ON i.rule_id = r.id
            WHERE r.status = 1 AND r.archived = 0
            ORDER BY r.priority DESC, r.id
        "#;
        let params = Value::Array(vec![]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, load_sql, params, "validate_load_rules")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("加载规则失败: {e}"))))?;

        let schema = dataset.schema.as_ref();
        use std::collections::{HashMap, HashSet};
        let mut rules_map: HashMap<String, (String, String, String, Option<String>, Vec<(i64, String)>)> =
            HashMap::new();

        for row in dataset.iter() {
            let rule_id: String = row.get_by_name_as(schema, "id").unwrap_or_default();
            let rule_entry = rules_map.entry(rule_id.clone()).or_insert_with(|| {
                (
                    row.get_by_name_as(schema, "code").unwrap_or_default(),
                    row.get_by_name_as(schema, "name").unwrap_or_default(),
                    row.get_by_name_as(schema, "rule_type").unwrap_or_default(),
                    row.get_by_name_as(schema, "violation_message"),
                    Vec::new(),
                )
            });
            let group_seq: i64 = row.get_by_name_as(schema, "group_seq").unwrap_or(1);
            let permission_id: String =
                row.get_by_name_as(schema, "permission_id").unwrap_or_default();
            rule_entry.4.push((group_seq, permission_id));
        }

        // 构建权限集合
        let mut perm_set: HashSet<String> = permission_ids.iter().cloned().collect();
        if let Some(uid) = user_id {
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
            let params = Value::Array(vec![Value::String(uid.to_string())]);
            let dataset = self
                .mm
                .query_sql_with_json(&self.db_id, None, user_perm_sql, params, "validate_user_perms")
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
        }

        // 校验规则
        let mut violations = Vec::new();
        for (rule_id, (code, name, rule_type, violation_message, items)) in &rules_map {
            if rule_type == "mutual_exclusion" {
                let matched: Vec<String> = items
                    .iter()
                    .filter(|(_, pid)| perm_set.contains(pid))
                    .map(|(_, pid)| pid.clone())
                    .collect();
                if matched.len() >= 2 {
                    violations.push(RuleViolationDetail {
                        rule_id: rule_id.clone(),
                        rule_code: code.clone(),
                        rule_name: name.clone(),
                        rule_type: rule_type.clone(),
                        violation_message: violation_message
                            .clone()
                            .unwrap_or_else(|| format!("权限组合违反互斥规则 [{}]", code)),
                        conflicting_permission_ids: matched,
                    });
                }
            } else if rule_type == "dependency" {
                let prerequisites: HashSet<&String> =
                    items.iter().filter(|(seq, _)| *seq == 1).map(|(_, pid)| pid).collect();
                let dependents: Vec<&String> =
                    items.iter().filter(|(seq, _)| *seq == 2).map(|(_, pid)| pid).collect();
                let has_prerequisite = prerequisites.iter().any(|pid| perm_set.contains(*pid));
                let conflicting: Vec<String> = dependents
                    .iter()
                    .filter(|pid| perm_set.contains(**pid) && !has_prerequisite)
                    .map(|pid| (*pid).clone())
                    .collect();
                if !conflicting.is_empty() {
                    violations.push(RuleViolationDetail {
                        rule_id: rule_id.clone(),
                        rule_code: code.clone(),
                        rule_name: name.clone(),
                        rule_type: rule_type.clone(),
                        violation_message: violation_message
                            .clone()
                            .unwrap_or_else(|| {
                                format!("权限依赖规则违反 [{}]：缺少前置权限", code)
                            }),
                        conflicting_permission_ids: conflicting,
                    });
                }
            }
        }

        let passed = violations.is_empty();
        Ok(ValidateRuleResponse { passed, violations })
    }
}
