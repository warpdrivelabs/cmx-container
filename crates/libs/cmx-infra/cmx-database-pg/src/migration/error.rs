//! 数据库迁移错误类型。

use thiserror::Error;

/// 数据库迁移错误类型
#[derive(Error, Debug)]
pub enum MigrationError {
    /// 迁移文件名无效
    #[error("迁移文件名无效: {0}")]
    InvalidFileName(String),
    /// 迁移文件读取失败
    #[error("迁移文件读取失败: {0}")]
    FileReadError(#[from] std::io::Error),
    /// SQL 执行失败
    #[error("SQL 执行失败: {0}")]
    SqlExecutionError(String),
    /// 校验和不匹配
    #[error("校验和不匹配 - 版本: {version}, 记录: {recorded}, 实际: {actual}")]
    ChecksumMismatch {
        /// 迁移版本号
        version: String,
        /// 数据库中记录的校验和
        recorded: String,
        /// 实际文件的校验和
        actual: String,
    },
    /// 迁移未找到
    #[error("迁移未找到: {0}")]
    MigrationNotFound(String),
    /// 无法回滚，状态无效
    #[error("无法回滚，状态无效: {0}")]
    InvalidRollbackState(String),
    /// 无回滚脚本
    #[error("无回滚脚本: {0}")]
    NoRollbackScript(String),
    /// 获取分布式锁失败
    #[error("获取分布式锁失败")]
    LockAcquireFailed,
    /// 数据库查询失败
    #[error("数据库查询失败: {0}")]
    QueryFailed(String),
}

/// 迁移操作结果类型
pub type MigrationResult<T> = Result<T, MigrationError>;
