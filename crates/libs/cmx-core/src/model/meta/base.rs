//! 基础错误类型、表定义数据库执行 trait、建表配置数据结构
//!
//! JSON 加载、配置管理、i18n 等具体实现已迁移至 cmx-metadata crate。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::cell::TableDefine;

// ==========================================
// 错误类型
// ==========================================

#[derive(Error, Debug)]
pub enum BaseError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("未实现: {0}")]
    Unimplemented(String),
    #[error("配置未找到: {0}")]
    ConfigNotFound(String),
    #[error("配置依赖错误: {0}")]
    ConfigDependency(String),
    #[error("DDL 生成错误: {0}")]
    DdlGeneration(String),
    #[error("DDL 解析错误: {0}")]
    DdlParse(String),
}

// ==========================================
// 建表 JSON 配置文件（纯数据结构）
// ==========================================

/// 建表 JSON 配置文件：描述一组表定义文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinesConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

// ==========================================
// 数据库创建/升级表接口
// ==========================================

/// 表定义在数据库中执行"创建或升级"的接口，后续由具体数据库实现（如 PostgreSQL / MySQL）。
// #[async_trait]
pub trait TableDefineDbExecutor: Send + Sync {
    async fn create_table(&self, define: &TableDefine) -> Result<(), BaseError>;
    async fn upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    async fn create_or_upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError> {
        if self.create_table(define).await.is_ok() {
            return Ok(());
        }
        self.upgrade_table(define).await
    }
}
