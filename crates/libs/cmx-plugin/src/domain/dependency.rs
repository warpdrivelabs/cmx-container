//! 依赖关系模块
//!
//! 定义依赖关系、依赖解析

use serde::{Deserialize, Serialize};

use super::version::VersionConstraint;

/// 依赖检查结果
#[derive(Debug, Clone)]
pub struct DependencyCheckResult {
    /// 是否满足所有依赖
    pub satisfied: bool,
    /// 缺失的依赖
    pub missing: Vec<MissingDependency>,
    /// 冲突的依赖
    pub conflicts: Vec<DependencyConflict>,
}

impl DependencyCheckResult {
    /// 创建新的依赖检查结果
    pub fn new() -> Self {
        Self {
            satisfied: true,
            missing: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// 添加缺失的依赖
    pub fn add_missing(&mut self, missing: MissingDependency) {
        self.satisfied = false;
        self.missing.push(missing);
    }

    /// 添加冲突的依赖
    pub fn add_conflict(&mut self, conflict: DependencyConflict) {
        self.satisfied = false;
        self.conflicts.push(conflict);
    }

    /// 合并另一个检查结果
    pub fn merge(&mut self, other: DependencyCheckResult) {
        if !other.satisfied {
            self.satisfied = false;
        }
        self.missing.extend(other.missing);
        self.conflicts.extend(other.conflicts);
    }
}

impl Default for DependencyCheckResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 依赖解析结果
#[derive(Debug, Clone)]
pub struct DependencyResolution {
    /// 解析是否成功
    pub success: bool,
    /// 解析后的依赖顺序
    pub order: Vec<String>,
    /// 错误信息
    pub errors: Vec<String>,
}

impl DependencyResolution {
    /// 创建成功的解析结果
    pub fn success(order: Vec<String>) -> Self {
        Self {
            success: true,
            order,
            errors: Vec::new(),
        }
    }

    /// 创建失败的解析结果
    pub fn failure(errors: Vec<String>) -> Self {
        Self {
            success: false,
            order: Vec::new(),
            errors,
        }
    }
}

/// 依赖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// 依赖插件ID
    pub plugin_id: String,
    /// 版本约束
    pub version_constraint: Option<VersionConstraint>,
    /// 是否可选
    pub optional: bool,
}

/// 缺失的依赖
#[derive(Debug, Clone)]
pub struct MissingDependency {
    /// 依赖插件ID
    pub plugin_id: String,
    /// 版本约束
    pub version_constraint: Option<VersionConstraint>,
    /// 被哪个插件依赖
    pub required_by: String,
}

/// 依赖冲突
#[derive(Debug, Clone)]
pub struct DependencyConflict {
    /// 依赖插件ID
    pub plugin_id: String,
    /// 冲突的版本约束列表
    pub constraints: Vec<(String, VersionConstraint)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::version::{SemanticVersion, VersionRelation};

    // ==================== DependencyCheckResult ====================

    #[test]
    fn test_dependency_check_result_new_is_satisfied() {
        let r = DependencyCheckResult::new();
        assert!(r.satisfied);
        assert!(r.missing.is_empty());
        assert!(r.conflicts.is_empty());
    }

    #[test]
    fn test_dependency_check_result_default_is_satisfied() {
        let r = DependencyCheckResult::default();
        assert!(r.satisfied);
    }

    #[test]
    fn test_add_missing_marks_unsatisfied() {
        let mut r = DependencyCheckResult::new();
        let missing = MissingDependency {
            plugin_id: "dep-1".to_string(),
            version_constraint: None,
            required_by: "root".to_string(),
        };
        r.add_missing(missing);
        assert!(!r.satisfied, "添加缺失依赖后应标记为未满足");
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].plugin_id, "dep-1");
        assert_eq!(r.missing[0].required_by, "root");
    }

    #[test]
    fn test_add_conflict_marks_unsatisfied() {
        let mut r = DependencyCheckResult::new();
        let conflict = DependencyConflict {
            plugin_id: "dep-1".to_string(),
            constraints: vec![(
                "root".to_string(),
                VersionConstraint::new(
                    VersionRelation::GreaterThanOrEqual,
                    SemanticVersion::new(1, 0, 0),
                ),
            )],
        };
        r.add_conflict(conflict);
        assert!(!r.satisfied, "添加依赖冲突后应标记为未满足");
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].plugin_id, "dep-1");
    }

    #[test]
    fn test_merge_unsatisfied_propagates() {
        let mut a = DependencyCheckResult::new();
        let mut b = DependencyCheckResult::new();
        b.add_missing(MissingDependency {
            plugin_id: "dep".to_string(),
            version_constraint: None,
            required_by: "root".to_string(),
        });
        a.merge(b);
        assert!(!a.satisfied, "合并未满足结果后应标记为未满足");
        assert_eq!(a.missing.len(), 1);
    }

    #[test]
    fn test_merge_combines_missing_and_conflicts() {
        let mut a = DependencyCheckResult::new();
        a.add_missing(MissingDependency {
            plugin_id: "dep-a".to_string(),
            version_constraint: None,
            required_by: "root".to_string(),
        });

        let mut b = DependencyCheckResult::new();
        b.add_conflict(DependencyConflict {
            plugin_id: "dep-b".to_string(),
            constraints: vec![],
        });

        a.merge(b);
        assert!(!a.satisfied);
        assert_eq!(a.missing.len(), 1);
        assert_eq!(a.conflicts.len(), 1);
        assert_eq!(a.missing[0].plugin_id, "dep-a");
        assert_eq!(a.conflicts[0].plugin_id, "dep-b");
    }

    #[test]
    fn test_merge_two_satisfied_remains_satisfied() {
        let mut a = DependencyCheckResult::new();
        let b = DependencyCheckResult::new();
        a.merge(b);
        assert!(a.satisfied, "合并两个已满足结果应保持已满足状态");
        assert!(a.missing.is_empty());
        assert!(a.conflicts.is_empty());
    }

    #[test]
    fn test_add_multiple_missing_and_conflicts() {
        let mut r = DependencyCheckResult::new();
        for i in 0..3 {
            r.add_missing(MissingDependency {
                plugin_id: format!("dep-{}", i),
                version_constraint: None,
                required_by: "root".to_string(),
            });
        }
        for i in 0..2 {
            r.add_conflict(DependencyConflict {
                plugin_id: format!("conflict-{}", i),
                constraints: vec![],
            });
        }
        assert_eq!(r.missing.len(), 3);
        assert_eq!(r.conflicts.len(), 2);
        assert!(!r.satisfied);
    }

    // ==================== DependencyResolution ====================

    #[test]
    fn test_dependency_resolution_success() {
        let order = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let r = DependencyResolution::success(order.clone());
        assert!(r.success);
        assert_eq!(r.order, order);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn test_dependency_resolution_failure() {
        let errors = vec!["cycle detected".to_string(), "missing dep".to_string()];
        let r = DependencyResolution::failure(errors.clone());
        assert!(!r.success);
        assert!(r.order.is_empty());
        assert_eq!(r.errors, errors);
    }

    #[test]
    fn test_dependency_resolution_success_empty_order() {
        // 空依赖列表应解析为成功且空顺序
        let r = DependencyResolution::success(vec![]);
        assert!(r.success);
        assert!(r.order.is_empty());
    }

    #[test]
    fn test_dependency_resolution_failure_no_errors() {
        let r = DependencyResolution::failure(vec![]);
        assert!(!r.success);
        assert!(r.errors.is_empty());
    }

    // ==================== Dependency / MissingDependency / DependencyConflict ====================

    #[test]
    fn test_dependency_with_version_constraint() {
        let dep = Dependency {
            plugin_id: "dep".to_string(),
            version_constraint: Some(VersionConstraint::new(
                VersionRelation::Equal,
                SemanticVersion::new(1, 2, 3),
            )),
            optional: false,
        };
        assert_eq!(dep.plugin_id, "dep");
        assert!(!dep.optional);
        let constraint = dep.version_constraint.unwrap();
        assert_eq!(constraint.relation, VersionRelation::Equal);
        assert_eq!(constraint.version, SemanticVersion::new(1, 2, 3));
    }

    #[test]
    fn test_dependency_optional_default() {
        let dep = Dependency {
            plugin_id: "opt".to_string(),
            version_constraint: None,
            optional: true,
        };
        assert!(dep.optional);
        assert!(dep.version_constraint.is_none());
    }

    #[test]
    fn test_missing_dependency_with_constraint() {
        let missing = MissingDependency {
            plugin_id: "dep".to_string(),
            version_constraint: Some(VersionConstraint::new(
                VersionRelation::Compatible,
                SemanticVersion::new(1, 0, 0),
            )),
            required_by: "root-plugin".to_string(),
        };
        assert_eq!(missing.required_by, "root-plugin");
        let constraint = missing.version_constraint.unwrap();
        assert_eq!(constraint.relation, VersionRelation::Compatible);
    }
}
