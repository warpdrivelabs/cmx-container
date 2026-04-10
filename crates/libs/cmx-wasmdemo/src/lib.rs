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
//! pub fn my_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
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
//!     Ok(Json(FunctionOutput {
//!         result: "处理结果".to_string(),
//!     }))
//! }
//! ```

use extism_pdk::*;
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, DbQueryRequest};
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
pub fn count_vowels(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 从标准入参获取当前步骤输入
    let input_str = &input.input;

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
    Ok(Json(FunctionOutput {
        result: result.to_string(),
    }))
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
pub fn demo_log(Json(_input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
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
    Ok(Json(FunctionOutput {
        result: serde_json::to_string(&response)?,
    }))
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
pub fn demo_cache(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 解析业务参数
    let request: DemoRequest = serde_json::from_str(&input.input)
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    // 写入缓存
    let set_response = HostCaller::cache_set(
        &request.name,
        &request.count.to_string(),
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
    Ok(Json(FunctionOutput {
        result: serde_json::to_string(&response)?,
    }))
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
pub fn demo_database(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 解析业务参数
    let request: DemoRequest = serde_json::from_str(&input.input)
        .unwrap_or(DemoRequest {
            name: "default".to_string(),
            count: 0,
        });

    // 构建数据库查询请求
    let query_request = DbQueryRequest {
        sql: format!("SELECT '{}' as name, {} as count", request.name, request.count),
        params: None,
        dataset_id: None,
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
    Ok(Json(FunctionOutput {
        result: serde_json::to_string(&response)?,
    }))
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
pub fn demo_plugin_call(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 解析业务参数
    let request: DemoRequest = serde_json::from_str(&input.input)
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
    Ok(Json(FunctionOutput {
        result: serde_json::to_string(&response)?,
    }))
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
pub fn run_all_demos(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 解析业务参数
    let request: DemoRequest = serde_json::from_str(&input.input)
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
    match HostCaller::cache_set(&request.name, &request.count.to_string(), Some(3600)) {
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
    let query_request = DbQueryRequest {
        sql: format!("SELECT * from cmx_meta_table_define_version"),
        params: None,
        dataset_id: None,
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
    let _ = HostCaller::log_info(serde_json::to_string(&results).unwrap().as_str());

    // 返回标准出参
    Ok(Json(FunctionOutput {
        result: serde_json::to_string(&results)?,
    }))
}
