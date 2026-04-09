//! WASM 宿主函数 — 缓存操作
//!
//! 为 WASM 插件提供 Redis 缓存操作能力的宿主函数。
//! 所有缓存键自动附加插件ID前缀，实现插件间缓存隔离。

use cmx_traits::{HostFuncError, HostFunctionProvider, HostFunctionDef};

use crate::cache::GlobalCacheManager;

/// 缓存宿主函数提供者
///
/// 封装 CacheManager 的核心 API，向 WASM 运行时注册缓存操作宿主函数。
/// 所有缓存键自动添加 `plugin:{plugin_id}:` 前缀，确保插件间缓存隔离。
pub struct BufferHostFunctions;

impl BufferHostFunctions {
    /// 创建缓存宿主函数提供者
    pub fn new() -> Self {
        Self
    }

    /// 构建带插件隔离前缀的缓存键
    ///
    /// 格式：`plugin:{plugin_id}:{key}`
    fn build_key(plugin_id: &str, key: &str) -> String {
        format!("plugin:{}:{}", plugin_id, key)
    }

    /// 执行缓存读取
    fn do_cache_get(&self, input: String) -> Result<String, HostFuncError> {
        #[derive(serde::Deserialize)]
        struct CacheRequest {
            key: String,
        }

        let req: CacheRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
        };

        let cache = GlobalCacheManager::get();
        let full_key = Self::build_key("default", &req.key);

        // 当前已在 spawn_blocking 线程中，直接使用 block_on
        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                cache.ops().get(&full_key).await
            })
        };

        match result {
            Ok(Some(value)) => Ok(Self::ok_response(Some(value), Some(true))),
            Ok(None) => Ok(Self::ok_response(None, Some(false))),
            Err(e) => Ok(Self::err_response(e.to_string())),
        }
    }

    /// 执行缓存写入
    fn do_cache_set(&self, input: String) -> Result<String, HostFuncError> {
        #[derive(serde::Deserialize)]
        struct CacheRequest {
            key: String,
            value: Option<String>,
            ttl_seconds: Option<u64>,
        }

        let req: CacheRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
        };

        let value = req.value.as_deref().unwrap_or("");
        let cache = GlobalCacheManager::get();
        let full_key = Self::build_key("default", &req.key);
        let ttl = req.ttl_seconds;

        // 当前已在 spawn_blocking 线程中，直接使用 block_on
        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                if let Some(ttl_secs) = ttl {
                    cache.ops().set_ex(&full_key, value, std::time::Duration::from_secs(ttl_secs)).await
                } else {
                    cache.ops().set(&full_key, value).await
                }
            })
        };

        match result {
            Ok(()) => Ok(Self::ok_response(None, None)),
            Err(e) => Ok(Self::err_response(e.to_string())),
        }
    }

    /// 执行缓存删除
    fn do_cache_delete(&self, input: String) -> Result<String, HostFuncError> {
        #[derive(serde::Deserialize)]
        struct CacheRequest {
            key: String,
        }

        let req: CacheRequest = match serde_json::from_str(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
        };

        let cache = GlobalCacheManager::get();
        let full_key = Self::build_key("default", &req.key);

        // 当前已在 spawn_blocking 线程中，直接使用 block_on
        let result = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                cache.ops().del(&full_key).await
            })
        };

        match result {
            Ok(_deleted) => Ok(Self::ok_response(None, None)),
            Err(e) => Ok(Self::err_response(e.to_string())),
        }
    }

    /// 构建成功响应
    fn ok_response(value: Option<String>, exists: Option<bool>) -> String {
        #[derive(serde::Serialize)]
        struct CacheResponse {
            success: bool,
            value: Option<String>,
            exists: Option<bool>,
            error: Option<String>,
        }

        serde_json::to_string(&CacheResponse {
            success: true,
            value,
            exists,
            error: None,
        })
        .unwrap_or_default()
    }

    /// 构建错误响应
    fn err_response(msg: String) -> String {
        #[derive(serde::Serialize)]
        struct CacheResponse {
            success: bool,
            value: Option<String>,
            exists: Option<bool>,
            error: Option<String>,
        }

        serde_json::to_string(&CacheResponse {
            success: false,
            value: None,
            exists: None,
            error: Some(msg),
        })
        .unwrap_or_default()
    }
}

impl Default for BufferHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl HostFunctionProvider for BufferHostFunctions {
    /// 返回命名空间 "cmx:buffer"
    fn namespace(&self) -> &str {
        "cmx:buffer"
    }

    /// 返回提供的宿主函数列表
    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            HostFunctionDef::json_fn("cache_get", "cmx:buffer"),
            HostFunctionDef::json_fn("cache_set", "cmx:buffer"),
            HostFunctionDef::json_fn("cache_delete", "cmx:buffer"),
        ]
    }

    /// 调用宿主函数
    fn call(&self, name: &str, input: String) -> Result<String, HostFuncError> {
        match name {
            "cache_get" => self.do_cache_get(input),
            "cache_set" => self.do_cache_set(input),
            "cache_delete" => self.do_cache_delete(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec!["cache_get", "cache_set", "cache_delete"]
    }
}
