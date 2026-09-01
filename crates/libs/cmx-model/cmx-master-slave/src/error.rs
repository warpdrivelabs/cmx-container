//! 错误类型（纯协调器层，不含存储错误）。

use thiserror::Error;

/// 协调器 / 汇总层错误。存储错误（DB/校验/冲突）由服务侧的 `HierService` 实现自持。
#[derive(Debug, Error)]
pub enum MsError {
    /// schema 里重复路径（对齐前端 `duplicate path`）。
    #[error("duplicate path: {0}")]
    DuplicatePath(String),
    /// 引用了 schema 未定义的路径（对齐前端 `unknown path`）。
    #[error("unknown path: {0}")]
    UnknownPath(String),
    /// 汇总规则依赖图成环（对齐前端 16 层级联安全阀）。
    #[error("aggregation cycle detected among rules: {0}")]
    AggCycle(String),
    /// 汇总规则非法。
    #[error("invalid aggregation rule: {0}")]
    InvalidRule(String),
    /// 定义解析失败。
    #[error("invalid schema definition: {0}")]
    InvalidSchema(String),
    /// 树结构非法。
    #[error("invalid tree: {0}")]
    InvalidTree(String),
}

/// 协调器层 Result。
pub type Result<T> = std::result::Result<T, MsError>;
