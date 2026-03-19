//! 依赖关系模块
//! 
//! 定义依赖关系、依赖图、依赖解析

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

use super::version::{SemanticVersion, VersionConstraint};

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

/// 依赖图
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// 节点列表
    nodes: HashMap<String, DependencyNode>,
    /// 边列表（依赖关系）
    edges: HashMap<String, Vec<String>>,
}

/// 依赖节点
#[derive(Debug, Clone)]
pub struct DependencyNode {
    /// 插件ID
    pub plugin_id: String,
    /// 版本
    pub version: SemanticVersion,
    /// 依赖列表
    pub dependencies: Vec<Dependency>,
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

impl DependencyGraph {
    /// 创建新的依赖图
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
        }
    }
    
    /// 添加节点
    pub fn add_node(&mut self, node: DependencyNode) {
        let plugin_id = node.plugin_id.clone();
        let deps: Vec<String> = node.dependencies.iter()
            .map(|d| d.plugin_id.clone())
            .collect();
        self.nodes.insert(plugin_id.clone(), node);
        self.edges.insert(plugin_id, deps);
    }
    
    /// 拓扑排序
    pub fn topological_sort(&self) -> Result<Vec<String>, String> {
        let mut visited = HashSet::new();
        let mut temp_mark = HashSet::new();
        let mut result = Vec::new();
        
        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                self.visit(node_id, &mut visited, &mut temp_mark, &mut result)?;
            }
        }
        
        result.reverse();
        Ok(result)
    }
    
    fn visit(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        temp_mark: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), String> {
        if temp_mark.contains(node_id) {
            return Err(format!("检测到循环依赖: {}", node_id));
        }
        
        if visited.contains(node_id) {
            return Ok(());
        }
        
        temp_mark.insert(node_id.to_string());
        
        if let Some(deps) = self.edges.get(node_id) {
            for dep_id in deps {
                self.visit(dep_id, visited, temp_mark, result)?;
            }
        }
        
        temp_mark.remove(node_id);
        visited.insert(node_id.to_string());
        result.push(node_id.to_string());
        
        Ok(())
    }
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
