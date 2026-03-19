//! 版本管理模块
//! 
//! 定义语义版本、版本约束、版本比较

use std::cmp::Ordering;
use std::fmt;
use serde::{Deserialize, Serialize};

/// 语义版本
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticVersion {
    /// 主版本号
    pub major: u64,
    /// 次版本号
    pub minor: u64,
    /// 补丁版本号
    pub patch: u64,
    /// 预发布版本
    pub pre_release: Option<PreRelease>,
    /// 构建元数据
    pub build: Option<String>,
}

impl SemanticVersion {
    /// 创建新的语义版本
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre_release: None,
            build: None,
        }
    }
    
    /// 解析版本字符串
    pub fn parse(version: &str) -> Result<Self, VersionParseError> {
        let version = version.trim();
        if version.is_empty() {
            return Err(VersionParseError::EmptyString);
        }
        
        let parts: Vec<&str> = version.splitn(2, '+').collect();
        let version_part = parts[0];
        let build = parts.get(1).map(|s| s.to_string());
        
        let parts: Vec<&str> = version_part.splitn(2, '-').collect();
        let main_part = parts[0];
        let pre_release = parts.get(1).map(|s| PreRelease::parse(s)).transpose()?;
        
        let nums: Vec<&str> = main_part.split('.').collect();
        if nums.len() < 1 || nums.len() > 3 {
            return Err(VersionParseError::InvalidFormat);
        }
        
        let major = nums[0].parse().map_err(|_| VersionParseError::InvalidNumber)?;
        let minor = nums.get(1)
            .map(|s| s.parse().map_err(|_| VersionParseError::InvalidNumber))
            .transpose()?
            .unwrap_or(0);
        let patch = nums.get(2)
            .map(|s| s.parse().map_err(|_| VersionParseError::InvalidNumber))
            .transpose()?
            .unwrap_or(0);
        
        Ok(Self {
            major,
            minor,
            patch,
            pre_release,
            build,
        })
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.pre_release {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl PartialOrd for SemanticVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemanticVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => match self.patch.cmp(&other.patch) {
                    Ordering::Equal => {
                        match (&self.pre_release, &other.pre_release) {
                            (None, None) => Ordering::Equal,
                            (None, Some(_)) => Ordering::Greater,
                            (Some(_), None) => Ordering::Less,
                            (Some(a), Some(b)) => a.cmp(b),
                        }
                    }
                    other => other,
                },
                other => other,
            },
            other => other,
        }
    }
}

/// 预发布版本
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreRelease {
    /// 标识符列表
    pub identifiers: Vec<PreReleaseIdentifier>,
}

impl PreRelease {
    /// 解析预发布版本字符串
    pub fn parse(s: &str) -> Result<Self, VersionParseError> {
        let identifiers = s.split('.')
            .map(|id| {
                if let Ok(num) = id.parse::<u64>() {
                    Ok(PreReleaseIdentifier::Numeric(num))
                } else {
                    Ok(PreReleaseIdentifier::AlphaNumeric(id.to_string()))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(Self { identifiers })
    }
}

impl fmt::Display for PreRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.identifiers.iter().map(|id| id.to_string()).collect();
        write!(f, "{}", parts.join("."))
    }
}

impl PartialOrd for PreRelease {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PreRelease {
    fn cmp(&self, other: &Self) -> Ordering {
        for (a, b) in self.identifiers.iter().zip(other.identifiers.iter()) {
            match a.cmp(b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        self.identifiers.len().cmp(&other.identifiers.len())
    }
}

/// 预发布版本标识符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreReleaseIdentifier {
    /// 数字标识符
    Numeric(u64),
    /// 字母数字标识符
    AlphaNumeric(String),
}

impl fmt::Display for PreReleaseIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreReleaseIdentifier::Numeric(n) => write!(f, "{}", n),
            PreReleaseIdentifier::AlphaNumeric(s) => write!(f, "{}", s),
        }
    }
}

impl PartialOrd for PreReleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PreReleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (PreReleaseIdentifier::Numeric(a), PreReleaseIdentifier::Numeric(b)) => a.cmp(b),
            (PreReleaseIdentifier::Numeric(_), PreReleaseIdentifier::AlphaNumeric(_)) => Ordering::Less,
            (PreReleaseIdentifier::AlphaNumeric(_), PreReleaseIdentifier::Numeric(_)) => Ordering::Greater,
            (PreReleaseIdentifier::AlphaNumeric(a), PreReleaseIdentifier::AlphaNumeric(b)) => a.cmp(b),
        }
    }
}

/// 版本约束
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionConstraint {
    /// 版本关系
    pub relation: VersionRelation,
    /// 版本
    pub version: SemanticVersion,
}

impl VersionConstraint {
    /// 创建新的版本约束
    pub fn new(relation: VersionRelation, version: SemanticVersion) -> Self {
        Self { relation, version }
    }
    
    /// 解析版本约束字符串
    pub fn parse(constraint: &str) -> Result<Self, VersionParseError> {
        let constraint = constraint.trim();
        
        let (relation, version_str) = if constraint.starts_with(">=") {
            (VersionRelation::GreaterThanOrEqual, constraint[2..].trim())
        } else if constraint.starts_with("<=") {
            (VersionRelation::LessThanOrEqual, constraint[2..].trim())
        } else if constraint.starts_with('>') {
            (VersionRelation::GreaterThan, constraint[1..].trim())
        } else if constraint.starts_with('<') {
            (VersionRelation::LessThan, constraint[1..].trim())
        } else if constraint.starts_with('=') {
            (VersionRelation::Equal, constraint[1..].trim())
        } else if constraint.starts_with('^') {
            (VersionRelation::Compatible, constraint[1..].trim())
        } else if constraint.starts_with('~') {
            (VersionRelation::Approximately, constraint[1..].trim())
        } else {
            (VersionRelation::Equal, constraint)
        };
        
        let version = SemanticVersion::parse(version_str)?;
        Ok(Self { relation, version })
    }
    
    /// 检查版本是否满足约束
    pub fn satisfies(&self, version: &SemanticVersion) -> bool {
        match self.relation {
            VersionRelation::Equal => version == &self.version,
            VersionRelation::GreaterThan => version > &self.version,
            VersionRelation::GreaterThanOrEqual => version >= &self.version,
            VersionRelation::LessThan => version < &self.version,
            VersionRelation::LessThanOrEqual => version <= &self.version,
            VersionRelation::Compatible => {
                version >= &self.version && version.major == self.version.major
            }
            VersionRelation::Approximately => {
                version >= &self.version 
                    && version.major == self.version.major 
                    && version.minor == self.version.minor
            }
        }
    }
}

impl fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.relation {
            VersionRelation::Equal => write!(f, "={}", self.version),
            VersionRelation::GreaterThan => write!(f, ">{}", self.version),
            VersionRelation::GreaterThanOrEqual => write!(f, ">={}", self.version),
            VersionRelation::LessThan => write!(f, "<{}", self.version),
            VersionRelation::LessThanOrEqual => write!(f, "<={}", self.version),
            VersionRelation::Compatible => write!(f, "^{}", self.version),
            VersionRelation::Approximately => write!(f, "~{}", self.version),
        }
    }
}

/// 版本关系
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionRelation {
    /// 等于
    Equal,
    /// 大于
    GreaterThan,
    /// 大于等于
    GreaterThanOrEqual,
    /// 小于
    LessThan,
    /// 小于等于
    LessThanOrEqual,
    /// 兼容（^）
    Compatible,
    /// 近似（~）
    Approximately,
}

/// 版本解析错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionParseError {
    /// 空字符串
    EmptyString,
    /// 无效格式
    InvalidFormat,
    /// 无效数字
    InvalidNumber,
}

impl fmt::Display for VersionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionParseError::EmptyString => write!(f, "版本字符串为空"),
            VersionParseError::InvalidFormat => write!(f, "版本格式无效"),
            VersionParseError::InvalidNumber => write!(f, "版本号无效"),
        }
    }
}

impl std::error::Error for VersionParseError {}
