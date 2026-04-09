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
//! # 导出函数
//!
//! - `count_vowels` - 统计字符串中的元音字母数量
//! - `demo_log` - 演示日志功能
//! - `demo_cache` - 演示缓存功能
//! - `demo_database` - 演示数据库功能
//! - `demo_plugin_call` - 演示插件间调用

use extism_pdk::*;
use cmx_plugin_sdk::{HostCaller, DbQueryRequest};
use serde::{Deserialize, Serialize};

/// 示例请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequest {
    /// 名称
    pub name: String,
    /// 计数
    pub count: u32,
}

/// 示例响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoResponse {
    /// 消息
    pub message: String,
    /// 总数
    pub total: u32,
}

/// 统计字符串中的元音字母数量
///
/// # 参数
/// - `input`: 输入字符串
///
/// # 返回值
/// 返回 JSON 格式的统计结果
#[plugin_fn]
pub fn count_vowels(input: String) -> FnResult<String> {
    let vowels = ['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];
    let count = input.chars().filter(|c| vowels.contains(c)).count();

    let response = serde_json::json!({
        "count": count,
        "total": count,
        "input": input,
    });

    Ok(response.to_string())
}

/// 演示日志功能
///
/// 调用宿主的日志函数，记录不同级别的日志信息
#[plugin_fn]
pub fn demo_log() -> FnResult<String> {
    // 记录信息日志
    HostCaller::log_info("Hello from WASM demo!")?;

    // 记录错误日志
    HostCaller::log_error("This is an error from WASM demo!")?;

    // 记录调试日志
    HostCaller::log_debug("This is a debug message!")?;

    // 记录警告日志
    HostCaller::log_warn("This is a warning!")?;

    let response = DemoResponse {
        message: "日志演示完成".to_string(),
        total: 4,
    };

    Ok(serde_json::to_string(&response)?)
}

/// 演示缓存功能
///
/// 演示缓存的写入、读取和删除操作
#[plugin_fn]
pub fn demo_cache(Json(request): Json<DemoRequest>) -> FnResult<Json<DemoResponse>> {
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

    // 返回响应
    Ok(Json(DemoResponse {
        message: format!("缓存操作成功: {:?}", get_response),
        total: request.count,
    }))
}

/// 演示数据库查询功能
///
/// 执行一个简单的数据库查询
#[plugin_fn]
pub fn demo_database(Json(request): Json<DemoRequest>) -> FnResult<Json<DemoResponse>> {
    // 执行数据库查询
    let query_request = DbQueryRequest {
        sql: format!("SELECT '{}' as name, {} as count", request.name, request.count),
        params: None,
        dataset_id: None,
    };

    let db_response = HostCaller::db_query(query_request)?;

    HostCaller::log_info(&format!("数据库查询结果: {:?}", db_response))?;

    // 返回响应
    Ok(Json(DemoResponse {
        message: format!("数据库查询成功: {:?}", db_response),
        total: request.count,
    }))
}

/// 演示插件间调用
///
/// 调用其他插件的服务
#[plugin_fn]
pub fn demo_plugin_call(Json(request): Json<DemoRequest>) -> FnResult<Json<DemoResponse>> {
    // 调用其他插件
    let input_json = serde_json::to_string(&request)?;
    let plugin_response = HostCaller::call_service(
        "other-plugin",
        "some_function",
        &input_json,
    )?;

    HostCaller::log_info(&format!("插件调用结果: {:?}", plugin_response))?;

    // 返回响应
    Ok(Json(DemoResponse {
        message: format!("插件调用成功: {:?}", plugin_response),
        total: request.count,
    }))
}

/// 综合测试入口
///
/// 执行所有演示功能
#[plugin_fn]
pub fn run_all_demos(Json(request): Json<DemoRequest>) -> FnResult<String> {
    let mut results = Vec::new();

    // 测试日志
    match HostCaller::log_info("测试日志") {
        Ok(_) => results.push("日志测试: 成功".to_string()),
        Err(e) => results.push(format!("日志测试失败: {}", e)),
    }
    HostCaller::log_info("测试缓存写入");
    // 测试缓存
    match HostCaller::cache_set(&request.name, &request.count.to_string(), Some(3600)) {
        Ok(_) => results.push("缓存写入测试: 成功".to_string()),
        Err(e) => results.push(format!("缓存写入测试失败: {}", e)),
    }
    HostCaller::log_info("缓存读取测试");

    match HostCaller::cache_get(&request.name) {
        Ok(resp) => results.push(format!("缓存读取测试: {:?}", resp)),
        Err(e) => results.push(format!("缓存读取测试失败: {}", e)),
    }
    HostCaller::log_info("测试数据库");

    // 测试数据库
    let query_request = DbQueryRequest {
        sql: format!("SELECT * from cmx_meta_table_define_version"),
        params: None,
        dataset_id: None,
    };
    match HostCaller::db_query(query_request) {
        Ok(resp) => results.push(format!("数据库测试: {:?}", resp)),
        Err(e) => results.push(format!("数据库测试失败: {}", e)),
    }
    HostCaller::log_info("测试插件调用");

   // 测试插件调用
    let input_json = serde_json::to_string(&request)?;
    match HostCaller::call_service(
        "other-plugin",
        "some_function",
        &input_json,
    ) {
        Ok(resp) => results.push(format!("插件调用测试: {:?}", resp)),
        Err(e) => results.push(format!("插件调用测试失败: {}", e)),
    }
    HostCaller::log_info(serde_json::to_string(&results).unwrap().as_str());

    Ok(serde_json::to_string(&results)?)
}
