//! WASM 插件宿主函数调用封装
//!
//! 为 WASM 插件提供调用宿主函数的便捷封装。
//! 使用 extism-pdk 的 `#[host_fn]` 宏声明宿主函数签名。
//! 数据类函数使用 MsgPack (Vec<u8>) 编码，日志类函数使用 String 传递。

use extism_pdk::*;
use cmx_core::{
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, CallServiceRequest, CallServiceResponse,
};
use crate::error::PluginError;

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


    /// 调用指定插件的指定函数
    ///
    /// # 参数
    /// - `request`: PluginFunRequest 的 MsgPack 编码
    ///
    /// # 返回值
    /// CallServiceResponse 的 MsgPack 编码
    fn call_plugin(request: Vec<u8>) -> Vec<u8>;

    /// 调用指定服务编排
    ///
    /// # 参数
    /// - `request`: CallServiceRequest 的 MsgPack 编码
    ///
    /// # 返回值
    /// CallServiceResponse 的 MsgPack 编码
    fn call_service_by_key(request: Vec<u8>) -> Vec<u8>;
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



    /// 调用指定插件的指定函数
    ///
    /// 类似于 API `/api/service/call`，在 WASM 插件上下文中调用另一个插件的函数。
    ///
    /// # 参数说明
    /// - `request`: PluginFunRequest 请求结构体，包含目标插件ID、函数名和输入数据
    ///
    /// # 返回值说明
    /// - `Ok(serde_json::Value)`: 函数执行结果
    /// - `Err(Error)`: 调用失败，包含错误信息
    pub fn call_plugin(request: PluginFunRequest) -> Result<serde_json::Value, PluginError> {
        let bytes = rmp_serde::to_vec(&request).map_err(|e| PluginError::SerializationError(e.to_string()))?;
        let result = unsafe { call_plugin(bytes) }.map_err(|e| PluginError::HostCallFailed(e.to_string()))?;
        let response: CallServiceResponse = rmp_serde::from_slice(&result).map_err(|e| PluginError::DeserializationError(e.to_string()))?;
        if !response.success {
            return Err(PluginError::HostCallFailed(response.error.unwrap_or_default()));
        }
        Ok(response.output.unwrap_or(serde_json::Value::Null))
    }

    /// 调用指定服务编排
    ///
    /// 类似于 API `/api/service/execute`，在 WASM 插件上下文中执行一个完整的服务编排。
    ///
    /// # 参数说明
    /// - `request`: CallServiceRequest 请求结构体，包含服务标识、输入数据和执行选项
    ///
    /// # 返回值说明
    /// - `Ok(serde_json::Value)`: 服务执行的最终输出
    /// - `Err(Error)`: 执行失败，包含错误信息
    pub fn call_service_by_key(request: CallServiceRequest) -> Result<serde_json::Value, PluginError> {
        let bytes = rmp_serde::to_vec(&request).map_err(|e| PluginError::SerializationError(e.to_string()))?;
        let result = unsafe { call_service_by_key(bytes) }.map_err(|e| PluginError::HostCallFailed(e.to_string()))?;
        let response: CallServiceResponse = rmp_serde::from_slice(&result).map_err(|e| PluginError::DeserializationError(e.to_string()))?;
        if !response.success {
            return Err(PluginError::HostCallFailed(response.error.unwrap_or_default()));
        }
        Ok(response.output.unwrap_or(serde_json::Value::Null))
    }
}
