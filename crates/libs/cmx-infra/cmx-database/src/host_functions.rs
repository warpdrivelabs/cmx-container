//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 JSON 格式传递参数和结果。

use std::sync::Arc;
use cmx_traits::{HostFuncError, HostFunctionProvider, HostFunctionDef};

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
    fn do_query(&self, input: String) -> Result<String, HostFuncError> {
        let _query_request: cmx_core::wasm_types::DbQueryRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                let response = cmx_core::wasm_types::DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(format!("解析请求失败: {}", e)),
                };
                return Ok(serde_json::to_string(&response).unwrap_or_default());
            }
        };

        // TODO: 实现实际的数据库查询逻辑
        // 这里需要使用 self.db_manager 执行查询

        // 返回模拟响应
        let response = cmx_core::wasm_types::DbResponse {
            success: true,
            affected_rows: None,
            dataset: Some(r#"[{"id": 1, "name": "test"}]"#.to_string()),
            txn_id: None,
            error: None,
        };

        Ok(serde_json::to_string(&response).unwrap_or_default())
    }

    /// 执行数据库操作
    fn do_execute(&self, input: String) -> Result<String, HostFuncError> {
        let _execute_request: cmx_core::wasm_types::DbQueryRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => {
                let response = cmx_core::wasm_types::DbResponse {
                    success: false,
                    affected_rows: None,
                    dataset: None,
                    txn_id: None,
                    error: Some(format!("解析请求失败: {}", e)),
                };
                return Ok(serde_json::to_string(&response).unwrap_or_default());
            }
        };

        // TODO: 实现实际的数据库执行逻辑

        let response = cmx_core::wasm_types::DbResponse {
            success: true,
            affected_rows: Some(1),
            dataset: None,
            txn_id: None,
            error: None,
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
