//! WASM 插件宿主函数调用封装
//!
//! 为 WASM 插件提供调用宿主函数的便捷封装。
//! 使用 extism-pdk 的 `#[host_fn]` 宏声明宿主函数签名。
//! 数据类函数使用 MsgPack (Vec<u8>) 编码，日志类函数使用 String 传递。

use extism_pdk::*;
use cmx_core::{
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    ServiceCallRequest, ServiceCallResponse,
};

// 声明日志宿主函数（纯文本，保持 String 类型）
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

// 声明数据库宿主函数（MsgPack 编码）
#[host_fn("cmx:database")]
extern "ExtismHost" {
    /// 执行数据库查询
    fn db_query(request: Vec<u8>) -> Vec<u8>;
    /// 执行数据库操作
    fn db_execute(request: Vec<u8>) -> Vec<u8>;
}

// 声明缓存宿主函数（MsgPack 编码）
#[host_fn("cmx:buffer")]
extern "ExtismHost" {
    /// 获取缓存
    fn cache_get(request: Vec<u8>) -> Vec<u8>;
    /// 设置缓存
    fn cache_set(request: Vec<u8>) -> Vec<u8>;
    /// 删除缓存
    fn cache_delete(request: Vec<u8>) -> Vec<u8>;
}

// 声明插件间调用宿主函数（MsgPack 编码）
#[host_fn("cmx:plugin")]
extern "ExtismHost" {
    /// 调用其他插件的服务
    fn call_service(request: Vec<u8>) -> Vec<u8>;
}

/// 宿主函数调用器
///
/// 提供便捷的方法来调用宿主函数。
/// 日志函数直接传递字符串，数据类函数使用 MsgPack 编码结构体。
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
    pub fn db_query(request: DbRequest) -> Result<DbResponse, Error> {
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { db_query(bytes)? };
        let response: DbResponse = rmp_serde::from_slice(&result)?;
        Ok(response)
    }

    /// 执行数据库操作
    pub fn db_execute(request: DbRequest) -> Result<DbResponse, Error> {
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { db_execute(bytes)? };
        let response: DbResponse = rmp_serde::from_slice(&result)?;
        Ok(response)
    }

    /// 获取缓存
    pub fn cache_get(key: &str) -> Result<CacheResponse, Error> {
        let request = CacheGetRequest {
            key: key.to_string(),
        };
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { cache_get(bytes)? };
        let response: CacheResponse = rmp_serde::from_slice(&result)?;
        Ok(response)
    }

    /// 设置缓存
    ///
    /// # 参数
    /// - `key`: 缓存键
    /// - `value`: 缓存值（任意 JSON 可序列化的值）
    /// - `ttl_seconds`: 可选的过期时间（秒）
    pub fn cache_set(key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, Error> {
        let request = CacheSetRequest {
            key: key.to_string(),
            value,
            ttl_seconds,
        };
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { cache_set(bytes)? };
        let response: CacheResponse = rmp_serde::from_slice(&result)?;
        Ok(response)
    }

    /// 删除缓存
    pub fn cache_delete(key: &str) -> Result<CacheResponse, Error> {
        let request = CacheGetRequest {
            key: key.to_string(),
        };
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { cache_delete(bytes)? };
        let response: CacheResponse = rmp_serde::from_slice(&result)?;
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
        let bytes = rmp_serde::to_vec(&request)?;
        let result = unsafe { call_service(bytes)? };
        let response: ServiceCallResponse = rmp_serde::from_slice(&result)?;
        Ok(response)
    }
}
