//! 权限规则 Entity 定义

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 规则类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleType {
    /// 互斥规则
    #[serde(rename = "mutual_exclusion")]
    MutualExclusion,
    /// 依赖规则
    #[serde(rename = "dependency")]
    Dependency,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::MutualExclusion => "mutual_exclusion",
            RuleType::Dependency => "dependency",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "mutual_exclusion" => Some(RuleType::MutualExclusion),
            "dependency" => Some(RuleType::Dependency),
            _ => None,
        }
    }
}

/// 权限规则记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionRule {
    pub id: String,
    pub code: String,
    pub name: String,
    pub rule_type: String,
    pub violation_message: Option<String>,
    pub priority: i64,
    pub description: Option<String>,
    pub status: i64,
    pub archived: i64,
    pub create_time: chrono::DateTime<chrono::Utc>,
    pub update_time: chrono::DateTime<chrono::Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 规则权限项记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionRuleItem {
    pub id: String,
    pub rule_id: String,
    pub group_seq: i64,
    pub permission_id: String,
}

/// 创建规则 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionRuleForCreate {
    pub code: String,
    pub name: String,
    pub rule_type: String,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// 更新规则 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionRuleForUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 规则项输入（用于创建/更新规则时附带）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RuleItemInput {
    /// 组序号：互斥规则固定为 1；依赖规则用 1（前置）和 2（后置）
    pub group_seq: i64,
    pub permission_id: String,
}

/// 规则项创建 DTO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleItemForCreate {
    pub rule_id: String,
    pub group_seq: i64,
    pub permission_id: String,
}

/// 创建规则请求（含规则项列表）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreatePermissionRuleRequest {
    pub code: String,
    pub name: String,
    pub rule_type: String,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub items: Vec<RuleItemInput>,
}

/// 规则校验测试请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleRequest {
    pub permission_ids: Vec<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

/// 规则违反详情
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RuleViolationDetail {
    pub rule_id: String,
    pub rule_code: String,
    pub rule_name: String,
    pub rule_type: String,
    pub violation_message: String,
    pub conflicting_permission_ids: Vec<String>,
}

/// 规则校验测试响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleResponse {
    pub passed: bool,
    pub violations: Vec<RuleViolationDetail>,
}
