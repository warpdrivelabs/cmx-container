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
