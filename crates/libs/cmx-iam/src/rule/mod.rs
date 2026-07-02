//! 互斥规则模块（功能权限互斥 + 角色互斥）
//!
//! 提供互斥规则的 CRUD、启用/禁用、规则项管理、校验测试等功能。

pub mod bmc;
pub mod enforcer;
pub mod entity;
pub mod service;

pub use bmc::{ExclusionRuleBmc, ExclusionRuleItemBmc};
pub use enforcer::{RuleEnforcer, RuleEnforcerImpl};
pub use entity::{
    CreateExclusionRuleRequest, ExclusionRule, ExclusionRuleItem, RuleViolationDetail, SubjectType,
    UpdateExclusionRuleRequest, ValidateRuleRequest, ValidateRuleResponse,
};
pub use service::{ExclusionRuleService, ExclusionRuleServiceImpl};
