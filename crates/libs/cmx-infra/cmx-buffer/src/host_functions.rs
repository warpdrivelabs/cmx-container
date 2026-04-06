//! WASM 宿主函数 — 缓存操作
//!
//! 为 WASM 插件提供 Redis 缓存操作能力的宿主函数。
//! 所有缓存键自动附加插件ID前缀，实现插件间缓存隔离。

use std::sync::Arc;

use cmx_traits::{HostFuncError, HostFunctionProvider, HostFuncWrapper, WasmLinker};

use crate::cache::CacheManager;

/// 缓存宿主函数提供者
///
/// 封装 CacheManager 的核心 API，向 WASM 运行时注册缓存操作宿主函数。
/// 所有缓存键自动添加 `plugin:{plugin_id}:` 前缀，确保插件间缓存隔离。
pub struct BufferHostFunctions {
    /// 缓存管理器引用
    cache_manager: Arc<CacheManager>,
}

/// 缓存操作请求（JSON 反序列化）
#[derive(serde::Deserialize)]
struct CacheRequest {
    /// 缓存键
    key: String,
    /// 缓存值（写入操作使用）
    value: Option<String>,
    /// 过期时间（秒，可选）
    ttl_seconds: Option<u64>,
}

/// 缓存操作响应（JSON 序列化）
#[derive(serde::Serialize)]
struct CacheResponse {
    /// 是否成功
    success: bool,
    /// 缓存值（读取操作返回）
    value: Option<String>,
    /// 是否存在（exists 操作返回）
    exists: Option<bool>,
    /// 错误信息
    error: Option<String>,
}

impl BufferHostFunctions {
    /// 创建缓存宿主函数提供者
    ///
    /// # 参数
    ///
    /// * `cache_manager` - 缓存管理器共享引用
    pub fn new(cache_manager: Arc<CacheManager>) -> Self {
        Self { cache_manager }
    }

    /// 构建带插件隔离前缀的缓存键
    ///
    /// 格式：`plugin:{plugin_id}:{key}`
    fn build_key(plugin_id: &str, key: &str) -> String {
        format!("plugin:{}:{}", plugin_id, key)
    }

    /// 构建成功响应
    fn ok_response(value: Option<String>, exists: Option<bool>) -> Vec<u8> {
        serde_json::to_vec(&CacheResponse {
            success: true,
            value,
            exists,
            error: None,
        }).unwrap_or_default()
    }

    /// 构建错误响应
    fn err_response(msg: String) -> Vec<u8> {
        serde_json::to_vec(&CacheResponse {
            success: false,
            value: None,
            exists: None,
            error: Some(msg),
        }).unwrap_or_default()
    }

    /// 从输入字节解析请求
    fn parse_request(input: &[u8]) -> Result<CacheRequest, String> {
        serde_json::from_slice::<CacheRequest>(input)
            .map_err(|e| format!("请求数据解析失败: {}", e))
    }
}

impl HostFunctionProvider for BufferHostFunctions {
    /// 返回命名空间 "cmx:buffer"
    fn namespace(&self) -> &str {
        "cmx:buffer"
    }

    /// 注册缓存操作宿主函数
    ///
    /// 注册以下函数：
    /// - `cmx:buffer/cache_get` — 读取缓存
    /// - `cmx:buffer/cache_set` — 写入缓存（可选 TTL）
    /// - `cmx:buffer/cache_delete` — 删除缓存
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        // cmx:buffer/cache_get — 读取缓存
        let cache = self.cache_manager.clone();
        let get_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let plugin_id = caller.caller_data().plugin_id.clone();

            let request = match Self::parse_request(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(e)),
            };

            let full_key = Self::build_key(&plugin_id, &request.key);
            let cache = cache.clone();
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                cache.ops().get(&full_key).await
            });

            match result {
                Ok(Some(value)) => Ok(Self::ok_response(Some(value), Some(true))),
                Ok(None) => Ok(Self::ok_response(None, Some(false))),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:buffer", "cache_get", get_fn)?;

        // cmx:buffer/cache_set — 写入缓存
        let cache = self.cache_manager.clone();
        let set_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let plugin_id = caller.caller_data().plugin_id.clone();

            let request = match Self::parse_request(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(e)),
            };

            let value = request.value.as_deref().unwrap_or("");
            let full_key = Self::build_key(&plugin_id, &request.key);
            let cache = cache.clone();
            let ttl = request.ttl_seconds;
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                if let Some(ttl_secs) = ttl {
                    cache.ops().set_ex(&full_key, value, std::time::Duration::from_secs(ttl_secs)).await
                } else {
                    cache.ops().set(&full_key, value).await
                }
            });

            match result {
                Ok(()) => Ok(Self::ok_response(None, None)),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:buffer", "cache_set", set_fn)?;

        // cmx:buffer/cache_delete — 删除缓存
        let cache = self.cache_manager.clone();
        let del_fn: HostFuncWrapper = Box::new(move |caller, input| {
            let plugin_id = caller.caller_data().plugin_id.clone();

            let request = match Self::parse_request(input) {
                Ok(req) => req,
                Err(e) => return Ok(Self::err_response(e)),
            };

            let full_key = Self::build_key(&plugin_id, &request.key);
            let cache = cache.clone();
            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                cache.ops().del(&full_key).await
            });

            match result {
                Ok(_deleted) => Ok(Self::ok_response(None, None)),
                Err(e) => Ok(Self::err_response(e.to_string())),
            }
        });
        linker.define("cmx:buffer", "cache_delete", del_fn)?;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec![
            "cmx:buffer/cache_get",
            "cmx:buffer/cache_set",
            "cmx:buffer/cache_delete",
        ]
    }
}
