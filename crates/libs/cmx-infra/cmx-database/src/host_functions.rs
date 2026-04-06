//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 JSON 格式传递参数和结果。
//!
//! # 数据传递协议
//!
//! 所有宿主函数使用 JSON 格式传递结构化数据：
//! - 输入：由 linker_adapter 从 WASM 内存预读取的字节（JSON 字符串）
//! - 输出：JSON 格式的字节数组

use std::sync::Arc;

use cmx_traits::{HostFuncError, HostFunctionProvider, HostFuncWrapper, WasmLinker};

use crate::DatabaseManager;

/// 数据库宿主函数提供者
///
/// 封装 DatabaseManager 的核心 API，向 WASM 运行时注册数据库操作宿主函数。
/// 所有数据库操作通过 CallerData 中的 db_id 和 txn_id 确定目标数据库和事务上下文。
pub struct DatabaseHostFunctions {
    /// 数据库管理器引用
    db_manager: Arc<DatabaseManager>,
}

/// 数据库操作请求（JSON 反序列化）
#[derive(serde::Deserialize)]
struct DbRequest {
    /// SQL 语句
    sql: String,
    /// SQL 参数（JSON 值，可选）
    params: Option<serde_json::Value>,
    /// 数据集ID（查询时使用，可选）
    dataset_id: Option<String>,
}

/// 数据库操作响应（JSON 序列化）
#[derive(serde::Serialize)]
struct DbResponse {
    /// 是否成功
    success: bool,
    /// 影响行数（写操作返回）
    affected_rows: Option<u64>,
    /// 查询结果数据集（查询操作返回）
    dataset: Option<serde_json::Value>,
    /// 事务ID（事务操作返回）
    txn_id: Option<String>,
    /// 错误信息
    error: Option<String>,
}

impl DatabaseHostFunctions {
    /// 创建数据库宿主函数提供者
    ///
    /// # 参数
    ///
    /// * `db_manager` - 数据库管理器共享引用
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    /// 从输入字节解析请求数据
    fn parse_request(input: &[u8]) -> Result<DbRequest, String> {
        serde_json::from_slice::<DbRequest>(input)
            .map_err(|e| format!("请求数据解析失败: {}", e))
    }

    /// 构建成功响应
    fn ok_response(affected_rows: Option<u64>, dataset: Option<serde_json::Value>, txn_id: Option<String>) -> Vec<u8> {
        serde_json::to_vec(&DbResponse {
            success: true,
            affected_rows,
            dataset,
            txn_id,
            error: None,
        }).unwrap_or_default()
    }

    /// 构建错误响应
    fn err_response(msg: String) -> Vec<u8> {
        serde_json::to_vec(&DbResponse {
            success: false,
            affected_rows: None,
            dataset: None,
            txn_id: None,
            error: Some(msg),
        }).unwrap_or_default()
    }
}

impl HostFunctionProvider for DatabaseHostFunctions {
    /// 返回命名空间 "cmx:database"
    fn namespace(&self) -> &str {
        "cmx:database"
    }

    /// 注册数据库操作宿主函数
    ///
    /// 注册以下函数：
    /// - `cmx:database/execute_sql` — 执行写操作 SQL
    /// - `cmx:database/query_sql` — 执行查询 SQL
    /// - `cmx:database/txn_begin` — 开启事务
    /// - `cmx:database/txn_commit` — 提交事务
    /// - `cmx:database/txn_rollback` — 回滚事务
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        // cmx:database/execute_sql — 执行写操作 SQL
        let db_manager = self.db_manager.clone();
        let execute_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let db_id = caller.caller_data().db_id.clone();
            let txn_id = caller.caller_data().txn_id.clone();
            let manager = db_manager.clone();

            let request = match Self::parse_request(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(e)),
            };

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                if let Some(params) = &request.params {
                    manager.execute_sql_with_json(&db_id, txn_id.as_deref(), &request.sql, params.clone()).await
                } else {
                    manager.execute_sql(&db_id, txn_id.as_deref(), &request.sql).await
                }
            });

            match result {
                Ok(rows) => Ok(Self::ok_response(Some(rows), None, None)),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:database", "execute_sql", execute_fn)?;

        // cmx:database/query_sql — 执行查询 SQL
        let db_manager = self.db_manager.clone();
        let query_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let db_id = caller.caller_data().db_id.clone();
            let txn_id = caller.caller_data().txn_id.clone();
            let manager = db_manager.clone();

            let request = match Self::parse_request(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(e)),
            };

            let dataset_id = request.dataset_id.as_deref().unwrap_or("default");
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                if let Some(params) = &request.params {
                    manager.query_sql_with_json(&db_id, txn_id.as_deref(), &request.sql, params.clone(), dataset_id).await
                } else {
                    manager.query_sql(&db_id, txn_id.as_deref(), &request.sql, dataset_id).await
                }
            });

            match result {
                Ok(dataset) => {
                    let dataset_json = serde_json::to_value(&dataset).unwrap_or(serde_json::Value::Null);
                    Ok(Self::ok_response(None, Some(dataset_json), None))
                }
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:database", "query_sql", query_fn)?;

        // cmx:database/txn_begin — 开启事务
        let db_manager = self.db_manager.clone();
        let begin_fn: HostFuncWrapper = Box::new(move |caller, _input| {
            let db_id = caller.caller_data().db_id.clone();
            let manager = db_manager.clone();

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                let ctx = manager.get_transaction_context();
                ctx.begin(&db_id).await
            });

            match result {
                Ok(txn_id) => Ok(Self::ok_response(None, None, Some(txn_id))),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:database", "txn_begin", begin_fn)?;

        // cmx:database/txn_commit — 提交事务
        let db_manager = self.db_manager.clone();
        let commit_fn: HostFuncWrapper = Box::new(move |caller, _input| {
            let txn_id = caller.caller_data().txn_id.clone().unwrap_or_default();
            let manager = db_manager.clone();

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                let ctx = manager.get_transaction_context();
                ctx.commit(&txn_id).await
            });

            match result {
                Ok(()) => Ok(Self::ok_response(None, None, None)),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:database", "txn_commit", commit_fn)?;

        // cmx:database/txn_rollback — 回滚事务
        let db_manager = self.db_manager.clone();
        let rollback_fn: HostFuncWrapper = Box::new(move |caller, _input| {
            let txn_id = caller.caller_data().txn_id.clone().unwrap_or_default();
            let manager = db_manager.clone();

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                let ctx = manager.get_transaction_context();
                ctx.rollback(&txn_id).await
            });

            match result {
                Ok(()) => Ok(Self::ok_response(None, None, None)),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:database", "txn_rollback", rollback_fn)?;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec![
            "cmx:database/execute_sql",
            "cmx:database/query_sql",
            "cmx:database/txn_begin",
            "cmx:database/txn_commit",
            "cmx:database/txn_rollback",
        ]
    }
}
