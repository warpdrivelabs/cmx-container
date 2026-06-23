//! 互斥规则 Entity 定义

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 对象类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubjectType {
    /// 功能权限互斥
    #[serde(rename = "permission")]
    Permission,
    /// 角色互斥
    #[serde(rename = "role")]
    Role,
}

impl SubjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubjectType::Permission => "permission",
            SubjectType::Role => "role",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "permission" => Some(SubjectType::Permission),
            "role" => Some(SubjectType::Role),
            _ => None,
        }
    }
}

/// 互斥规则记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExclusionRule {
    pub id: String,
    pub code: String,
    pub name: String,
    /// 对象类型：permission-功能权限互斥，role-角色互斥
    pub subject_type: String,
    /// 主要对象ID（权限ID或角色ID）
    pub primary_subject_id: String,
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

/// 互斥对象明细记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExclusionRuleItem {
    pub id: String,
    pub rule_id: String,
    /// 互斥对象ID（权限ID或角色ID）
    pub subject_id: String,
}

/// 创建互斥规则请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateExclusionRuleRequest {
    pub code: String,
    pub name: String,
    /// 对象类型：permission | role
    pub subject_type: String,
    /// 主要对象ID（权限ID或角色ID）
    pub primary_subject_id: String,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    /// 互斥对象ID列表
    pub excluded_subject_ids: Vec<String>,
}

/// 更新规则请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateExclusionRuleRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// 修改主对象（需校验不在现有互斥列表中）
    #[serde(default)]
    pub primary_subject_id: Option<String>,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<i64>,
}

/// 规则校验测试请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleRequest {
    /// 待校验的权限组合
    #[serde(default)]
    pub permission_ids: Vec<String>,
    /// 待校验的角色组合
    #[serde(default)]
    pub role_ids: Vec<String>,
    /// 可选：模拟合并到指定用户的现有权限/角色
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
    pub subject_type: String,
    pub violation_message: String,
    pub primary_subject_id: String,
    /// 触发冲突的互斥对象ID
    pub conflicting_subject_id: String,
}

/// 规则校验测试响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleResponse {
    pub passed: bool,
    pub violations: Vec<RuleViolationDetail>,
}
