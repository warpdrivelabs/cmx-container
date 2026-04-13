//! WASM 插件宿主函数调用封装
//!
//! 为 WASM 插件提供调用宿主函数的便捷封装。
//! 使用 extism-pdk 的 `#[host_fn]` 宏声明宿主函数签名。

use extism_pdk::*;
use cmx_core::{
    DbQueryRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    ServiceCallRequest, ServiceCallResponse,
};

// 声明日志宿主函数
#[host_fn("cmx:log")]
extern "ExtismHost" {
    /// 记录信息日志
    fn log_info(message: String) -> ();
    /// 记录错误日志
    fn log_error(message: String) -> ();
    /// 记录调试日志
    fn log_debug(message: String) -> ();
    /// 记录警告日志
    fn log_warn(message: String) -> ();
}

// 声明数据库宿主函数
#[host_fn("cmx:database")]
extern "ExtismHost" {
    /// 执行数据库查询
    fn db_query(request: String) -> String;
    /// 执行数据库操作
    fn db_execute(request: String) -> String;
}

// 声明缓存宿主函数
#[host_fn("cmx:buffer")]
extern "ExtismHost" {
    /// 获取缓存
    fn cache_get(request: String) -> String;
    /// 设置缓存
    fn cache_set(request: String) -> String;
    /// 删除缓存
    fn cache_delete(request: String) -> String;
}

// 声明插件间调用宿主函数
#[host_fn("cmx:plugin")]
extern "ExtismHost" {
    /// 调用其他插件的服务
    fn call_service(request: String) -> String;
}

/// 宿主函数调用器
///
/// 提供便捷的方法来调用宿主函数
pub struct HostCaller;

impl HostCaller {
    /// 记录信息日志
    pub fn log_info(message: &str) -> Result<(), Error> {
        unsafe { log_info(message.to_string())? };
        Ok(())
    }

    /// 记录错误日志
    pub fn log_error(message: &str) -> Result<(), Error> {
        unsafe { log_error(message.to_string())? };
        Ok(())
    }

    /// 记录调试日志
    pub fn log_debug(message: &str) -> Result<(), Error> {
        unsafe { log_debug(message.to_string())? };
        Ok(())
    }

    /// 记录警告日志
    pub fn log_warn(message: &str) -> Result<(), Error> {
        unsafe { log_warn(message.to_string())? };
        Ok(())
    }

    /// 执行数据库查询
    pub fn db_query(request: DbQueryRequest) -> Result<DbResponse, Error> {
        let json = serde_json::to_string(&request)?;
        let result = unsafe { db_query(json)? };
        let response: DbResponse = serde_json::from_str(&result)?;
        Ok(response)
    }

    /// 执行数据库操作
    pub fn db_execute(request: DbQueryRequest) -> Result<DbResponse, Error> {
        let json = serde_json::to_string(&request)?;
        let result = unsafe { db_execute(json)? };
        let response: DbResponse = serde_json::from_str(&result)?;
        Ok(response)
    }

    /// 获取缓存
    pub fn cache_get(key: &str) -> Result<CacheResponse, Error> {
        let request = CacheGetRequest {
            key: key.to_string(),
        };
        let json = serde_json::to_string(&request)?;
        let result = unsafe { cache_get(json)? };
        let response: CacheResponse = serde_json::from_str(&result)?;
        Ok(response)
    }

    /// 设置缓存
    pub fn cache_set(key: &str, value: &str, ttl_seconds: Option<u64>) -> Result<CacheResponse, Error> {
        let request = CacheSetRequest {
            key: key.to_string(),
            value: value.to_string(),
            ttl_seconds,
        };
        let json = serde_json::to_string(&request)?;
        let result = unsafe { cache_set(json)? };
        let response: CacheResponse = serde_json::from_str(&result)?;
        Ok(response)
    }

    /// 删除缓存
    pub fn cache_delete(key: &str) -> Result<CacheResponse, Error> {
        let request = CacheGetRequest {
            key: key.to_string(),
        };
        let json = serde_json::to_string(&request)?;
        let result = unsafe { cache_delete(json)? };
        let response: CacheResponse = serde_json::from_str(&result)?;
        Ok(response)
    }

    /// 调用其他插件的服务
    pub fn call_service(
        target_plugin_id: &str,
        function_name: &str,
        input: &str,
    ) -> Result<ServiceCallResponse, Error> {
        let request = ServiceCallRequest {
            target_plugin_id: target_plugin_id.to_string(),
            function_name: function_name.to_string(),
            input: input.to_string(),
        };
        let json = serde_json::to_string(&request)?;
        let result = unsafe { call_service(json)? };
        let response: ServiceCallResponse = serde_json::from_str(&result)?;
        Ok(response)
    }
}
