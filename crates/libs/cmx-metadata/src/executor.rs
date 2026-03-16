//! DDL 执行模块 — 在数据库中执行 DDL 语句
//!
//! 提供 DDL 执行函数和 `PgTableDefineExecutor` 实现。
//! 依赖 cmx-database 提供的底层 SQL 执行能力。

use cmx_core::model::cell::TableDefine;
use cmx_core::model::meta::base::{BaseError, TableDefineDbExecutor};
use cmx_database::execute_sql_by_ids;

use crate::ddl::postgres::PostgresDdlDialect;
use crate::ddl::DdlDialect;
use crate::MetadataError;

/// 执行多条 DDL 语句（逐条执行）
pub async fn execute_ddl_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statements: &[String],
) -> Result<(), MetadataError> {
    for stmt in statements {
        execute_sql_by_ids(db_id, txn_id, stmt)
            .await
            .map_err(|e| MetadataError::DdlExecution(e.to_string()))?;
    }
    Ok(())
}

/// 执行单条 DDL 语句
pub async fn execute_ddl_statement_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statement: &str,
) -> Result<u64, MetadataError> {
    execute_sql_by_ids(db_id, txn_id, statement)
        .await
        .map_err(|e| MetadataError::DdlExecution(e.to_string()))
}

/// PostgreSQL TableDefine 执行器
///
/// 将 TableDefine 转为 DDL 语句后通过数据库连接执行。
/// 注意：`TableDefineDbExecutor` trait 的方法是同步签名，
/// 因此内部使用 `tokio::runtime::Handle::current().block_on()` 执行异步操作。
pub struct PgTableDefineExecutor {
    pub db_id: String,
    pub txn_id: Option<String>,
}

impl PgTableDefineExecutor {
    pub fn new(db_id: impl Into<String>, txn_id: Option<String>) -> Self {
        Self {
            db_id: db_id.into(),
            txn_id,
        }
    }

    /// 执行多条 DDL 语句（内部辅助）
    fn execute_statements(&self, statements: Vec<String>) -> Result<(), MetadataError> {
        let db_id = self.db_id.clone();
        let txn_id = self.txn_id.clone();
        tokio::runtime::Handle::current().block_on(async move {
            for stmt in &statements {
                execute_sql_by_ids(&db_id, txn_id.as_deref(), stmt)
                    .await
                    .map_err(|e| MetadataError::DdlExecution(e.to_string()))?;
            }
            Ok(())
        })
    }
}

impl TableDefineDbExecutor for PgTableDefineExecutor {
    fn create_table(&self, define: &TableDefine) -> std::result::Result<(), BaseError> {
        let dialect = PostgresDdlDialect::default();

        // 生成 CREATE TABLE + COMMENT + INDEX
        let create_stmt = dialect
            .generate_create_table(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;
        let comment_stmts = dialect
            .generate_comments(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;
        let index_stmts = dialect
            .generate_create_indexes(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;

        let mut all_stmts = vec![create_stmt];
        all_stmts.extend(comment_stmts);
        all_stmts.extend(index_stmts);

        self.execute_statements(all_stmts)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))
    }

    fn upgrade_table(&self, _define: &TableDefine) -> std::result::Result<(), BaseError> {
        // 升级表需要先查询当前表结构，再 diff，再生成增量 DDL
        // 暂时返回未实现
        Err(BaseError::Unimplemented(
            "upgrade_table 需要查询当前表结构后再 diff 实现".to_string(),
        ))
    }
}
