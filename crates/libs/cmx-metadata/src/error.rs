//! 模块错误类型定义
//!
//! 定义了 cmx-metadata 模块可能遇到的所有错误类型，
//! 包括 IO 错误、JSON 解析错误、DDL 生成/解析/执行错误、配置错误等。

use thiserror::Error;

/// cmx-metadata 模块的错误类型
///
/// 使用 thiserror 库实现，便于错误传播和错误信息格式化。
#[derive(Error, Debug)]
pub enum MetadataError {
    /// IO 错误 - 文件读写等操作系统级别的错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错误 - JSON 格式解析失败
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    /// DDL 生成错误 - 将 TableDefine 转换为 DDL 语句失败
    #[error("DDL 生成错误: {0}")]
    DdlGeneration(String),

    /// DDL 解析错误 - 解析 DDL 语句还原为 TableDefine 失败
    #[error("DDL 解析错误: {0}")]
    DdlParse(String),

    /// DDL 执行错误 - 在数据库中执行 DDL 语句失败
    #[error("DDL 执行错误: {0}")]
    DdlExecution(String),

    /// 配置未找到错误 - 指定的配置文件不存在
    #[error("配置未找到: {0}")]
    ConfigNotFound(String),

    /// 配置依赖错误 - 配置之间存在循环依赖或依赖不存在的配置
    #[error("配置依赖错误: {0}")]
    ConfigDependency(String),
}
