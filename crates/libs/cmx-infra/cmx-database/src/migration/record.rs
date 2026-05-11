use serde::{Deserialize, Serialize};
use std::fmt;

use super::error::MigrationError;

/// 迁移状态枚举
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStatus {
    /// 待执行
    Pending,
    /// 执行成功
    Success,
    /// 执行失败
    Failed,
    /// 已回滚
    RolledBack,
}

impl fmt::Display for MigrationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrationStatus::Pending => write!(f, "pending"),
            MigrationStatus::Success => write!(f, "success"),
            MigrationStatus::Failed => write!(f, "failed"),
            MigrationStatus::RolledBack => write!(f, "rolled_back"),
        }
    }
}

impl std::str::FromStr for MigrationStatus {
    type Err = MigrationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(MigrationStatus::Pending),
            "success" => Ok(MigrationStatus::Success),
            "failed" => Ok(MigrationStatus::Failed),
            "rolled_back" => Ok(MigrationStatus::RolledBack),
            _ => Err(MigrationError::QueryFailed(format!(
                "无效的迁移状态: {}",
                s
            ))),
        }
    }
}

/// 数据库迁移记录
///
/// 对应 cmx_schema_migrations 表中的一条记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    /// 迁移版本号（如 20260510_001）
    pub version: String,
    /// 迁移名称（如 init_core_tables）
    pub name: String,
    /// 校验和（SHA256）
    pub checksum: String,
    /// 执行状态
    pub status: MigrationStatus,
    /// 执行该迁移的节点ID
    pub executed_by: String,
    /// 执行耗时（毫秒）
    pub execution_time_ms: Option<i64>,
    /// 错误信息（执行失败时记录）
    pub error_message: Option<String>,
    /// 创建时间
    pub created_at: Option<String>,
    /// 更新时间
    pub updated_at: Option<String>,
}

/// 待执行的迁移
///
/// 从文件系统加载的迁移信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMigration {
    /// 迁移版本号
    pub version: String,
    /// 迁移名称
    pub name: String,
    /// 升级 SQL 内容
    pub up_sql: String,
    /// 回滚 SQL 内容
    pub down_sql: Option<String>,
    /// 校验和（基于 up_sql 内容计算）
    pub checksum: String,
}

/// 迁移执行摘要
///
/// 记录一次迁移执行的整体结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSummary {
    /// 已执行的迁移数量
    pub executed_count: usize,
    /// 跳过的迁移数量（已执行过的）
    pub skipped_count: usize,
    /// 执行失败的迁移列表
    pub failed: Vec<FailedMigration>,
}

/// 执行失败的迁移
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedMigration {
    /// 迁移版本号
    pub version: String,
    /// 迁移名称
    pub name: String,
    /// 错误信息
    pub error: String,
}

/// 校验和验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否验证通过
    pub is_valid: bool,
    /// 不匹配的记录列表
    pub mismatches: Vec<ChecksumMismatch>,
}

/// 校验和不匹配详情
#[derive(Debug, Clone)]
pub struct ChecksumMismatch {
    /// 迁移版本号
    pub version: String,
    /// 数据库中记录的校验和
    pub recorded: String,
    /// 实际文件的校验和
    pub actual: String,
}
