//! WASM 宿主函数 — 缓存操作
//!
//! 为 WASM 插件提供 Redis 缓存操作能力的宿主函数。
//! 所有缓存键自动附加插件ID前缀，实现插件间缓存隔离。

use cmx_traits::{ExtismFunctionProvider, HostFuncError};
use extism::{host_fn, Manifest, UserData, ValType};

use crate::cache::GlobalCacheManager;

const PTR: ValType = ValType::I64;

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

    /// 构建成功响应
    fn ok_response(value: Option<String>, exists: Option<bool>) -> String {
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

impl ExtismFunctionProvider for BufferHostFunctions {
    /// 返回命名空间 "cmx:buffer"
    fn namespace(&self) -> &str {
        "cmx:buffer"
    }

    /// 注册缓存操作宿主函数
    ///
    /// 注册以下函数：
    /// - `cache_get` — 读取缓存
    /// - `cache_set` — 写入缓存（可选 TTL）
    /// - `cache_delete` — 删除缓存
    fn register_functions(&self, builder: &mut extism::PluginBuilder) -> Result<(), HostFuncError> {
        // cache_get — 读取缓存
        host_fn!(cache_get(_user_data: (); request: String) -> String {
            let req: CacheRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => return Ok(BufferHostFunctions::err_response(format!("解析请求失败: {}", e))),
            };

            let cache = GlobalCacheManager::get();
            let full_key = BufferHostFunctions::build_key("default", &req.key);

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                cache.ops().get(&full_key).await
            });

            match result {
                Ok(Some(value)) => Ok(BufferHostFunctions::ok_response(Some(value), Some(true))),
                Ok(None) => Ok(BufferHostFunctions::ok_response(None, Some(false))),
                Err(e) => Ok(BufferHostFunctions::err_response(e.to_string())),
            }
        });

        // cache_set — 写入缓存
        host_fn!(cache_set(_user_data: (); request: String) -> String {
            let req: CacheRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => return Ok(BufferHostFunctions::err_response(format!("解析请求失败: {}", e))),
            };

            let value = req.value.as_deref().unwrap_or("");
            let cache = GlobalCacheManager::get();
            let full_key = BufferHostFunctions::build_key("default", &req.key);
            let ttl = req.ttl_seconds;

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                if let Some(ttl_secs) = ttl {
                    cache.ops().set_ex(&full_key, value, std::time::Duration::from_secs(ttl_secs)).await
                } else {
                    cache.ops().set(&full_key, value).await
                }
            });

            match result {
                Ok(()) => Ok(BufferHostFunctions::ok_response(None, None)),
                Err(e) => Ok(BufferHostFunctions::err_response(e.to_string())),
            }
        });

        // cache_delete — 删除缓存
        host_fn!(cache_delete(_user_data: (); request: String) -> String {
            let req: CacheRequest = match serde_json::from_str(&request) {
                Ok(r) => r,
                Err(e) => return Ok(BufferHostFunctions::err_response(format!("解析请求失败: {}", e))),
            };

            let cache = GlobalCacheManager::get();
            let full_key = BufferHostFunctions::build_key("default", &req.key);

            let rt = tokio::runtime::Handle::current();
            let result = rt.block_on(async {
                cache.ops().del(&full_key).await
            });

            match result {
                Ok(_deleted) => Ok(BufferHostFunctions::ok_response(None, None)),
                Err(e) => Ok(BufferHostFunctions::err_response(e.to_string())),
            }
        });

        // 使用 std::mem::replace 替换 builder
        let temp_manifest = Manifest::new([extism::Wasm::data(vec![])]);
        let temp_builder = extism::PluginBuilder::new(temp_manifest);
        let old_builder = std::mem::replace(builder, temp_builder);

        let new_builder = old_builder
            .with_function("cache_get", [PTR], [PTR], UserData::new(()), cache_get)
            .with_function("cache_set", [PTR], [PTR], UserData::new(()), cache_set)
            .with_function("cache_delete", [PTR], [PTR], UserData::new(()), cache_delete);

        *builder = new_builder;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec!["cache_get", "cache_set", "cache_delete"]
    }
}
