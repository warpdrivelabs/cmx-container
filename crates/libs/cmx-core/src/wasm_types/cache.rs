//! 缓存操作相关类型
//!
//! 定义宿主与 WASM 之间缓存操作的请求和响应结构体。

use serde::{Deserialize, Serialize};

/// 缓存读取请求
///
/// 用于 WASM 插件向宿主发起缓存读取操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheGetRequest {
    /// 缓存键
    pub key: String,
}

/// 缓存写入请求
///
/// 用于 WASM 插件向宿主发起缓存写入操作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSetRequest {
    /// 缓存键
    pub key: String,
    /// 缓存值（使用 serde_json::Value 支持任意 JSON 类型）
    pub value: serde_json::Value,
    /// 过期时间(秒)
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// 缓存操作响应
///
/// 宿主返回给 WASM 插件的缓存操作结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponse {
    /// 是否成功
    pub success: bool,
    /// 缓存值(读取操作返回，使用 serde_json::Value 支持任意 JSON 类型)
    pub value: Option<serde_json::Value>,
    /// 是否存在
    pub exists: Option<bool>,
    /// 错误信息
    pub error: Option<String>,
}
