use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("DDL 生成错误: {0}")]
    DdlGeneration(String),
    #[error("DDL 解析错误: {0}")]
    DdlParse(String),
    #[error("DDL 执行错误: {0}")]
    DdlExecution(String),
    #[error("配置未找到: {0}")]
    ConfigNotFound(String),
    #[error("配置依赖错误: {0}")]
    ConfigDependency(String),
}
