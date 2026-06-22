//! 权限规则模块（互斥/依赖）
//!
//! 提供权限规则的 CRUD、启用/禁用、规则项管理、校验测试等功能。

pub mod bmc;
pub mod enforcer;
pub mod entity;
pub mod service;

pub use bmc::{PermissionRuleBmc, PermissionRuleItemBmc};
pub use enforcer::{RuleEnforcer, RuleEnforcerImpl};
pub use entity::{
    CreatePermissionRuleRequest, PermissionRule, PermissionRuleForCreate, PermissionRuleForUpdate,
    PermissionRuleItem, PermissionRuleItemForCreate, RuleItemInput, RuleType, RuleViolationDetail,
    ValidateRuleRequest, ValidateRuleResponse,
};
pub use service::PermissionRuleServiceImpl;
