//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 JSON 格式传递参数和结果。

use std::sync::Arc;
use cmx_traits::{HostFuncError, HostFunctionProvider, HostFunctionDef};
use cmx_core::wasm_types::{DbRequest, DbResponse};

use crate::DatabaseManager;

/// 数据库宿主函数提供者
///
/// 封装 DatabaseManager 的核心 API，向 WASM 运行时注册数据库操作宿主函数。
pub struct DatabaseHostFunctions {
    /// 数据库管理器引用
    db_manager: Arc<DatabaseManager>,
}

impl DatabaseHostFunctions {
    /// 创建数据库宿主函数提供者
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    /// 构建错误响应
    fn error_response(msg: impl Into<String>) -> DbResponse {
        DbResponse {
            success: false,
            affected_rows: None,
            dataset: None,
            txn_id: None,
            error: Some(msg.into()),
        }
    }

    /// 执行数据库查询
    ///
    /// # 参数
    /// - `input`: JSON 格式的查询请求，包含 sql、params、dataset_id、db_id、txn_id 字段
    ///
    /// # 返回
    /// - JSON 格式的响应，包含 success、dataset、txn_id、error 字段
    fn do_query(&self, input: String) -> Result<String, HostFuncError> {
        let request: DbRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                return Ok(serde_json::to_string(&Self::error_response(format!("解析请求失败: {}", e))).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = request.sql;
        let params = request.params;
        let dataset_id = request.dataset_id.unwrap_or_else(|| "wasm_query".to_string());
        let request_db_id = request.db_id;
        let request_txn_id = request.txn_id;

        // 当前已在 spawn_blocking 线程中，直接使用 block_on
        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                // 获取数据库ID：优先使用请求中的 db_id，否则使用默认数据库
                let db_id = match request_db_id {
                    Some(ref id) if !id.is_empty() => id.clone(),
                    _ => db_manager.get_default_db_id().await,
                };

                match params {
                    Some(params_value) => {
                        db_manager
                            .query_sql_with_json(&db_id, request_txn_id.as_deref(), &sql, params_value, &dataset_id)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    None => {
                        db_manager
                            .query_sql(&db_id, request_txn_id.as_deref(), &sql, &dataset_id)
                            .await
                            .map_err(|e| e.to_string())
                    }
                }
            })
        };

        let response = match result {
            Ok(dataset) => DbResponse {
                success: true,
                affected_rows: None,
                dataset: Some(dataset),
                txn_id: request_txn_id,
                error: None,
            },
            Err(e) => Self::error_response(e),
        };

        Ok(serde_json::to_string(&response).unwrap_or_default())
    }

    /// 执行数据库操作（INSERT/UPDATE/DELETE）
    ///
    /// # 参数
    /// - `input`: JSON 格式的执行请求，包含 sql、params、db_id、txn_id 字段
    ///
    /// # 返回
    /// - JSON 格式的响应，包含 success、affected_rows、txn_id、error 字段
    fn do_execute(&self, input: String) -> Result<String, HostFuncError> {
        let request: DbRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                return Ok(serde_json::to_string(&Self::error_response(format!("解析请求失败: {}", e))).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = request.sql;
        let params = request.params;
        let request_db_id = request.db_id;
        let request_txn_id = request.txn_id;

        // 当前已在 spawn_blocking 线程中，直接使用 block_on
        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                // 获取数据库ID：优先使用请求中的 db_id，否则使用默认数据库
                let db_id = match request_db_id {
                    Some(ref id) if !id.is_empty() => id.clone(),
                    _ => db_manager.get_default_db_id().await,
                };

                match params {
                    Some(params_value) => {
                        db_manager
                            .execute_sql_with_json(&db_id, request_txn_id.as_deref(), &sql, params_value)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    None => {
                        db_manager
                            .execute_sql(&db_id, request_txn_id.as_deref(), &sql)
                            .await
                            .map_err(|e| e.to_string())
                    }
                }
            })
        };

        let response = match result {
            Ok(affected_rows) => DbResponse {
                success: true,
                affected_rows: Some(affected_rows),
                dataset: None,
                txn_id: request_txn_id,
                error: None,
            },
            Err(e) => Self::error_response(e),
        };

        Ok(serde_json::to_string(&response).unwrap_or_default())
    }
}

impl HostFunctionProvider for DatabaseHostFunctions {
    /// 返回命名空间 "cmx:database"
    fn namespace(&self) -> &str {
        "cmx:database"
    }

    /// 返回提供的宿主函数列表
    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            HostFunctionDef::json_fn("db_query", "cmx:database"),
            HostFunctionDef::json_fn("db_execute", "cmx:database"),
        ]
    }

    /// 调用宿主函数
    fn call(&self, name: &str, input: String) -> Result<String, HostFuncError> {
        match name {
            "db_query" => self.do_query(input),
            "db_execute" => self.do_execute(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    /// 列出提供的函数名
    fn provided_functions(&self) -> Vec<&str> {
        vec!["db_query", "db_execute"]
    }
}
