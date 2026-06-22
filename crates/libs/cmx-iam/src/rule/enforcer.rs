//! 权限规则校验引擎
//!
//! 提供 `RuleEnforcer` trait 和 `RuleEnforcerImpl` 实现，
//! 在 `assign_permissions` 和 `assign_roles` 时进行 SoD 规则校验。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use cmx_database::DatabaseManager;
use serde_json::Value;
use tracing::debug;

use crate::config::{IamConfig, SodCheckScope};
use crate::error::IamError;

/// 规则校验引擎 trait
#[async_trait]
pub trait RuleEnforcer: Send + Sync {
    /// 校验角色权限组合本身是否违反互斥规则
    ///
    /// - `permission_ids`：待分配给角色的权限ID列表
    /// - 仅校验互斥规则（依赖规则针对用户场景，角色层不校验）
    async fn check_role_permissions(
        &self,
        permission_ids: &[String],
    ) -> Result<(), IamError>;

    /// 校验用户权限组合是否违反规则（合并现有权限 + 待分配角色权限）
    ///
    /// - `user_id`：目标用户ID（用于查询已有权限）
    /// - `role_ids`：待分配的角色ID列表（展开到权限后与现有权限合并）
    async fn check_user_roles(
        &self,
        user_id: &str,
        role_ids: &[String],
    ) -> Result<(), IamError>;
}

/// 规则校验引擎实现
pub struct RuleEnforcerImpl {
    mm: Arc<DatabaseManager>,
    db_id: String,
    config: IamConfig,
}

impl RuleEnforcerImpl {
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self { mm, db_id, config }
    }

    /// 加载所有启用的规则及其关联权限项
    /// 返回：(rule_code, rule_type, violation_message, Vec<(group_seq, permission_id)>)
    async fn load_active_rules(
        &self,
    ) -> Result<Vec<LoadedRule>, IamError> {
        let sql = r#"
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
            .query_sql_with_json(&self.db_id, None, sql, params, "load_rules")
            .await
            .map_err(|e| IamError::Business(format!("加载规则失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let mut rules_map: HashMap<String, LoadedRule> = HashMap::new();

        for row in dataset.iter() {
            let rule_id: String = row
                .get_by_name_as(schema, "id")
                .unwrap_or_default();
            let rule_entry = rules_map.entry(rule_id.clone()).or_insert_with(|| {
                LoadedRule {
                    id: rule_id.clone(),
                    code: row
                        .get_by_name_as(schema, "code")
                        .unwrap_or_default(),
                    name: row
                        .get_by_name_as(schema, "name")
                        .unwrap_or_default(),
                    rule_type: row
                        .get_by_name_as(schema, "rule_type")
                        .unwrap_or_default(),
                    violation_message: row
                        .get_by_name_as(schema, "violation_message"),
                    items: Vec::new(),
                }
            });

            let group_seq: i64 =
                row.get_by_name_as(schema, "group_seq").unwrap_or(1);
            let permission_id: String = row
                .get_by_name_as(schema, "permission_id")
                .unwrap_or_default();
            rule_entry.items.push((group_seq, permission_id));
        }

        Ok(rules_map.into_values().collect())
    }

    /// 查询用户当前已有的权限ID集合（合并永久 + 临时授权）
    async fn get_user_permission_ids(&self, user_id: &str) -> Result<HashSet<String>, IamError> {
        let sql = r#"
            SELECT DISTINCT rp.permission_id
            FROM cmx_role_permission rp
            INNER JOIN cmx_user_role ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1 AND ur.archived = 0 AND rp.archived = 0

            UNION

            SELECT DISTINCT rp.permission_id
            FROM cmx_role_permission rp
            INNER JOIN cmx_user_role_assignment ura ON ura.role_id = rp.role_id
            WHERE ura.user_id = $1
              AND ura.status = 1 AND ura.archived = 0
              AND NOW() BETWEEN ura.effective_from AND ura.effective_until
              AND rp.archived = 0
        "#;
        let params = Value::Array(vec![Value::String(user_id.to_string())]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "user_perm_ids")
            .await
            .map_err(|e| IamError::Business(format!("查询用户权限ID失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let mut perm_ids = HashSet::new();
        for row in dataset.iter() {
            if let Some(pid) = row.get_by_name_as::<String>(schema, "permission_id") {
                perm_ids.insert(pid);
            }
        }
        Ok(perm_ids)
    }

    /// 查询多个角色关联的权限ID集合
    async fn get_role_permission_ids(&self, role_ids: &[String]) -> Result<HashSet<String>, IamError> {
        if role_ids.is_empty() {
            return Ok(HashSet::new());
        }
        // 单次查询所有角色的权限ID（避免 N+1）
        let sql = r#"
            SELECT permission_id FROM cmx_role_permission
            WHERE role_id = ANY($1) AND archived = 0
        "#;
        let role_id_array = Value::Array(
            role_ids.iter().map(|id| Value::String(id.clone())).collect(),
        );
        let params = Value::Array(vec![role_id_array]);
        let dataset = self
            .mm
            .query_sql_with_json(&self.db_id, None, sql, params, "role_perm_ids")
            .await
            .map_err(|e| IamError::Business(format!("查询角色权限ID失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let mut perm_ids = HashSet::new();
        for row in dataset.iter() {
            if let Some(pid) = row.get_by_name_as::<String>(schema, "permission_id") {
                perm_ids.insert(pid);
            }
        }
        Ok(perm_ids)
    }

    /// 校验互斥规则
    /// 检查 permission_set 是否包含同一互斥规则下的任意两个权限
    fn check_mutual_exclusion(
        &self,
        rules: &[LoadedRule],
        permission_set: &HashSet<String>,
    ) -> Result<(), IamError> {
        for rule in rules {
            if rule.rule_type != "mutual_exclusion" {
                continue;
            }
            // 收集该规则下用户拥有的所有权限
            let matched: Vec<&String> = rule
                .items
                .iter()
                .filter_map(|(_, pid)| {
                    if permission_set.contains(pid) {
                        Some(pid)
                    } else {
                        None
                    }
                })
                .collect();

            if matched.len() >= 2 {
                // 违反互斥规则
                let message = rule.violation_message.clone().unwrap_or_else(|| {
                    format!("权限组合违反互斥规则 [{}]", rule.code)
                });
                return Err(IamError::RuleViolation {
                    rule_code: rule.code.clone(),
                    message,
                });
            }
        }
        Ok(())
    }

    /// 校验依赖规则
    /// 对于每个 group_seq=2 的权限，检查是否至少有一个 group_seq=1 的权限
    fn check_dependency(
        &self,
        rules: &[LoadedRule],
        permission_set: &HashSet<String>,
    ) -> Result<(), IamError> {
        for rule in rules {
            if rule.rule_type != "dependency" {
                continue;
            }
            // group_seq=1 的权限集合（前置条件）
            let prerequisites: HashSet<&String> = rule
                .items
                .iter()
                .filter(|(seq, _)| *seq == 1)
                .map(|(_, pid)| pid)
                .collect();

            // group_seq=2 的权限集合（依赖前置）
            let dependents: HashSet<&String> = rule
                .items
                .iter()
                .filter(|(seq, _)| *seq == 2)
                .map(|(_, pid)| pid)
                .collect();

            // 检查用户拥有的 group_seq=2 权限是否都有至少一个 group_seq=1 权限
            let has_prerequisite = prerequisites.iter().any(|pid| permission_set.contains(*pid));

            for dep_pid in &dependents {
                if permission_set.contains(*dep_pid) && !has_prerequisite {
                    let message = rule
                        .violation_message
                        .clone()
                        .unwrap_or_else(|| {
                            format!("权限依赖规则违反 [{}]：缺少前置权限", rule.code)
                        });
                    return Err(IamError::RuleViolation {
                        rule_code: rule.code.clone(),
                        message,
                    });
                }
            }
        }
        Ok(())
    }
}

/// 加载的规则数据
struct LoadedRule {
    #[allow(dead_code)]
    id: String,
    code: String,
    #[allow(dead_code)]
    name: String,
    rule_type: String,
    violation_message: Option<String>,
    /// (group_seq, permission_id)
    items: Vec<(i64, String)>,
}

#[async_trait]
impl RuleEnforcer for RuleEnforcerImpl {
    async fn check_role_permissions(
        &self,
        permission_ids: &[String],
    ) -> Result<(), IamError> {
        if !self.config.enable_sod_check {
            return Ok(());
        }

        debug!(
            "{:<12} - RuleEnforcer::check_role_permissions - count: {}",
            "IAM-RULE",
            permission_ids.len()
        );

        let rules = self.load_active_rules().await?;
        let perm_set: HashSet<String> = permission_ids.iter().cloned().collect();

        // 角色层仅校验互斥规则
        self.check_mutual_exclusion(&rules, &perm_set)
    }

    async fn check_user_roles(
        &self,
        user_id: &str,
        role_ids: &[String],
    ) -> Result<(), IamError> {
        if !self.config.enable_sod_check {
            return Ok(());
        }

        debug!(
            "{:<12} - RuleEnforcer::check_user_roles - user: {}, role_count: {}",
            "IAM-RULE",
            user_id,
            role_ids.len()
        );

        let rules = self.load_active_rules().await?;

        // 1. 查询用户当前已有权限
        let existing_perms = self.get_user_permission_ids(user_id).await?;

        // 2. 展开待分配角色到权限集合
        let new_perm_ids = self.get_role_permission_ids(role_ids).await?;

        // 3. 根据配置决定校验范围
        match self.config.sod_check_scope {
            SodCheckScope::All => {
                // 校验合并后的完整权限集合（已有 + 新增）
                let mut perm_set = existing_perms.clone();
                perm_set.extend(new_perm_ids.clone());
                self.check_mutual_exclusion(&rules, &perm_set)?;
                self.check_dependency(&rules, &perm_set)?;
            }
            SodCheckScope::Incremental => {
                // 仅校验本次新增权限是否违反规则
                // 互斥：仅检查新增权限之间是否互斥
                let new_only: HashSet<String> = new_perm_ids.iter().cloned().collect();
                self.check_mutual_exclusion(&rules, &new_only)?;
                // 依赖：检查新增权限中的 group_seq=2 是否有前置权限（新增或已有均可）
                let mut merged = existing_perms.clone();
                merged.extend(new_perm_ids);
                self.check_dependency(&rules, &merged)?;
            }
        }

        Ok(())
    }
}
