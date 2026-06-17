//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 MsgPack 格式传递参数和结果。

use std::sync::Arc;
use cmx_traits::error::HostFuncError;
use cmx_traits::runtime::{HostFunctionProvider, HostFunctionDef};
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
    /// - `input`: MsgPack 编码的查询请求（DbRequest）
    ///
    /// # 返回
    /// - MsgPack 编码的响应（DbResponse）
    fn do_query(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let request: DbRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => {
                return Ok(rmp_serde::to_vec(&Self::error_response(format!("解析请求失败: {}", e))).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = request.sql;
        let params = request.params;
        let dataset_id = request.dataset_id.unwrap_or_else(|| "wasm_query".to_string());
        let request_db_id = request.db_id;
        let request_txn_id = request.txn_id;

        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
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

        Ok(rmp_serde::to_vec(&response).unwrap_or_default())
    }

    /// 执行数据库操作（INSERT/UPDATE/DELETE）
    ///
    /// # 参数
    /// - `input`: MsgPack 编码的执行请求（DbRequest）
    ///
    /// # 返回
    /// - MsgPack 编码的响应（DbResponse）
    fn do_execute(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let request: DbRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => {
                return Ok(rmp_serde::to_vec(&Self::error_response(format!("解析请求失败: {}", e))).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = request.sql;
        let params = request.params;
        let request_db_id = request.db_id;
        let request_txn_id = request.txn_id;

        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
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

        Ok(rmp_serde::to_vec(&response).unwrap_or_default())
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
            HostFunctionDef::msgpack_fn("db_query", "cmx:database"),
            HostFunctionDef::msgpack_fn("db_execute", "cmx:database"),
        ]
    }

    /// 调用宿主函数
    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
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
