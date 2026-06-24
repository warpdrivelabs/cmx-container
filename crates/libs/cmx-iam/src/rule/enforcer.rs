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
use cmx_database::DatabaseManager;
use cmx_core::model::cell::DataValue;
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
    async fn check_role_permissions(
        &self,
        permission_ids: &[String],
    ) -> Result<(), IamError>;

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
    async fn check_user_roles(
        &self,
        user_id: &str,
        role_ids: &[String],
    ) -> Result<(), IamError>;
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
                    subject_type: row
                        .get_by_name_as(schema, "subject_type")
                        .unwrap_or_default(),
                    primary_subject_id: row
                        .get_by_name_as(schema, "primary_subject_id")
                        .unwrap_or_default(),
                    violation_message: row
                        .get_by_name_as(schema, "violation_message"),
                    excluded_ids: HashSet::new(),
                }
            });

            let subject_id: String = row
                .get_by_name_as(schema, "subject_id")
                .unwrap_or_default();
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
    async fn get_role_permission_ids(&self, role_ids: &[String]) -> Result<HashSet<String>, IamError> {
        if role_ids.is_empty() {
            return Ok(HashSet::new());
        }
        // 单次查询所有角色的权限ID（避免 N+1）
        let sql = r#"
            SELECT permission_id FROM cmx_role_permission
            WHERE role_id = ANY($1) AND archived = 0
        "#;
        let role_id_array = DataValue::Array(
            role_ids.iter().map(|id| DataValue::String(id.clone())).collect(),
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
            let violated = rule
                .excluded_ids
                .iter()
                .any(|id| subject_set.contains(id));
            if violated {
                let message = rule.violation_message.clone().unwrap_or_else(|| {
                    format!("互斥规则违反 [{}]", rule.code)
                });
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

        let rules = self.load_active_rules(Some("permission")).await?;
        let perm_set: HashSet<String> = permission_ids.iter().cloned().collect();

        // 角色层仅校验功能权限互斥
        Self::check_exclusion(&rules, &perm_set)
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

        // 一次性加载全部规则，按 subject_type 分区
        let rules = self.load_active_rules(None).await?;
        let (perm_rules, role_rules): (Vec<LoadedRule>, Vec<LoadedRule>) = rules
            .into_iter()
            .partition(|r| r.subject_type == "permission");

        // 权限级校验：合并已有权限 + 待分配角色权限
        if !perm_rules.is_empty() {
            let existing_perms = self.get_user_permission_ids(user_id).await?;
            let new_perms = self.get_role_permission_ids(role_ids).await?;
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
