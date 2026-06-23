//! 互斥规则 Entity 定义。
//!
//! 定义互斥规则（Exclusion Rule）相关的数据传输对象，
//! 包括规则主体、明细项、创建/更新请求以及规则校验请求/响应。
//! 用于 SoD（职责分离）场景下的权限与角色互斥约束。

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 互斥对象类型。
///
/// 区分互斥规则作用的对象类别：功能权限或角色。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubjectType {
    /// 功能权限互斥。
    #[serde(rename = "permission")]
    Permission,
    /// 角色互斥。
    #[serde(rename = "role")]
    Role,
}

impl SubjectType {
    /// 返回对象类型的字符串表示。
    ///
    /// # Returns
    ///
    /// * `"permission"` - 当对象类型为 `Permission` 时。
    /// * `"role"` - 当对象类型为 `Role` 时。
    pub fn as_str(&self) -> &'static str {
        match self {
            SubjectType::Permission => "permission",
            SubjectType::Role => "role",
        }
    }

    /// 从字符串解析对象类型。
    ///
    /// # Arguments
    ///
    /// * `s` - 待解析的字符串，取值为 `"permission"` 或 `"role"`。
    ///
    /// # Returns
    ///
    /// * `Some(SubjectType)` - 当字符串可识别时返回对应变体。
    /// * `None` - 当字符串无法识别时返回。
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "permission" => Some(SubjectType::Permission),
            "role" => Some(SubjectType::Role),
            _ => None,
        }
    }
}

/// 互斥规则记录。
///
/// 对应数据库 `cmx_exclusion_rule` 表的一行，描述一条完整的互斥规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExclusionRule {
    /// 规则唯一标识。
    pub id: String,
    /// 规则编码，业务唯一。
    pub code: String,
    /// 规则名称。
    pub name: String,
    /// 对象类型：`permission`-功能权限互斥，`role`-角色互斥。
    pub subject_type: String,
    /// 主要对象 ID（权限 ID 或角色 ID）。
    pub primary_subject_id: String,
    /// 违规提示消息。
    pub violation_message: Option<String>,
    /// 优先级，数值越大优先级越高。
    pub priority: i64,
    /// 规则描述。
    pub description: Option<String>,
    /// 状态：`1`-启用，`0`-禁用。
    pub status: i64,
    /// 归档标记：`1`-已归档，`0`-未归档。
    pub archived: i64,
    /// 创建时间。
    pub create_time: chrono::DateTime<chrono::Utc>,
    /// 更新时间。
    pub update_time: chrono::DateTime<chrono::Utc>,
    /// 创建人 ID。
    pub create_by: Option<String>,
    /// 创建人名称。
    pub create_name: Option<String>,
    /// 更新人 ID。
    pub update_by: Option<String>,
    /// 更新人名称。
    pub update_name: Option<String>,
}

/// 互斥对象明细记录。
///
/// 对应数据库 `cmx_exclusion_rule_item` 表的一行，描述规则下的单个互斥对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExclusionRuleItem {
    /// 明细唯一标识。
    pub id: String,
    /// 所属规则 ID。
    pub rule_id: String,
    /// 互斥对象 ID（权限 ID 或角色 ID）。
    pub subject_id: String,
}

/// 创建互斥规则请求。
///
/// 用于创建一条新的互斥规则及其关联的互斥对象明细。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateExclusionRuleRequest {
    /// 规则编码，业务唯一。
    pub code: String,
    /// 规则名称。
    pub name: String,
    /// 对象类型：`permission` 或 `role`。
    pub subject_type: String,
    /// 主要对象 ID（权限 ID 或角色 ID）。
    pub primary_subject_id: String,
    #[serde(default)]
    pub violation_message: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    /// 互斥对象 ID 列表。
    pub excluded_subject_ids: Vec<String>,
}

/// 更新规则请求。
///
/// 所有字段均为可选，仅更新提供的字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateExclusionRuleRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// 修改主对象（需校验不在现有互斥列表中）。
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

/// 规则校验测试请求。
///
/// 用于在不实际分配权限/角色的情况下，预先校验组合是否违反互斥规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleRequest {
    /// 待校验的权限组合。
    #[serde(default)]
    pub permission_ids: Vec<String>,
    /// 待校验的角色组合。
    #[serde(default)]
    pub role_ids: Vec<String>,
    /// 可选：模拟合并到指定用户的现有权限/角色。
    #[serde(default)]
    pub user_id: Option<String>,
}

/// 规则违反详情。
///
/// 描述一次规则校验中检测到的具体违反情况。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RuleViolationDetail {
    /// 违反的规则 ID。
    pub rule_id: String,
    /// 违反的规则编码。
    pub rule_code: String,
    /// 违反的规则名称。
    pub rule_name: String,
    /// 对象类型：`permission` 或 `role`。
    pub subject_type: String,
    /// 违规提示消息。
    pub violation_message: String,
    /// 主要对象 ID。
    pub primary_subject_id: String,
    /// 触发冲突的互斥对象 ID。
    pub conflicting_subject_id: String,
}

/// 规则校验测试响应。
///
/// 返回规则校验的最终结果与所有违反详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ValidateRuleResponse {
    /// 是否通过校验：`true`-通过，`false`-存在违反。
    pub passed: bool,
    /// 违反详情列表，通过校验时为空。
    pub violations: Vec<RuleViolationDetail>,
}
