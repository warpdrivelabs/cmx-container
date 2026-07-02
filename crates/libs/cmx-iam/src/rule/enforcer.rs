//! 互斥规则校验引擎
//!
//! 提供 `RuleEnforcer` trait 和 `RuleEnforcerImpl` 实现，
//! 在 `assign_permissions` 和 `assign_roles` 时进行互斥规则校验。
//!
//! 核心模型：「1 主对象 + N 互斥对象」。仅当用户集合同时包含主对象和
//! 任一互斥对象时判定违反，互斥对象之间不互斥。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::cell::DataValue;
use cmx_database::DatabaseManager;
use tracing::debug;

use crate::config::IamConfig;
use crate::error::IamError;

/// 规则校验引擎 trait。
///
/// 定义互斥规则校验接口，供 `assign_permissions` 和 `assign_roles` 时调用。
/// 实现见 `RuleEnforcerImpl`。
#[async_trait]
pub trait RuleEnforcer: Send + Sync {
    /// 校验角色权限组合本身是否违反功能权限互斥规则。
    ///
    /// 仅校验 `subject_type = 'permission'` 的互斥规则（角色层不涉及角色互斥）。
    ///
    /// # Arguments
    ///
    /// * `permission_ids` - 待分配给角色的权限 ID 列表。
    ///
    /// # Errors
    ///
    /// 当权限组合违反互斥规则时返回 `IamError::RuleViolation`。
    async fn check_role_permissions(&self, permission_ids: &[String]) -> Result<(), IamError>;

    /// 校验用户角色组合是否违反互斥规则（合并现有权限/角色 + 待分配角色权限）。
    ///
    /// 同时校验功能权限互斥和角色互斥两类规则。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID（用于查询已有权限和角色）。
    /// * `role_ids` - 待分配的角色 ID 列表。
    ///
    /// # Errors
    ///
    /// 当合并后的权限或角色组合违反互斥规则时返回 `IamError::RuleViolation`。
    async fn check_user_roles(&self, user_id: &str, role_ids: &[String]) -> Result<(), IamError>;
}

/// 规则校验引擎实现。
///
/// 通过数据库查询加载启用的互斥规则，在内存中执行校验。
/// 当 `IamConfig::enable_sod_check` 为 `false` 时，所有校验直接通过。
pub struct RuleEnforcerImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,
    /// 认证库 `db_id`。
    db_id: String,
    /// IAM 配置。
    config: IamConfig,
}

impl RuleEnforcerImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id` 及是否启用 SoD 校验。
    ///
    /// # Returns
    ///
    /// 返回新的 `RuleEnforcerImpl` 实例。
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self { mm, db_id, config }
    }

    /// 加载所有启用的互斥规则及其互斥对象项
    ///
    /// - `subject_type`：可选过滤对象类型（`permission` / `role`），None 加载全部
    ///
    /// 按 rule_id 聚合 excluded_ids
    async fn load_active_rules(
        &self,
        subject_type: Option<&str>,
    ) -> Result<Vec<LoadedRule>, IamError> {
        let sql = r#"
            SELECT r.id, r.code, r.name, r.subject_type, r.primary_subject_id,
                   r.violation_message, i.subject_id
            FROM cmx_exclusion_rule r
            INNER JOIN cmx_exclusion_rule_item i ON i.rule_id = r.id
            WHERE r.status = 1 AND r.archived = 0
              AND ($1::text IS NULL OR r.subject_type = $1)
            ORDER BY r.priority DESC, r.id
        "#;
        let params: Vec<DataValue> = vec![
            subject_type
                .map(|s| DataValue::String(s.to_string()))
                .unwrap_or(DataValue::Null),
        ];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "load_rules")
            .await
            .map_err(|e| IamError::Business(format!("加载规则失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let mut rules_map: HashMap<String, LoadedRule> = HashMap::new();

        for row in dataset.iter() {
            let rule_id: String = row.get_by_name_as(schema, "id").unwrap_or_default();
            let rule_entry = rules_map
                .entry(rule_id.clone())
                .or_insert_with(|| LoadedRule {
                    id: rule_id.clone(),
                    code: row.get_by_name_as(schema, "code").unwrap_or_default(),
                    name: row.get_by_name_as(schema, "name").unwrap_or_default(),
                    subject_type: row
                        .get_by_name_as(schema, "subject_type")
                        .unwrap_or_default(),
                    primary_subject_id: row
                        .get_by_name_as(schema, "primary_subject_id")
                        .unwrap_or_default(),
                    violation_message: row.get_by_name_as(schema, "violation_message"),
                    excluded_ids: HashSet::new(),
                });

            let subject_id: String = row.get_by_name_as(schema, "subject_id").unwrap_or_default();
            rule_entry.excluded_ids.insert(subject_id);
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
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_perm_ids")
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
    async fn get_role_permission_ids(
        &self,
        role_ids: &[String],
    ) -> Result<HashSet<String>, IamError> {
        if role_ids.is_empty() {
            return Ok(HashSet::new());
        }
        // 单次查询所有角色的权限ID（避免 N+1）
        let sql = r#"
            SELECT permission_id FROM cmx_role_permission
            WHERE role_id = ANY($1) AND archived = 0
        "#;
        let role_id_array = DataValue::Array(
            role_ids
                .iter()
                .map(|id| DataValue::String(id.clone()))
                .collect(),
        );
        let params = vec![role_id_array];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "role_perm_ids")
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

    /// 查询用户当前已有的角色ID集合（合并永久 + 临时授权）
    async fn get_user_role_ids(&self, user_id: &str) -> Result<HashSet<String>, IamError> {
        let sql = r#"
            SELECT role_id FROM cmx_user_role
            WHERE user_id = $1 AND archived = 0

            UNION

            SELECT role_id FROM cmx_user_role_assignment
            WHERE user_id = $1 AND status = 1 AND archived = 0
              AND NOW() BETWEEN effective_from AND effective_until
        "#;
        let params = vec![DataValue::String(user_id.to_string())];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_role_ids")
            .await
            .map_err(|e| IamError::Business(format!("查询用户角色ID失败: {e}")))?;

        let schema = dataset.schema.as_ref();
        let mut role_ids = HashSet::new();
        for row in dataset.iter() {
            if let Some(rid) = row.get_by_name_as::<String>(schema, "role_id") {
                role_ids.insert(rid);
            }
        }
        Ok(role_ids)
    }

    /// 统一互斥校验
    ///
    /// 遍历规则，仅当 `subject_set` 包含 `primary_subject_id` 时，
    /// 检查是否同时包含任一 `excluded_id`。违反时返回 `IamError::RuleViolation`。
    fn check_exclusion(
        rules: &[LoadedRule],
        subject_set: &HashSet<String>,
    ) -> Result<(), IamError> {
        for rule in rules {
            // 不含主对象则不可能违反本规则
            if !subject_set.contains(&rule.primary_subject_id) {
                continue;
            }
            // 同时包含任一互斥对象即违反
            let violated = rule.excluded_ids.iter().any(|id| subject_set.contains(id));
            if violated {
                let message = rule
                    .violation_message
                    .clone()
                    .unwrap_or_else(|| format!("互斥规则违反 [{}]", rule.code));
                return Err(IamError::RuleViolation {
                    rule_code: rule.code.clone(),
                    message,
                });
            }
        }
        Ok(())
    }
}

/// 加载的互斥规则数据
struct LoadedRule {
    #[allow(dead_code)]
    id: String,
    code: String,
    #[allow(dead_code)]
    name: String,
    /// 对象类型：permission | role
    subject_type: String,
    /// 主对象ID（权限ID或角色ID）
    primary_subject_id: String,
    violation_message: Option<String>,
    /// 互斥对象ID集合
    excluded_ids: HashSet<String>,
}

#[async_trait]
impl RuleEnforcer for RuleEnforcerImpl {
    async fn check_role_permissions(&self, permission_ids: &[String]) -> Result<(), IamError> {
        if !self.config.enable_sod_check {
            return Ok(());
        }

        debug!(
            "{:<12} - RuleEnforcer::check_role_permissions - count: {}",
            "IAM-RULE",
            permission_ids.len()
        );

        let rules = self.load_active_rules(Some("permission")).await?;
        let perm_set: HashSet<String> = permission_ids.iter().cloned().collect();

        // 角色层仅校验功能权限互斥
        Self::check_exclusion(&rules, &perm_set)
    }

    async fn check_user_roles(&self, user_id: &str, role_ids: &[String]) -> Result<(), IamError> {
        if !self.config.enable_sod_check {
            return Ok(());
        }

        debug!(
            "{:<12} - RuleEnforcer::check_user_roles - user: {}, role_count: {}",
            "IAM-RULE",
            user_id,
            role_ids.len()
        );

        // 一次性加载全部规则，按 subject_type 分区
        let rules = self.load_active_rules(None).await?;
        let (perm_rules, role_rules): (Vec<LoadedRule>, Vec<LoadedRule>) = rules
            .into_iter()
            .partition(|r| r.subject_type == "permission");

        // 权限级校验：合并已有权限 + 待分配角色权限（两个查询相互独立，并发执行）
        if !perm_rules.is_empty() {
            let (existing_perms, new_perms) = tokio::try_join!(
                self.get_user_permission_ids(user_id),
                self.get_role_permission_ids(role_ids),
            )?;
            let mut perm_set = existing_perms;
            perm_set.extend(new_perms);
            Self::check_exclusion(&perm_rules, &perm_set)?;
        }

        // 角色级校验：合并已有角色 + 待分配角色
        if !role_rules.is_empty() {
            let existing_roles = self.get_user_role_ids(user_id).await?;
            let mut role_set = existing_roles;
            role_set.extend(role_ids.iter().cloned());
            Self::check_exclusion(&role_rules, &role_set)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 构造一条 LoadedRule 用于测试。
    fn make_rule(
        code: &str,
        primary: &str,
        excluded: &[&str],
        message: Option<&str>,
    ) -> LoadedRule {
        LoadedRule {
            id: format!("rule-{}", code),
            code: code.to_string(),
            name: format!("rule-{}", code),
            subject_type: "permission".to_string(),
            primary_subject_id: primary.to_string(),
            violation_message: message.map(|s| s.to_string()),
            excluded_ids: excluded.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 无规则时不拦截（验证安全启用：规则表为空时校验直接通过）
    #[test]
    fn test_no_rules_no_violation() {
        let rules: Vec<LoadedRule> = vec![];
        let set: HashSet<String> = ["p1".to_string(), "p2".to_string()].into_iter().collect();
        assert!(RuleEnforcerImpl::check_exclusion(&rules, &set).is_ok());
    }

    /// 有互斥规则时，同时包含主对象和任一互斥对象则违反
    #[test]
    fn test_primary_and_excluded_in_set_violation() {
        let rules = vec![make_rule("R1", "p1", &["p2", "p3"], None)];
        let set: HashSet<String> = ["p1".to_string(), "p2".to_string()].into_iter().collect();
        let err = RuleEnforcerImpl::check_exclusion(&rules, &set).unwrap_err();
        match err {
            IamError::RuleViolation { rule_code, message } => {
                assert_eq!(rule_code, "R1");
                assert!(message.contains("R1"));
            }
            other => panic!("期望 RuleViolation，实际: {:?}", other),
        }
    }

    /// 不包含互斥对象则通过（包含主对象但不含任何互斥对象）
    #[test]
    fn test_primary_without_excluded_passes() {
        let rules = vec![make_rule("R1", "p1", &["p2", "p3"], None)];
        // 含主对象 p1，但只含无关权限 p4
        let set: HashSet<String> = ["p1".to_string(), "p4".to_string()].into_iter().collect();
        assert!(RuleEnforcerImpl::check_exclusion(&rules, &set).is_ok());
    }

    /// 主对象不在集合中则不违反（即使含互斥对象）
    #[test]
    fn test_excluded_without_primary_no_violation() {
        let rules = vec![make_rule("R1", "p1", &["p2", "p3"], None)];
        // 不含主对象 p1，仅含互斥对象 p2（互斥对象之间不互斥）
        let set: HashSet<String> = ["p2".to_string(), "p3".to_string()].into_iter().collect();
        assert!(RuleEnforcerImpl::check_exclusion(&rules, &set).is_ok());
    }

    /// 多个互斥对象时，任一匹配即违反
    #[test]
    fn test_multiple_excluded_any_match_violation() {
        let rules = vec![make_rule("R1", "p1", &["p2", "p3", "p4"], None)];
        // 主对象 p1 + 互斥对象 p4
        let set: HashSet<String> = ["p1".to_string(), "p4".to_string()].into_iter().collect();
        assert!(matches!(
            RuleEnforcerImpl::check_exclusion(&rules, &set).unwrap_err(),
            IamError::RuleViolation { .. }
        ));
    }

    /// 自定义违规消息被使用
    #[test]
    fn test_custom_violation_message_used() {
        let rules = vec![make_rule(
            "R1",
            "p1",
            &["p2"],
            Some("不能同时持有付款和审批权限"),
        )];
        let set: HashSet<String> = ["p1".to_string(), "p2".to_string()].into_iter().collect();
        let err = RuleEnforcerImpl::check_exclusion(&rules, &set).unwrap_err();
        match err {
            IamError::RuleViolation { message, .. } => {
                assert_eq!(message, "不能同时持有付款和审批权限");
            }
            other => panic!("期望 RuleViolation，实际: {:?}", other),
        }
    }

    /// 无自定义消息时使用默认格式
    #[test]
    fn test_default_message_when_no_custom() {
        let rules = vec![make_rule("R1", "p1", &["p2"], None)];
        let set: HashSet<String> = ["p1".to_string(), "p2".to_string()].into_iter().collect();
        let err = RuleEnforcerImpl::check_exclusion(&rules, &set).unwrap_err();
        match err {
            IamError::RuleViolation { rule_code, message } => {
                assert_eq!(rule_code, "R1");
                assert_eq!(message, "互斥规则违反 [R1]");
            }
            other => panic!("期望 RuleViolation，实际: {:?}", other),
        }
    }

    /// 多规则时，第一个违反的规则被返回（按顺序遍历）
    #[test]
    fn test_multiple_rules_first_violation_returned() {
        let rules = vec![
            make_rule("R1", "p1", &["p2"], None),
            make_rule("R2", "p3", &["p4"], None),
        ];
        // 同时违反 R1 和 R2
        let set: HashSet<String> = ["p1", "p2", "p3", "p4"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let err = RuleEnforcerImpl::check_exclusion(&rules, &set).unwrap_err();
        match err {
            IamError::RuleViolation { rule_code, .. } => {
                assert_eq!(rule_code, "R1", "应返回第一个违反的规则");
            }
            other => panic!("期望 RuleViolation，实际: {:?}", other),
        }
    }

    /// 多规则都不违反时通过
    #[test]
    fn test_multiple_rules_none_violated() {
        let rules = vec![
            make_rule("R1", "p1", &["p2"], None),
            make_rule("R2", "p3", &["p4"], None),
        ];
        // 仅含主对象，不含任一互斥对象
        let set: HashSet<String> = ["p1".to_string(), "p3".to_string()].into_iter().collect();
        assert!(RuleEnforcerImpl::check_exclusion(&rules, &set).is_ok());
    }

    /// 空集合永不违反（即使有规则，无主对象则不触发）
    #[test]
    fn test_empty_subject_set_no_violation() {
        let rules = vec![make_rule("R1", "p1", &["p2"], None)];
        let set: HashSet<String> = HashSet::new();
        assert!(RuleEnforcerImpl::check_exclusion(&rules, &set).is_ok());
    }
}
