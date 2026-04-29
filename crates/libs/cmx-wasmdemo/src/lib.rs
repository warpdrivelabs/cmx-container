//! cmx-wasmdemo - WASM 插件演示模块
//!
//! 本模块用于验证 Extism 插件功能，提供各种演示函数。
//!
//! # 编译目标
//!
//! 使用 `wasm32-unknown-unknown` 目标编译：
//! ```bash
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! # 标准函数签名
//!
//! 所有服务编排中的函数都使用统一的入参和出参格式：
//!
//! - **入参**: `FunctionInput` — 包含当前步骤输入和服务调用上下文
//! - **出参**: `FunctionOutput` — 包含函数执行结果
//!
//! # 导出函数
//!
//! | 函数名 | 功能 | 适用场景 |
//! |--------|------|----------|
//! | `count_vowels` | 统计元音字母 | 简单字符串处理 |
//! | `demo_log` | 日志演示 | 调试日志功能 |
//! | `demo_cache` | 缓存演示 | 缓存读写操作 |
//! | `demo_database` | 数据库演示 | 数据库查询 |
//! | `demo_plugin_call` | 插件调用演示 | 插件间调用 |
//! | `run_all_demos` | 综合测试 | 功能验证 |
//!
//! # 使用示例
//!
//! ```rust
//! use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
//! use extism_pdk::*;
//!
//! #[plugin_fn]
//! pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
//!     // 获取当前步骤输入
//!     let current_input = &input.input;
//!
//!     // 获取初始入参（API 请求传入的参数）
//!     let initial_input = &input.context.initial_input;
//!
//!     // 获取请求头
//!     let headers = &input.context.headers;
//!
//!     // 获取前序步骤输出
//!     if let Some(prev_output) = input.context.get_step_output("previous_node_id") {
//!         // 使用前序步骤输出
//!     }
//!
//!     // 返回结果
//!     Ok(Msgpack(FunctionOutput::new("处理结果")))
//! }
//! ```

use extism_pdk::*;
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, DbRequest};
use serde::{Deserialize, Serialize};

// ==================== 业务数据结构 ====================

/// 示例请求 — 用于演示函数的业务参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequest {
    /// 名称
    pub name: String,
    /// 计数
    pub count: u32,
}

/// 示例响应 — 用于演示函数的业务结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoResponse {
    /// 消息
    pub message: String,
    /// 总数
    pub total: u32,
}

// ==================== 演示函数 ====================

/// 统计字符串中的元音字母数量
///
/// 这是一个简单的字符串处理函数，演示标准入参出参的使用。
///
/// # 输入处理
/// - `input.input`: 要统计的字符串
///
/// # 输出
/// - `result`: JSON 格式的统计结果
///
/// # 示例
/// 输入: `{"input": "hello world", "context": {...}}`
/// 输出: `{"result": "{\"count\":3,\"total\":3,\"input\":\"hello world\"}"}`
#[plugin_fn]
pub fn count_vowels(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 从标准入参获取当前步骤输入（现在是 Value 类型）
    let input_str = input.input.as_str().unwrap_or_default();

    // 统计元音字母
    let vowels = ['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];
    let count = input_str.chars().filter(|c| vowels.contains(c)).count();

    // 构建结果 JSON
    let result = serde_json::json!({
        "count": count,
        "total": count,
        "input": input_str,
    });

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 演示日志功能
///
/// 调用宿主的日志函数，记录不同级别的日志信息。
/// 此函数不需要业务输入，直接执行日志演示。
///
/// # 输入处理
/// - 忽略 `input.input`，仅用于演示
///
/// # 输出
/// - `result`: JSON 格式的演示结果
#[plugin_fn]
pub fn demo_log(Msgpack(_input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 记录信息日志
    HostCaller::log_info("Hello from WASM demo!")?;

    // 记录错误日志
    HostCaller::log_error("This is an error from WASM demo!")?;

    // 记录调试日志
    HostCaller::log_debug("This is a debug message!")?;

    // 记录警告日志
    HostCaller::log_warn("This is a warning!")?;

    // 构建响应
    let response = DemoResponse {
        message: "日志演示完成".to_string(),
        total: 4,
    };

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(&response)?)))
}

/// 演示缓存功能
///
/// 演示缓存的写入、读取和删除操作。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，包含 name 和 count
///
/// # 输出
/// - `result`: JSON 格式的操作结果
///
/// # 示例
/// 输入: `{"input": "{\"name\":\"test\",\"count\":100}", "context": {...}}`
/// 输出: `{"result": "{\"message\":\"缓存操作成功: ...\",\"total\":100}"}`
#[plugin_fn]
pub fn demo_cache(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 解析业务参数（input.input 现在是 Value，需要转为字符串）
    let request: DemoRequest = serde_json::from_value(input.input.clone())
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    // 写入缓存
    let set_response = HostCaller::cache_set(
        &request.name,
        serde_json::Value::String(request.count.to_string()),
        Some(3600),
    )?;

    HostCaller::log_info(&format!("缓存写入结果: {:?}", set_response))?;

    // 读取缓存
    let get_response = HostCaller::cache_get(&request.name)?;

    HostCaller::log_info(&format!("缓存读取结果: {:?}", get_response))?;

    // 构建响应
    let response = DemoResponse {
        message: format!("缓存操作成功: {:?}", get_response),
        total: request.count,
    };

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(&response)?)))
}

/// 演示数据库查询功能
///
/// 执行一个简单的数据库查询。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，用于构建 SQL
///
/// # 输出
/// - `result`: JSON 格式的查询结果
///
/// # 事务支持
/// 如果 `input.txn_id` 存在，函数将在指定事务中执行 SQL。
#[plugin_fn]
pub fn demo_database(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 解析业务参数（input.input 现在是 Value）
    let request: DemoRequest = serde_json::from_value(input.input.clone())
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    // 构建数据库查询请求
    let query_request = DbRequest {
        sql: format!("SELECT '{}' as name, {} as count", request.name, request.count),
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    // 执行数据库查询
    // 注意：如果 input.txn_id 存在，应该在事务中执行
    let db_response = HostCaller::db_query(query_request)?;

    HostCaller::log_info(&format!("数据库查询结果: {:?}", db_response))?;

    // 构建响应
    let response = DemoResponse {
        message: format!("数据库查询成功: {:?}", db_response),
        total: request.count,
    };

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(&response)?)))
}

/// 演示插件间调用
///
/// 调用其他插件的服务。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，传递给目标插件
///
/// # 输出
/// - `result`: JSON 格式的调用结果
#[plugin_fn]
pub fn demo_plugin_call(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 解析业务参数（input.input 现在是 Value）
    let request: DemoRequest = serde_json::from_value(input.input.clone())
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    // 调用其他插件
    let input_json = serde_json::to_string(&request)?;
    let plugin_response = HostCaller::call_service(
        "other-plugin",
        "some_function",
        &input_json,
    )?;

    HostCaller::log_info(&format!("插件调用结果: {:?}", plugin_response))?;

    // 构建响应
    let response = DemoResponse {
        message: format!("插件调用成功: {:?}", plugin_response),
        total: request.count,
    };

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(&response)?)))
}

/// 综合测试入口
///
/// 执行所有演示功能，用于验证插件环境是否正常。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，用于测试参数传递
///
/// # 输出
/// - `result`: JSON 数组，包含各项测试的结果
///
/// # 测试项
/// 1. 日志功能测试
/// 2. 缓存写入测试
/// 3. 缓存读取测试
/// 4. 数据库查询测试
/// 5. 插件调用测试
#[plugin_fn]
pub fn run_all_demos(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 解析业务参数（input.input 现在是 Value）
    let request: DemoRequest = serde_json::from_value(input.input.clone())
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    let mut results = Vec::new();


    // ==================== 测试日志功能 ====================
    match HostCaller::log_info("测试日志") {
        Ok(_) => results.push("日志测试: 成功".to_string()),
        Err(e) => results.push(format!("日志测试失败: {}", e)),
    }
    let _ = HostCaller::log_info("测试缓存写入");

    // ==================== 测试缓存写入 ====================
    match HostCaller::cache_set(&request.name, serde_json::Value::String(request.count.to_string()), Some(3600)) {
        Ok(_) => results.push("缓存写入测试: 成功".to_string()),
        Err(e) => results.push(format!("缓存写入测试失败: {}", e)),
    }
    let _ = HostCaller::log_info("缓存读取测试");

    // ==================== 测试缓存读取 ====================
    match HostCaller::cache_get(&request.name) {
        Ok(resp) => results.push(format!("缓存读取测试: {:?}", resp)),
        Err(e) => results.push(format!("缓存读取测试失败: {}", e)),
    }
    let _ = HostCaller::log_info("测试数据库");

    // ==================== 测试数据库 ====================
    let query_request = DbRequest {
        sql: "SELECT * from cmx_meta_table_define_version".to_string(),
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };
    match HostCaller::db_query(query_request) {
        Ok(resp) => results.push(format!("数据库测试: {:?}", resp)),
        Err(e) => results.push(format!("数据库测试失败: {}", e)),
    }
    let _ = HostCaller::log_info("测试插件调用");

    // ==================== 测试插件调用 ====================
    let input_json = serde_json::to_string(&request)?;
    match HostCaller::call_service(
        "other-plugin",
        "some_function",
        &input_json,
    ) {
        Ok(resp) => results.push(format!("插件调用测试: {:?}", resp)),
        Err(e) => results.push(format!("插件调用测试失败: {}", e)),
    }
    // let _ = HostCaller::log_info(serde_json::to_string(&results).unwrap().as_str());

    // 返回标准出参
    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(&results)?)))
}

// ==================== 服务编排测试函数 ====================

/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识。
/// 用于 skylake-switch 节点，返回值对应 options 中的选项。
///
/// # 输入处理
/// - `input.input`: JSON 格式，包含 route 字段
///
/// # 输出
/// - `result`: 返回 "1"、"2" 或 "3"，对应三个分支
///
/// # 示例
/// 输入: `{"input": "{\"route\":\"1\"}", "context": {...}}`
/// 输出: `{"result": "1"}`
#[plugin_fn]
#[doc_type = "branch_fn"]
pub fn route_check(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct RouteInput {
        route: String,
    }

    let route_input: RouteInput = serde_json::from_value(input.input.clone())
        .unwrap_or(RouteInput {
            route: "1".to_string(),
        });

    let route = route_input.route.trim();
    let result = match route {
        "1" => "1",
        "2" => "2",
        "3" => "3",
        "4" => "4",
        _ => "1",
    };

    HostCaller::log_info(&format!("路由判断: route={}, 返回分支={}", route, result))?;

    Ok(Msgpack(FunctionOutput::from_json(serde_json::to_value(result)?)))
}

/// 分支1处理函数
///
/// 处理分支1的业务逻辑。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_1_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    HostCaller::log_info("执行分支1处理")?;

    let result = serde_json::json!({
        "branch": "1",
        "message": "分支1处理完成",
        "input": input.input,
        "initial_input": input.context.initial_input,
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 分支2处理函数
///
/// 处理分支2的业务逻辑。
///
/// # 输入处理
/// - `input.input`: JSON 格式的业务数据
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_2_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    HostCaller::log_info("执行分支2处理")?;

    let result = serde_json::json!({
        "branch": "2",
        "message": "分支2处理完成",
        "input": input.input,
        "initial_input": input.context.initial_input,
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 分支3处理函数
///
/// 处理分支3的业务逻辑。
///
/// # 输入处理
/// - `input.input`: JSON 格式的业务数据
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_3_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    HostCaller::log_info("执行分支3处理")?;

    let result = serde_json::json!({
        "branch": "3",
        "message": "分支3处理完成",
        "input": input.input,
        "initial_input": input.context.initial_input,
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 合并结果函数
///
/// 合并各分支的处理结果。
/// 可以通过 step_outputs 获取各分支的输出。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.step_outputs`: 各步骤的输出缓存
///
/// # 输出
/// - `result`: JSON 格式的合并结果
#[plugin_fn]
pub fn merge_result(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    HostCaller::log_info("执行合并结果处理")?;

    let branch_output = input.context.get_step_output("branch_1_func")
        .or_else(|| input.context.get_step_output("branch_2_func"))
        .or_else(|| input.context.get_step_output("branch_3_func"))
        .cloned()
        .unwrap_or_else(|| input.input.clone());

    let result = serde_json::json!({
        "merged": true,
        "branch_output": branch_output,
        "message": "结果合并完成",
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 事务插入函数
///
/// 在事务中执行插入操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的插入数据
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_insert(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let txn_id = input.context.txn_id.clone();
    HostCaller::log_info(&format!("执行事务插入, txn_id={:?}", txn_id))?;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct InsertData {
        table: String,
        name: String,
        value: i32,
    }

    let insert_data: InsertData = serde_json::from_value(input.input.clone())
        .unwrap_or(InsertData {
            table: "test_table".to_string(),
            name: "test".to_string(),
            value: 1,
        });

    let sql = format!(
        "INSERT INTO {} (name, value) VALUES ('{}', {})",
        insert_data.table, insert_data.name, insert_data.value
    );

    let query_request = DbRequest {
        sql,
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: txn_id.clone(),
    };

    let db_response = HostCaller::db_execute(query_request)?;

    let result = serde_json::json!({
        "operation": "insert",
        "txn_id": txn_id,
        "table": insert_data.table,
        "affected_rows": db_response.affected_rows ,
        "message": "插入完成",
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 事务更新函数
///
/// 在事务中执行更新操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的更新数据
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_update(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let txn_id = input.context.txn_id.clone();
    HostCaller::log_info(&format!("执行事务更新, txn_id={:?}", txn_id))?;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct UpdateData {
        table: String,
        name: String,
        value: i32,
    }

    let update_data: UpdateData = serde_json::from_value(input.input.clone())
        .unwrap_or(UpdateData {
            table: "test_table".to_string(),
            name: "test".to_string(),
            value: 2,
        });

    let sql = format!(
        "UPDATE {} SET value = {} WHERE name = '{}'",
        update_data.table, update_data.value, update_data.name
    );

    let query_request = DbRequest {
        sql,
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: txn_id.clone(),
    };

    let db_response = HostCaller::db_execute(query_request)?;

    let result = serde_json::json!({
        "operation": "update",
        "txn_id": txn_id,
        "table": update_data.table,
        "affected_rows": db_response.affected_rows,
        "message": "更新完成",
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 事务查询函数
///
/// 在事务中执行查询操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的查询条件
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的查询结果
#[plugin_fn]
pub fn tx_query(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let txn_id = input.context.txn_id.clone();
    HostCaller::log_info(&format!("执行事务查询, txn_id={:?}", txn_id))?;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct QueryData {
        table: String,
        name: String,
    }

    let query_data: QueryData = serde_json::from_value(input.input.clone())
        .unwrap_or(QueryData {
            table: "test_table".to_string(),
            name: "test".to_string(),
        });

    let sql = format!(
        "SELECT * FROM {} WHERE name = '{}'",
        query_data.table, query_data.name
    );

    let query_request = DbRequest {
        sql,
        params: None,
        dataset_id: Some("test_table".to_string()),
        db_id: None,
        txn_id: txn_id.clone(),
    };

    let db_response = HostCaller::db_query(query_request)?;

    let result = serde_json::json!({
        "operation": "query",
        "txn_id": txn_id,
        "table": query_data.table,
        "success": db_response.success,
        "dataset": db_response.dataset,
        "message": "查询完成",
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 事务删除函数
///
/// 在事务中执行删除操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的删除条件
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_delete(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let txn_id = input.context.txn_id.clone();
    HostCaller::log_info(&format!("执行事务删除, txn_id={:?}", txn_id))?;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct DeleteData {
        table: String,
        name: String,
    }

    let delete_data: DeleteData = serde_json::from_value(input.input.clone())
        .unwrap_or(DeleteData {
            table: "test_table".to_string(),
            name: "test1".to_string(),
        });

    let sql = format!(
        "DELETE FROM {} WHERE name = '{}'",
        delete_data.table, delete_data.name
    );

    let query_request = DbRequest {
        sql,
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: txn_id.clone(),
    };

    let db_response = HostCaller::db_execute(query_request)?;

    let result = serde_json::json!({
        "operation": "delete",
        "txn_id": txn_id,
        "table": delete_data.table,
        "affected_rows": db_response.affected_rows ,
        "message": "删除完成",
    });

    Ok(Msgpack(FunctionOutput::from_json(result)))
}

/// 最终处理函数
///
/// 最终处理并返回结果。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.step_outputs`: 各步骤的输出缓存
///
/// # 输出
/// - `result`: JSON 格式的最终结果
#[plugin_fn]
pub fn final_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    HostCaller::log_info("执行最终处理")?;

    let merge_output = input.context.get_step_output("merge_func")
        .cloned()
        .unwrap_or_else(|| input.input.clone());

    let tx_insert_output = input.context.get_step_output("tx_insert");
    let tx_update_output = input.context.get_step_output("tx_update");
    let tx_query_output = input.context.get_step_output("tx_query");
    let tx_delete_output = input.context.get_step_output("tx_delete");

    let result = serde_json::json!({
        "final": true,
        "merge_output": merge_output,
        "tx_insert_output": tx_insert_output,
        "tx_update_output": tx_update_output,
        "tx_query_output": tx_query_output,
        "tx_delete_output": tx_delete_output,
        "txn_id": input.context.txn_id,
        "message": "服务编排执行完成",
    });


    Ok(Msgpack(FunctionOutput::from_json(result)))
}
