//! 编码引擎错误类型。

use thiserror::Error;

/// 编码引擎统一错误类型。
#[derive(Debug, Error)]
pub enum CodeError {
    #[error("编码规则段定义无效：{0}")]
    InvalidSegment(String),

    #[error("不支持的段类型：{0}")]
    UnknownSegmentType(String),

    #[error("段求值失败：{field} 段期望 {expected}，实际 {actual}")]
    SegmentEvalFailed {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("未找到匹配的编码规则（ruleCode={0}）")]
    NoMatchingRule(String),

    #[error("编码冲突重试超上限（最多 {0} 次），可能并发过高或号段将满")]
    MaxRetryExceeded(u32),

    #[error("随机段空间耗尽（重试 {0} 次仍冲突），请扩大位数或更换字符池")]
    RandomSpaceExhausted(u32),

    #[error("引用段取值失败：字段 {0} 不存在或为空")]
    RefFieldMissing(String),

    #[error("正则校验失败：编码 {code} 不匹配规则 {pattern}")]
    PatternMismatch { code: String, pattern: String },

    #[error("编码引擎内部错误：{0}")]
    Internal(String),

    #[error("数据库操作失败：{0}")]
    Database(String),
}

/// 编码引擎统一 Result 别名。
pub type Result<T> = core::result::Result<T, CodeError>;

impl From<serde_json::Error> for CodeError {
    fn from(e: serde_json::Error) -> Self {
        CodeError::InvalidSegment(format!("JSON 解析失败：{e}"))
    }
}
