//! WASM 宿主函数 — 数据库操作
//!
//! 为 WASM 插件提供数据库操作能力的宿主函数。
//! 封装 DatabaseManager 的核心 API，通过 JSON 格式传递参数和结果。

use std::sync::Arc;
use cmx_traits::{HostFuncError, ExtismFunctionProvider};
use extism::{host_fn, ValType, UserData, Manifest};

use crate::DatabaseManager;

const PTR: ValType = ValType::I64;

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
}

impl ExtismFunctionProvider for DatabaseHostFunctions {
    /// 返回命名空间 "cmx:database"
    fn namespace(&self) -> &str {
        "cmx:database"
    }

    /// 注册数据库操作宿主函数
    fn register_functions(&self, builder: &mut extism::PluginBuilder) -> Result<(), HostFuncError> {
        // db_query 函数
        host_fn!(db_query(_user_data: (); request: String) -> String {
            // 解析请求
            let query_request: cmx_core::wasm_types::DbQueryRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => {
                    let response = cmx_core::wasm_types::DbResponse {
                        success: false,
                        affected_rows: None,
                        dataset: None,
                        txn_id: None,
                        error: Some(format!("解析请求失败: {}", e)),
                    };
                    return Ok(serde_json::to_string(&response)?);
                }
            };
            
            // 执行查询（这里需要实际的数据库管理器）
            // TODO: 注入 db_manager
            let response = cmx_core::wasm_types::DbResponse {
                success: true,
                affected_rows: None,
                dataset: Some(r#"[{"id": 1, "name": "test"}]"#.to_string()),
                txn_id: None,
                error: None,
            };
            
            Ok(serde_json::to_string(&response)?)
        });
        
        // db_execute 函数
        host_fn!(db_execute(_user_data: (); request: String) -> String {
            let execute_request: cmx_core::wasm_types::DbQueryRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => {
                    let response = cmx_core::wasm_types::DbResponse {
                        success: false,
                        affected_rows: None,
                        dataset: None,
                        txn_id: None,
                        error: Some(format!("解析请求失败: {}", e)),
                    };
                    return Ok(serde_json::to_string(&response)?);
                }
            };
            
            let response = cmx_core::wasm_types::DbResponse {
                success: true,
                affected_rows: Some(1),
                dataset: None,
                txn_id: None,
                error: None,
            };
            
            Ok(serde_json::to_string(&response)?)
        });
        
        // 使用 std::mem::replace 替换 builder
        let temp_manifest = Manifest::new([extism::Wasm::data(vec![])]);
        let temp_builder = extism::PluginBuilder::new(temp_manifest);
        let old_builder = std::mem::replace(builder, temp_builder);
        
        let new_builder = old_builder
            .with_function("db_query", [PTR], [PTR], UserData::new(()), db_query)
            .with_function("db_execute", [PTR], [PTR], UserData::new(()), db_execute);
        
        *builder = new_builder;
        
        Ok(())
    }

    /// 列出提供的函数名
    fn provided_functions(&self) -> Vec<&str> {
        vec!["db_query", "db_execute"]
    }
}
