//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 JSON 格式传递参数和结果。

use std::sync::Arc;
use cmx_traits::{HostFuncError, HostFunctionProvider, HostFunctionDef};
use cmx_core::wasm_types::{DbQueryRequest, DbResponse};

use crate::DatabaseManager;

/// 数据库宿主函数提供者
///
/// 封装 DatabaseManager 的核心 API，向 WASM 运行时注册数据库操作宿主函数。
/// 所有数据库操作通过 CallerData 中的 db_id 和 txn_id 确定目标数据库和事务上下文。
pub struct DatabaseHostFunctions {
    /// 数据库管理器引用
    db_manager: Arc<DatabaseManager>,
}

impl DatabaseHostFunctions {
    /// 创建数据库宿主函数提供者
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }

    /// 执行数据库查询
    ///
    /// # 参数
    /// - `input`: JSON 格式的查询请求，包含 sql、params、dataset_id 字段
    ///
    /// # 返回
    /// - JSON 格式的响应，包含 success、dataset、error 字段
    fn do_query(&self, input: String) -> Result<String, HostFuncError> {
        let query_request: DbQueryRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                let response = DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(format!("解析请求失败: {}", e)),
                };
                return Ok(serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = query_request.sql.clone();
        let params = query_request.params.clone();
        let dataset_id = query_request.dataset_id.clone().unwrap_or_else(|| "wasm_query".to_string());

        // 使用 block_in_place 允许在异步上下文中执行阻塞操作
        let result = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                // 获取默认数据库 ID
                let db_id = db_manager.get_default_db_id().await;
                
                // 根据是否有参数选择不同的查询方法
                if let Some(params_str) = params {
                    // 解析参数为 JSON Value
                    let params_value: serde_json::Value = match serde_json::from_str(&params_str) {
                        Ok(v) => v,
                        Err(e) => {
                            return Err(format!("解析参数失败: {}", e));
                        }
                    };
                    
                    db_manager
                        .query_sql_with_json(&db_id, None, &sql, params_value, &dataset_id)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    db_manager
                        .query_sql(&db_id, None, &sql, &dataset_id)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

        match result {
            Ok(dataset) => {
                // 将 DataSet 序列化为 JSON 字符串
                let dataset_json = serde_json::to_string(&dataset).unwrap_or_default();
                let response = DbResponse {
                    success: true,
                    affected_rows: None,
                    dataset: Some(dataset_json),
                    txn_id: None,
                    error: None,
                };
                Ok(serde_json::to_string(&response).unwrap_or_default())
            }
            Err(e) => {
                let response = DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(e),
                };
                Ok(serde_json::to_string(&response).unwrap_or_default())
            }
        }
    }

    /// 执行数据库操作（INSERT/UPDATE/DELETE）
    ///
    /// # 参数
    /// - `input`: JSON 格式的执行请求，包含 sql、params 字段
    ///
    /// # 返回
    /// - JSON 格式的响应，包含 success、affected_rows、error 字段
    fn do_execute(&self, input: String) -> Result<String, HostFuncError> {
        let execute_request: DbQueryRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                let response = DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(format!("解析请求失败: {}", e)),
                };
                return Ok(serde_json::to_string(&response).unwrap_or_default());
            }
        };

        let db_manager = self.db_manager.clone();
        let sql = execute_request.sql.clone();
        let params = execute_request.params.clone();

        // 使用 block_in_place 允许在异步上下文中执行阻塞操作
        let result = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                // 获取默认数据库 ID
                let db_id = db_manager.get_default_db_id().await;
                
                // 根据是否有参数选择不同的执行方法
                if let Some(params_str) = params {
                    // 解析参数为 JSON Value
                    let params_value: serde_json::Value = match serde_json::from_str(&params_str) {
                        Ok(v) => v,
                        Err(e) => {
                            return Err(format!("解析参数失败: {}", e));
                        }
                    };
                    
                    db_manager
                        .execute_sql_with_json(&db_id, None, &sql, params_value)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    db_manager
                        .execute_sql(&db_id, None, &sql)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
        });

        match result {
            Ok(affected_rows) => {
                let response = DbResponse {
                    success: true,
                    affected_rows: Some(affected_rows),
                    dataset: None,
                    txn_id: None,
                    error: None,
                };
                Ok(serde_json::to_string(&response).unwrap_or_default())
            }
            Err(e) => {
                let response = DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(e),
                };
                Ok(serde_json::to_string(&response).unwrap_or_default())
            }
        }
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
