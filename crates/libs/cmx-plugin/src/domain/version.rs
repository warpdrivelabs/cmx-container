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
        if nums.is_empty() || nums.len() > 3 {
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

        let (relation, version_str) = if let Some(stripped) = constraint.strip_prefix(">=") {
            (VersionRelation::GreaterThanOrEqual, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix("<=") {
            (VersionRelation::LessThanOrEqual, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix('>') {
            (VersionRelation::GreaterThan, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix('<') {
            (VersionRelation::LessThan, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix('=') {
            (VersionRelation::Equal, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix('^') {
            (VersionRelation::Compatible, stripped.trim())
        } else if let Some(stripped) = constraint.strip_prefix('~') {
            (VersionRelation::Approximately, stripped.trim())
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== SemanticVersion::parse ====================

    #[test]
    fn test_parse_full_version() {
        let v = SemanticVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.pre_release, None);
        assert_eq!(v.build, None);
    }

    #[test]
    fn test_parse_major_minor_only() {
        // 仅主次版本号，补丁号缺省为 0
        let v = SemanticVersion::parse("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_major_only() {
        // 仅主版本号，次版本与补丁号缺省为 0
        let v = SemanticVersion::parse("1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn test_parse_with_pre_release() {
        let v = SemanticVersion::parse("1.0.0-alpha.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.pre_release.as_ref().unwrap().identifiers.len(), 2);
        match &v.pre_release.as_ref().unwrap().identifiers[0] {
            PreReleaseIdentifier::AlphaNumeric(s) => assert_eq!(s, "alpha"),
            other => panic!("期望 AlphaNumeric 标识符，得到 {:?}", other),
        }
        match &v.pre_release.as_ref().unwrap().identifiers[1] {
            PreReleaseIdentifier::Numeric(n) => assert_eq!(*n, 1),
            other => panic!("期望 Numeric 标识符，得到 {:?}", other),
        }
    }

    #[test]
    fn test_parse_with_build_metadata() {
        let v = SemanticVersion::parse("1.0.0+build.123").unwrap();
        assert_eq!(v.build.as_deref(), Some("build.123"));
        assert_eq!(v.pre_release, None);
    }

    #[test]
    fn test_parse_with_pre_release_and_build() {
        let v = SemanticVersion::parse("1.0.0-rc.1+build.5").unwrap();
        assert!(v.pre_release.is_some());
        assert_eq!(v.build.as_deref(), Some("build.5"));
    }

    #[test]
    fn test_parse_trim_whitespace() {
        let v = SemanticVersion::parse("  1.2.3  ").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_parse_empty_string_errors() {
        assert_eq!(SemanticVersion::parse("").unwrap_err(), VersionParseError::EmptyString);
    }

    #[test]
    fn test_parse_whitespace_only_errors() {
        // 仅空白字符串解析后为空，应返回 EmptyString
        assert_eq!(
            SemanticVersion::parse("   ").unwrap_err(),
            VersionParseError::EmptyString
        );
    }

    #[test]
    fn test_parse_invalid_number_errors() {
        assert_eq!(
            SemanticVersion::parse("a.b.c").unwrap_err(),
            VersionParseError::InvalidNumber
        );
        assert_eq!(
            SemanticVersion::parse("1.0.x").unwrap_err(),
            VersionParseError::InvalidNumber
        );
    }

    #[test]
    fn test_parse_too_many_parts_errors() {
        // 超过 3 段版本号应返回 InvalidFormat
        assert_eq!(
            SemanticVersion::parse("1.2.3.4").unwrap_err(),
            VersionParseError::InvalidFormat
        );
    }

    // ==================== SemanticVersion Display ====================

    #[test]
    fn test_display_simple_version() {
        let v = SemanticVersion::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_display_with_pre_release() {
        let v = SemanticVersion::parse("1.0.0-alpha.1").unwrap();
        assert_eq!(v.to_string(), "1.0.0-alpha.1");
    }

    #[test]
    fn test_display_with_build() {
        let v = SemanticVersion::parse("1.0.0+build.5").unwrap();
        assert_eq!(v.to_string(), "1.0.0+build.5");
    }

    // ==================== SemanticVersion 比较（升级/降级判断基础） ====================

    #[test]
    fn test_compare_major_version() {
        let v1 = SemanticVersion::new(1, 0, 0);
        let v2 = SemanticVersion::new(2, 0, 0);
        assert!(v1 < v2, "1.0.0 应小于 2.0.0");
        assert!(v2 > v1, "2.0.0 应大于 1.0.0");
    }

    #[test]
    fn test_compare_minor_version() {
        let v1 = SemanticVersion::new(1, 0, 0);
        let v2 = SemanticVersion::new(1, 1, 0);
        assert!(v1 < v2);
    }

    #[test]
    fn test_compare_patch_version() {
        let v1 = SemanticVersion::new(1, 0, 0);
        let v2 = SemanticVersion::new(1, 0, 1);
        assert!(v1 < v2);
    }

    #[test]
    fn test_compare_equal_versions() {
        let v1 = SemanticVersion::new(1, 2, 3);
        let v2 = SemanticVersion::new(1, 2, 3);
        assert_eq!(v1.cmp(&v2), Ordering::Equal);
        assert!(v1 == v2);
    }

    #[test]
    fn test_pre_release_lower_than_release() {
        // 预发布版本应低于同号正式版本（semver 规则）
        let pre = SemanticVersion::parse("1.0.0-rc.1").unwrap();
        let release = SemanticVersion::new(1, 0, 0);
        assert!(pre < release, "1.0.0-rc.1 应小于 1.0.0");
    }

    #[test]
    fn test_compare_pre_release_numeric() {
        // alpha.1 < alpha.2
        let a = SemanticVersion::parse("1.0.0-alpha.1").unwrap();
        let b = SemanticVersion::parse("1.0.0-alpha.2").unwrap();
        assert!(a < b);
    }

    #[test]
    fn test_compare_pre_release_alphanumeric() {
        // alpha < beta（字母序）
        let a = SemanticVersion::parse("1.0.0-alpha").unwrap();
        let b = SemanticVersion::parse("1.0.0-beta").unwrap();
        assert!(a < b);
    }

    #[test]
    fn test_compare_pre_release_numeric_lower_than_alphanumeric() {
        // 数字标识符低于字母数字标识符（semver 规则）
        let numeric = SemanticVersion::parse("1.0.0-1").unwrap();
        let alpha = SemanticVersion::parse("1.0.0-alpha").unwrap();
        assert!(numeric < alpha, "数字预发布标识符应低于字母数字标识符");
    }

    #[test]
    fn test_compare_pre_release_different_identifier_count() {
        // 标识符少的更低：alpha < alpha.1
        let short = SemanticVersion::parse("1.0.0-alpha").unwrap();
        let long = SemanticVersion::parse("1.0.0-alpha.1").unwrap();
        assert!(short < long);
    }

    // ==================== VersionConstraint::parse ====================

    #[test]
    fn test_parse_constraint_equal_default() {
        // 无前缀默认 Equal
        let c = VersionConstraint::parse("1.2.3").unwrap();
        assert_eq!(c.relation, VersionRelation::Equal);
        assert_eq!(c.version, SemanticVersion::new(1, 2, 3));
    }

    #[test]
    fn test_parse_constraint_explicit_equal() {
        let c = VersionConstraint::parse("=1.0.0").unwrap();
        assert_eq!(c.relation, VersionRelation::Equal);
    }

    #[test]
    fn test_parse_constraint_greater_than() {
        let c = VersionConstraint::parse(">1.0.0").unwrap();
        assert_eq!(c.relation, VersionRelation::GreaterThan);
    }

    #[test]
    fn test_parse_constraint_greater_than_or_equal() {
        let c = VersionConstraint::parse(">=1.0.0").unwrap();
        assert_eq!(c.relation, VersionRelation::GreaterThanOrEqual);
    }

    #[test]
    fn test_parse_constraint_less_than() {
        let c = VersionConstraint::parse("<1.0.0").unwrap();
        assert_eq!(c.relation, VersionRelation::LessThan);
    }

    #[test]
    fn test_parse_constraint_less_than_or_equal() {
        let c = VersionConstraint::parse("<=1.0.0").unwrap();
        assert_eq!(c.relation, VersionRelation::LessThanOrEqual);
    }

    #[test]
    fn test_parse_constraint_compatible() {
        let c = VersionConstraint::parse("^1.2.3").unwrap();
        assert_eq!(c.relation, VersionRelation::Compatible);
    }

    #[test]
    fn test_parse_constraint_approximately() {
        let c = VersionConstraint::parse("~1.2.3").unwrap();
        assert_eq!(c.relation, VersionRelation::Approximately);
    }

    // ==================== VersionConstraint::satisfies ====================

    #[test]
    fn test_satisfies_equal() {
        let c = VersionConstraint::parse("=1.0.0").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(1, 0, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 0, 1)));
    }

    #[test]
    fn test_satisfies_greater_than() {
        let c = VersionConstraint::parse(">1.0.0").unwrap();
        assert!(!c.satisfies(&SemanticVersion::new(1, 0, 0)));
        assert!(c.satisfies(&SemanticVersion::new(1, 0, 1)));
        assert!(c.satisfies(&SemanticVersion::new(2, 0, 0)));
    }

    #[test]
    fn test_satisfies_greater_than_or_equal() {
        let c = VersionConstraint::parse(">=1.0.0").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(1, 0, 0)));
        assert!(c.satisfies(&SemanticVersion::new(1, 5, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(0, 9, 0)));
    }

    #[test]
    fn test_satisfies_less_than() {
        let c = VersionConstraint::parse("<1.0.0").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(0, 9, 9)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 0, 0)));
    }

    #[test]
    fn test_satisfies_less_than_or_equal() {
        let c = VersionConstraint::parse("<=1.0.0").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(1, 0, 0)));
        assert!(c.satisfies(&SemanticVersion::new(0, 5, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 0, 1)));
    }

    #[test]
    fn test_satisfies_compatible() {
        // ^1.2.3：>=1.2.3 且同主版本号
        let c = VersionConstraint::parse("^1.2.3").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(1, 2, 3)));
        assert!(c.satisfies(&SemanticVersion::new(1, 9, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 2, 2)));
        assert!(!c.satisfies(&SemanticVersion::new(2, 0, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(0, 9, 0)));
    }

    #[test]
    fn test_satisfies_approximately() {
        // ~1.2.3：>=1.2.3 且同主次版本号
        let c = VersionConstraint::parse("~1.2.3").unwrap();
        assert!(c.satisfies(&SemanticVersion::new(1, 2, 3)));
        assert!(c.satisfies(&SemanticVersion::new(1, 2, 9)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 3, 0)));
        assert!(!c.satisfies(&SemanticVersion::new(1, 2, 2)));
    }

    // ==================== VersionConstraint Display ====================

    #[test]
    fn test_constraint_display_roundtrip() {
        for s in ["=1.0.0", ">1.0.0", ">=1.0.0", "<1.0.0", "<=1.0.0", "^1.0.0", "~1.0.0"] {
            let c = VersionConstraint::parse(s).unwrap();
            assert_eq!(c.to_string(), s, "约束 {} 的 Display 应能往返", s);
        }
    }

    // ==================== PreRelease / PreReleaseIdentifier ====================

    #[test]
    fn test_pre_release_parse_and_display() {
        let pre = PreRelease::parse("rc.1.2").unwrap();
        assert_eq!(pre.identifiers.len(), 3);
        assert_eq!(pre.to_string(), "rc.1.2");
    }

    #[test]
    fn test_prerelease_identifier_comparison_numeric() {
        let a = PreReleaseIdentifier::Numeric(1);
        let b = PreReleaseIdentifier::Numeric(2);
        assert!(a < b);
    }

    #[test]
    fn test_prerelease_identifier_comparison_alphanumeric() {
        let a = PreReleaseIdentifier::AlphaNumeric("alpha".to_string());
        let b = PreReleaseIdentifier::AlphaNumeric("beta".to_string());
        assert!(a < b);
    }

    #[test]
    fn test_version_parse_error_display() {
        assert_eq!(VersionParseError::EmptyString.to_string(), "版本字符串为空");
        assert_eq!(VersionParseError::InvalidFormat.to_string(), "版本格式无效");
        assert_eq!(VersionParseError::InvalidNumber.to_string(), "版本号无效");
    }

    // ==================== 升级/降级判断场景 ====================

    #[test]
    fn test_upgrade_scenario_version_comparison() {
        // 模拟升级判断：新版本必须大于当前版本
        let current = SemanticVersion::parse("1.0.0").unwrap();
        let candidate = SemanticVersion::parse("1.1.0").unwrap();
        assert!(candidate > current, "1.1.0 应可从 1.0.0 升级");
        let same = SemanticVersion::parse("1.0.0").unwrap();
        assert!(same <= current, "相同版本不应触发升级");
    }

    #[test]
    fn test_downgrade_scenario_version_comparison() {
        // 模拟降级判断：目标版本必须小于当前版本
        let current = SemanticVersion::parse("2.0.0").unwrap();
        let target = SemanticVersion::parse("1.9.0").unwrap();
        assert!(target < current, "1.9.0 应可从 2.0.0 降级");
        assert!(current >= target, "2.0.0 不应小于 1.9.0");
    }

    #[test]
    fn test_upgrade_constraint_check() {
        // 模拟升级约束：候选版本必须满足 ^1.0.0（兼容升级）
        let constraint = VersionConstraint::parse("^1.0.0").unwrap();
        let compatible_upgrade = SemanticVersion::parse("1.5.0").unwrap();
        let breaking_upgrade = SemanticVersion::parse("2.0.0").unwrap();
        assert!(constraint.satisfies(&compatible_upgrade), "1.5.0 应满足 ^1.0.0");
        assert!(!constraint.satisfies(&breaking_upgrade), "2.0.0 不满足 ^1.0.0");
    }
}
