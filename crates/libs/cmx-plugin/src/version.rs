//! 语义版本管理模块

use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::error::PluginError;
use crate::types::{
    BreakingChange, CompatibilityLevel, CompatibilityResult, DependencyCheckResult,
    DependencyConflict, DependencyGraph, DependencyResolution, DepNode, DepEdge,
    MissingDependency, ResolutionStatus, UpgradePath, UpgradeStep, VersionRelation,
};

/// 预发布版本
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreRelease {
    Alpha(String),
    Beta(String),
    Rc(String),
    Number(u32),
}

/// 语义版本结构
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<PreRelease>,
    pub build: Option<String>,
}

impl SemanticVersion {
    /// 解析版本字符串
    pub fn parse(version: &str) -> Result<Self, PluginError> {
        let version = version.trim();
        
        let re = Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:-([a-zA-Z0-9.]+))?(?:\+([a-zA-Z0-9.]+))?$")
            .map_err(|e| PluginError::Version(format!("正则编译错误: {}", e)))?;
        
        let caps = re.captures(version)
            .ok_or_else(|| PluginError::Version(format!("无效的版本格式: {}", version)))?;
        
        let major: u32 = caps.get(1).unwrap().as_str().parse()
            .map_err(|_| PluginError::Version("主版本号解析失败".to_string()))?;
        let minor: u32 = caps.get(2).unwrap().as_str().parse()
            .map_err(|_| PluginError::Version("次版本号解析失败".to_string()))?;
        let patch: u32 = caps.get(3).unwrap().as_str().parse()
            .map_err(|_| PluginError::Version("修订版本号解析失败".to_string()))?;
        
        let pre = caps.get(4).map(|m| {
            let pre_str = m.as_str();
            if pre_str.starts_with("alpha.") {
                PreRelease::Alpha(pre_str[6..].to_string())
            } else if pre_str.starts_with("beta.") {
                PreRelease::Beta(pre_str[5..].to_string())
            } else if pre_str.starts_with("rc.") {
                PreRelease::Rc(pre_str[3..].to_string())
            } else {
                PreRelease::Number(pre_str.parse().unwrap_or(0))
            }
        });
        
        let build = caps.get(5).map(|m| m.as_str().to_string());
        
        Ok(SemanticVersion { major, minor, patch, pre, build })
    }
    
    /// 转换为字符串
    pub fn to_string(&self) -> String {
        let mut version = format!("{}.{}.{}", self.major, self.minor, self.patch);
        
        if let Some(pre) = &self.pre {
            version.push('-');
            match pre {
                PreRelease::Alpha(s) => version.push_str(&format!("alpha.{}", s)),
                PreRelease::Beta(s) => version.push_str(&format!("beta.{}", s)),
                PreRelease::Rc(s) => version.push_str(&format!("rc.{}", s)),
                PreRelease::Number(n) => version.push_str(&n.to_string()),
            }
        }
        
        if let Some(build) = &self.build {
            version.push('+');
            version.push_str(build);
        }
        
        version
    }
    
    /// 比较版本
    pub fn cmp(&self, other: &SemanticVersion) -> VersionRelation {
        if self.major != other.major {
            return if self.major > other.major {
                VersionRelation::Greater
            } else {
                VersionRelation::Less
            };
        }
        
        if self.minor != other.minor {
            return if self.minor > other.minor {
                VersionRelation::Greater
            } else {
                VersionRelation::Less
            };
        }
        
        if self.patch != other.patch {
            return if self.patch > other.patch {
                VersionRelation::Greater
            } else {
                VersionRelation::Less
            };
        }
        
        // 比较预发布版本
        match (&self.pre, &other.pre) {
            (None, None) => VersionRelation::Equal,
            (None, Some(_)) => VersionRelation::Greater,
            (Some(_), None) => VersionRelation::Less,
            (Some(p1), Some(p2)) => {
                if p1 == p2 {
                    VersionRelation::Equal
                } else {
                    let precedence = |pre: &PreRelease| match pre {
                        PreRelease::Alpha(_) => 1,
                        PreRelease::Beta(_) => 2,
                        PreRelease::Rc(_) => 3,
                        PreRelease::Number(_) => 4,
                    };
                    if precedence(p1) > precedence(p2) {
                        VersionRelation::Greater
                    } else {
                        VersionRelation::Less
                    }
                }
            }
        }
    }
    
    /// 是否是稳定版本
    pub fn is_stable(&self) -> bool {
        self.pre.is_none()
    }
}

/// 版本管理器
pub struct VersionManager;

impl VersionManager {
    /// 解析版本字符串
    pub fn parse_version(version: &str) -> Result<SemanticVersion, PluginError> {
        SemanticVersion::parse(version)
    }
    
    /// 比较两个版本
    pub fn compare(v1: &str, v2: &str) -> Result<VersionRelation, PluginError> {
        let v1 = SemanticVersion::parse(v1)?;
        let v2 = SemanticVersion::parse(v2)?;
        Ok(v1.cmp(&v2))
    }
    
    /// 检查版本是否满足约束
    pub fn satisfies_constraint(version: &str, constraint: &str) -> bool {
        if let Ok(v) = SemanticVersion::parse(version) {
            Self::check_constraint(&v, constraint)
        } else {
            false
        }
    }
    
    /// 检查单个约束
    fn check_constraint(version: &SemanticVersion, constraint: &str) -> bool {
        let constraint = constraint.trim();
        
        if constraint.starts_with("^") {
            // Caret 约束 (^1.0.0) - 兼容更新
            if let Ok(base) = SemanticVersion::parse(&constraint[1..]) {
                return version.major == base.major
                    && (version.minor > base.minor 
                        || (version.minor == base.minor && version.patch >= base.patch));
            }
        } else if constraint.starts_with("~") {
            // Tilde 约束 (~1.0.0) - 补丁更新
            if let Ok(base) = SemanticVersion::parse(&constraint[1..]) {
                return version.major == base.major
                    && version.minor == base.minor
                    && version.patch >= base.patch;
            }
        } else if constraint.starts_with(">=") {
            if let Ok(base) = SemanticVersion::parse(&constraint[2..]) {
                return version.cmp(&base) != VersionRelation::Less;
            }
        } else if constraint.starts_with(">") {
            if let Ok(base) = SemanticVersion::parse(&constraint[1..]) {
                return version.cmp(&base) == VersionRelation::Greater;
            }
        } else if constraint.starts_with("<=") {
            if let Ok(base) = SemanticVersion::parse(&constraint[2..]) {
                return version.cmp(&base) != VersionRelation::Greater;
            }
        } else if constraint.starts_with("<") {
            if let Ok(base) = SemanticVersion::parse(&constraint[1..]) {
                return version.cmp(&base) == VersionRelation::Less;
            }
        } else if constraint.starts_with("=") {
            if let Ok(base) = SemanticVersion::parse(&constraint[1..]) {
                return version.cmp(&base) == VersionRelation::Equal;
            }
        } else {
            // 精确版本
            if let Ok(base) = SemanticVersion::parse(constraint) {
                return version.cmp(&base) == VersionRelation::Equal;
            }
        }
        
        false
    }
    
    /// 检查升级兼容性
    pub fn check_upgrade_compatibility(
        from: &str,
        to: &str,
    ) -> Result<CompatibilityResult, PluginError> {
        let from_v = SemanticVersion::parse(from)?;
        let to_v = SemanticVersion::parse(to)?;
        
        let level = if from_v.major == to_v.major {
            if from_v.minor == to_v.minor {
                CompatibilityLevel::FullyCompatible
            } else if to_v.minor > from_v.minor {
                CompatibilityLevel::BackwardCompatible
            } else {
                CompatibilityLevel::Incompatible
            }
        } else if to_v.major > from_v.major {
            CompatibilityLevel::ConditionallyCompatible
        } else {
            CompatibilityLevel::Incompatible
        };
        
        let mut breaking_changes = Vec::new();
        let mut warnings = Vec::new();
        
        if to_v.major > from_v.major {
            warnings.push("主版本升级可能包含破坏性变更".to_string());
        }
        
        Ok(CompatibilityResult {
            level,
            breaking_changes,
            warnings,
            migration_guide: None,
        })
    }
    
    /// 获取可用的升级路径
    pub fn get_upgrade_path(
        from: &str,
        available_versions: &[String],
    ) -> Result<Vec<UpgradePath>, PluginError> {
        let from_v = SemanticVersion::parse(from)?;
        let mut paths = Vec::new();
        
        for av in available_versions {
            if let Ok(av_v) = SemanticVersion::parse(av) {
                if av_v.cmp(&from_v) == VersionRelation::Greater {
                    let comp_result = Self::check_upgrade_compatibility(
                        &from_v.to_string(),
                        &av_v.to_string(),
                    )?;
                    
                    paths.push(UpgradePath {
                        from: from_v.to_string(),
                        to: av_v.to_string(),
                        steps: vec![UpgradeStep {
                            version: av_v.to_string(),
                            description: format!("升级到 {}", av_v.to_string()),
                        }],
                        is_safe: matches!(
                            comp_result.level,
                            CompatibilityLevel::FullyCompatible | CompatibilityLevel::BackwardCompatible
                        ),
                        warnings: comp_result.warnings,
                    });
                }
            }
        }
        
        paths.sort_by(|a, b| a.to.cmp(&b.to));
        Ok(paths)
    }
}

/// 依赖解析器
pub struct DependencyResolver;

impl DependencyResolver {
    /// 解析插件的所有依赖（递归）
    pub async fn resolve(
        plugin_id: &str,
        version: &str,
        dependencies: &[(String, String, String)],
    ) -> Result<DependencyResolution, PluginError> {
        let mut conflicts = Vec::new();
        let mut missing = Vec::new();
        
        // 构建依赖图
        let mut graph = DependencyGraph::default();
        graph.nodes.push(DepNode {
            plugin_id: plugin_id.to_string(),
            version: version.to_string(),
            is_root: true,
        });
        
        for (dep_id, dep_version, _constraint) in dependencies {
            graph.edges.push(DepEdge {
                from: plugin_id.to_string(),
                to: dep_id.clone(),
                constraint: dep_version.clone(),
            });
            graph.nodes.push(DepNode {
                plugin_id: dep_id.clone(),
                version: dep_version.clone(),
                is_root: false,
            });
        }
        
        // 检测循环依赖
        if let Some(cycle) = Self::detect_cycles(&graph) {
            return Err(PluginError::Dependency(format!(
                "检测到循环依赖: {}",
                cycle.join(" -> ")
            )));
        }
        
        // 简化处理：返回直接依赖
        let mut result = HashMap::new();
        for (dep_id, dep_version, _) in dependencies {
            result.insert(dep_id.clone(), dep_version.clone());
        }
        
        Ok(DependencyResolution {
            resolved: result,
            conflicts,
            missing,
        })
    }
    
    /// 检查依赖是否满足
    pub async fn check_dependencies(
        plugin_id: &str,
        dependencies: &[(String, String)],
        installed: &[(String, String)],
    ) -> DependencyCheckResult {
        let mut missing = Vec::new();
        let mut conflicts = Vec::new();
        
        let installed_map: HashSet<String> = installed.iter()
            .map(|(id, ver)| format!("{}:{}", id, ver))
            .collect();
        
        for (dep_id, required_version) in dependencies {
            let found = installed.iter()
                .find(|(id, _)| id == dep_id);
            
            match found {
                Some((_, installed_version)) => {
                    if !VersionManager::satisfies_constraint(installed_version, required_version) {
                        conflicts.push(DependencyConflict {
                            plugin_id: dep_id.clone(),
                            required_version: required_version.clone(),
                            existing_version: installed_version.clone(),
                        });
                    }
                }
                None => {
                    missing.push(MissingDependency {
                        plugin_id: dep_id.clone(),
                        constraint: required_version.clone(),
                    });
                }
            }
        }
        
        DependencyCheckResult {
            satisfied: missing.is_empty() && conflicts.is_empty(),
            missing,
            conflicts,
        }
    }
    
    /// 检测循环依赖 (使用 DFS)
    pub fn detect_cycles(graph: &DependencyGraph) -> Option<Vec<String>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut rec_stack: HashSet<String> = HashSet::new();
        
        for node in &graph.nodes {
            if !visited.contains(&node.plugin_id) {
                let mut path = Vec::new();
                if Self::detect_cycle_dfs(
                    &node.plugin_id,
                    graph,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                ) {
                    return Some(path);
                }
            }
        }
        
        None
    }
    
    fn detect_cycle_dfs(
        plugin_id: &str,
        graph: &DependencyGraph,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        visited.insert(plugin_id.to_string());
        rec_stack.insert(plugin_id.to_string());
        path.push(plugin_id.to_string());
        
        for edge in &graph.edges {
            if edge.from == plugin_id {
                let dep_id = &edge.to;
                if !visited.contains(dep_id) {
                    if Self::detect_cycle_dfs(dep_id, graph, visited, rec_stack, path) {
                        return true;
                    }
                } else if rec_stack.contains(dep_id) {
                    return true;
                }
            }
        }
        
        path.pop();
        rec_stack.remove(plugin_id);
        false
    }
    
    /// 解决依赖冲突
    pub async fn resolve_conflicts(
        requirements: &[(String, String)],
    ) -> Result<DependencyResolution, PluginError> {
        let mut resolved = HashMap::new();
        let mut conflicts = Vec::new();
        let mut missing = Vec::new();
        
        for (plugin_id, version) in requirements {
            resolved.insert(plugin_id.clone(), version.clone());
        }
        
        Ok(DependencyResolution {
            resolved,
            conflicts,
            missing,
        })
    }
}

/// 版本约束类型 - 支持复杂的版本约束表达式
#[derive(Debug, Clone, PartialEq)]
pub enum VersionConstraint {
    /// 精确版本 (=1.0.0)
    Exact(String),
    /// 范围版本 (>=1.0.0 <2.0.0)
    Range {
        min: Option<String>,
        max: Option<String>,
        min_exclusive: bool,
        max_exclusive: bool,
    },
    /// Caret 约束 (^1.0.0) - 允许兼容更新
    Caret(String),
    /// Tilde 约束 (~1.0.0) - 允许补丁更新
    Tilde(String),
    /// 通配符 (1.x, 1.2.x)
    Wildcard { major: u32, minor: Option<u32> },
    /// 或约束 (1.0.0 || 2.0.0)
    Or(Vec<VersionConstraint>),
    /// 与约束 (>=1.0.0 <2.0.0)
    And(Vec<VersionConstraint>),
}

impl VersionConstraint {
    /// 解析版本约束字符串
    pub fn parse(constraint: &str) -> Result<Self, PluginError> {
        let constraint = constraint.trim();
        
        // 处理或约束
        if constraint.contains("||") {
            let parts: Vec<&str> = constraint.split("||").collect();
            let constraints: Vec<VersionConstraint> = parts
                .iter()
                .map(|p| Self::parse(p.trim()))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(VersionConstraint::Or(constraints));
        }
        
        // 处理空格分隔的与约束 (>=1.0.0 <2.0.0)
        let parts: Vec<&str> = constraint.split_whitespace().collect();
        if parts.len() > 1 {
            let constraints: Vec<VersionConstraint> = parts
                .iter()
                .map(|p| Self::parse_single(p))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(VersionConstraint::And(constraints));
        }
        
        Self::parse_single(constraint)
    }
    
    /// 解析单个约束
    fn parse_single(constraint: &str) -> Result<Self, PluginError> {
        let constraint = constraint.trim();
        
        // Caret 约束 (^1.0.0)
        if constraint.starts_with('^') {
            return Ok(VersionConstraint::Caret(constraint[1..].to_string()));
        }
        
        // Tilde 约束 (~1.0.0)
        if constraint.starts_with('~') {
            return Ok(VersionConstraint::Tilde(constraint[1..].to_string()));
        }
        
        // 通配符 (1.x, 1.2.x, *)
        if constraint.ends_with(".x") || constraint.ends_with(".*") || constraint == "*" {
            let parts: Vec<&str> = constraint.trim_end_matches(".x").trim_end_matches(".*").split('.').collect();
            match parts.len() {
                1 if parts[0] == "*" => {
                    return Ok(VersionConstraint::Wildcard { major: 0, minor: None });
                }
                1 => {
                    let major = parts[0].parse().map_err(|_| {
                        PluginError::Version(format!("无效的通配符: {}", constraint))
                    })?;
                    return Ok(VersionConstraint::Wildcard { major, minor: None });
                }
                2 => {
                    let major = parts[0].parse().map_err(|_| {
                        PluginError::Version(format!("无效的通配符: {}", constraint))
                    })?;
                    let minor = parts[1].parse().ok();
                    return Ok(VersionConstraint::Wildcard { major, minor });
                }
                _ => {}
            }
        }
        
        // 范围约束
        if constraint.starts_with(">=") {
            let version = constraint[2..].trim().to_string();
            return Ok(VersionConstraint::Range {
                min: Some(version),
                max: None,
                min_exclusive: false,
                max_exclusive: false,
            });
        }
        
        if constraint.starts_with('>') && !constraint.starts_with(">=") {
            let version = constraint[1..].trim().to_string();
            return Ok(VersionConstraint::Range {
                min: Some(version),
                max: None,
                min_exclusive: true,
                max_exclusive: false,
            });
        }
        
        if constraint.starts_with("<=") {
            let version = constraint[2..].trim().to_string();
            return Ok(VersionConstraint::Range {
                min: None,
                max: Some(version),
                min_exclusive: false,
                max_exclusive: false,
            });
        }
        
        if constraint.starts_with('<') && !constraint.starts_with("<=") {
            let version = constraint[1..].trim().to_string();
            return Ok(VersionConstraint::Range {
                min: None,
                max: Some(version),
                min_exclusive: false,
                max_exclusive: true,
            });
        }
        
        // 精确版本
        if constraint.starts_with('=') {
            return Ok(VersionConstraint::Exact(constraint[1..].trim().to_string()));
        }
        
        // 默认为精确版本
        Ok(VersionConstraint::Exact(constraint.to_string()))
    }
    
    /// 检查版本是否满足约束
    pub fn satisfies(&self, version: &str) -> bool {
        let v = match SemanticVersion::parse(version) {
            Ok(v) => v,
            Err(_) => return false,
        };
        
        match self {
            VersionConstraint::Exact(exact) => {
                match SemanticVersion::parse(exact) {
                    Ok(exact_v) => v.cmp(&exact_v) == VersionRelation::Equal,
                    Err(_) => false,
                }
            }
            VersionConstraint::Range { min, max, min_exclusive, max_exclusive } => {
                let min_ok = match min {
                    Some(min_v) => match SemanticVersion::parse(min_v) {
                        Ok(min_v) => {
                            let cmp = v.cmp(&min_v);
                            if *min_exclusive {
                                cmp == VersionRelation::Greater
                            } else {
                                cmp != VersionRelation::Less
                            }
                        }
                        Err(_) => false,
                    },
                    None => true,
                };
                
                let max_ok = match max {
                    Some(max_v) => match SemanticVersion::parse(max_v) {
                        Ok(max_v) => {
                            let cmp = v.cmp(&max_v);
                            if *max_exclusive {
                                cmp == VersionRelation::Less
                            } else {
                                cmp != VersionRelation::Greater
                            }
                        }
                        Err(_) => false,
                    },
                    None => true,
                };
                
                min_ok && max_ok
            }
            VersionConstraint::Caret(base) => {
                match SemanticVersion::parse(base) {
                    Ok(base_v) => {
                        // ^1.2.3 允许 >=1.2.3 <2.0.0
                        // ^0.2.3 允许 >=0.2.3 <0.3.0
                        // ^0.0.3 允许 >=0.0.3 <0.0.4
                        if base_v.major == 0 {
                            if base_v.minor == 0 {
                                v.major == 0 && v.minor == 0 && v.patch == base_v.patch
                            } else {
                                v.major == 0 && v.minor == base_v.minor && v.patch >= base_v.patch
                            }
                        } else {
                            v.major == base_v.major &&
                            (v.minor > base_v.minor || 
                             (v.minor == base_v.minor && v.patch >= base_v.patch))
                        }
                    }
                    Err(_) => false,
                }
            }
            VersionConstraint::Tilde(base) => {
                match SemanticVersion::parse(base) {
                    Ok(base_v) => {
                        // ~1.2.3 允许 >=1.2.3 <1.3.0
                        v.major == base_v.major &&
                        v.minor == base_v.minor &&
                        v.patch >= base_v.patch
                    }
                    Err(_) => false,
                }
            }
            VersionConstraint::Wildcard { major, minor } => {
                v.major == *major && minor.map_or(true, |m| v.minor == m)
            }
            VersionConstraint::Or(constraints) => {
                constraints.iter().any(|c| c.satisfies(version))
            }
            VersionConstraint::And(constraints) => {
                constraints.iter().all(|c| c.satisfies(version))
            }
        }
    }
    
    /// 转换为字符串
    pub fn to_string(&self) -> String {
        match self {
            VersionConstraint::Exact(v) => format!("={}", v),
            VersionConstraint::Range { min, max, min_exclusive, max_exclusive } => {
                let mut parts = Vec::new();
                if let Some(min_v) = min {
                    if *min_exclusive {
                        parts.push(format!(">{}", min_v));
                    } else {
                        parts.push(format!(">={}", min_v));
                    }
                }
                if let Some(max_v) = max {
                    if *max_exclusive {
                        parts.push(format!("<{}", max_v));
                    } else {
                        parts.push(format!("<={}", max_v));
                    }
                }
                parts.join(" ")
            }
            VersionConstraint::Caret(v) => format!("^{}", v),
            VersionConstraint::Tilde(v) => format!("~{}", v),
            VersionConstraint::Wildcard { major, minor } => {
                match minor {
                    Some(m) => format!("{}.{}.x", major, m),
                    None => format!("{}.x", major),
                }
            }
            VersionConstraint::Or(constraints) => {
                constraints.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" || ")
            }
            VersionConstraint::And(constraints) => {
                constraints.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
            }
        }
    }
}

/// 版本约束解析器
pub struct VersionConstraintParser;

impl VersionConstraintParser {
    /// 解析约束字符串
    pub fn parse(constraint: &str) -> Result<VersionConstraint, PluginError> {
        VersionConstraint::parse(constraint)
    }
    
    /// 检查版本是否满足约束
    pub fn satisfies(version: &str, constraint: &str) -> bool {
        match Self::parse(constraint) {
            Ok(c) => c.satisfies(version),
            Err(_) => false,
        }
    }
    
    /// 找到满足所有约束的最佳版本
    pub fn find_best_version(
        constraints: &[String],
        available_versions: &[String],
    ) -> Option<String> {
        let parsed_constraints: Vec<VersionConstraint> = constraints
            .iter()
            .filter_map(|c| Self::parse(c).ok())
            .collect();
        
        let mut candidates: Vec<SemanticVersion> = available_versions
            .iter()
            .filter_map(|v| SemanticVersion::parse(v).ok())
            .filter(|v| {
                parsed_constraints.iter().all(|c| c.satisfies(&v.to_string()))
            })
            .collect();
        
        // 按版本号降序排序，返回最高版本
        candidates.sort_by(|a, b| {
            match a.cmp(b) {
                VersionRelation::Greater => std::cmp::Ordering::Less,
                VersionRelation::Less => std::cmp::Ordering::Greater,
                VersionRelation::Equal => std::cmp::Ordering::Equal,
                VersionRelation::Incompatible => std::cmp::Ordering::Equal,
            }
        });
        
        candidates.first().map(|v| v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_version() {
        let v = SemanticVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_none());
    }
    
    #[test]
    fn test_parse_prerelease() {
        let v = SemanticVersion::parse("1.2.3-beta.1").unwrap();
        assert_eq!(v.major, 1);
        assert!(matches!(v.pre, Some(PreRelease::Beta(_))));
    }
    
    #[test]
    fn test_compare_versions() {
        let v1 = SemanticVersion::parse("1.2.3").unwrap();
        let v2 = SemanticVersion::parse("2.0.0").unwrap();
        let v3 = SemanticVersion::parse("1.2.3").unwrap();
        
        assert_eq!(v1.cmp(&v2), VersionRelation::Less);
        assert_eq!(v1.cmp(&v3), VersionRelation::Equal);
    }
    
    #[test]
    fn test_caret_constraint() {
        assert!(VersionManager::satisfies_constraint("1.2.3", "^1.0.0"));
        assert!(VersionManager::satisfies_constraint("1.9.9", "^1.0.0"));
        assert!(!VersionManager::satisfies_constraint("2.0.0", "^1.0.0"));
    }
}
