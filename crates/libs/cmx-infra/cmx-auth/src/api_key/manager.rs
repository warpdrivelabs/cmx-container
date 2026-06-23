//! API Key 管理器。

use std::time::Duration;

use cmx_buffer::CacheManager;
use cmx_traits::auth::{AuthError, AuthStorageQuery};
use tracing::debug;

use super::entity::ApiKeyEntity;

/// API Key 本地缓存 TTL（秒）。
const API_KEY_CACHE_TTL_SECS: u64 = 60;

/// API Key 管理器。
///
/// 通过 `key_prefix` 查找存储的 SHA256 哈希并比对明文 Key，
/// 提供 M2M 场景下的无状态认证。
///
/// # 缓存策略
///
/// 为避免高频 M2M 调用打垮数据库，使用 Redis 缓存 `key_prefix → ApiKeyEntity`，
/// TTL 60 秒。缓存命中时直接返回，跳过 DB 查询。
/// API Key 被撤销/禁用/修改时通过 Pub/Sub 广播 `api_key:{key_prefix}` 触发缓存失效。
pub struct ApiKeyManager {
    /// Redis 缓存管理器（用于缓存加速 + 本地缓存）。
    cache: CacheManager,

    /// API Key 存储查询 trait 对象（由 `AuthServiceImpl` 提供）。
    auth_storage: std::sync::Arc<dyn AuthStorageQuery>,
}

impl ApiKeyManager {
    /// 创建新的 API Key 管理器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `auth_storage` - API Key 存储查询 trait 对象。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `ApiKeyManager` 实例。
    pub fn new(cache: CacheManager, auth_storage: std::sync::Arc<dyn AuthStorageQuery>) -> Self {
        Self { cache, auth_storage }
    }

    /// 验证 API Key 并返回关联实体。
    ///
    /// 通过 `key_prefix` 查找对应的哈希，然后比对明文 Key。
    /// 优先查 Redis 缓存，命中则跳过 DB 查询。
    ///
    /// # Arguments
    ///
    /// * `api_key` - 待验证的 API Key 明文字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回 `ApiKeyEntity`，包含 Key 元数据与关联用户/服务信息。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidApiKey` - Key 格式错误、状态禁用或哈希不匹配。
    /// * `AuthError::Internal` - 数据库或 Redis 查询失败。
    pub async fn validate(&self, api_key: &str) -> Result<ApiKeyEntity, AuthError> {
        // 1. 提取 key_prefix（格式：cmx_xxxxxxxx...）
        let key_prefix = if api_key.len() >= 8 {
            &api_key[..8]
        } else {
            return Err(AuthError::InvalidApiKey);
        };

        // 2. 查 Redis 缓存
        let cache_key = format!("auth:api_key:{}", key_prefix);
        if let Ok(Some(cached)) = self.cache.ops().get(&cache_key).await
            && let Ok(entity) = serde_json::from_str::<ApiKeyEntity>(&cached) {
                // 缓存命中：仍需校验明文 key 的 SHA256（防止缓存被篡改后绕过校验）
                let input_hash = sha256_hex(api_key);
                if input_hash == entity.key_hash {
                    debug!(key_prefix = %key_prefix, "API Key 缓存命中，跳过 DB 查询");
                    return Ok(entity);
                }
                // hash 不匹配：可能是无效 key 撞了 prefix，返回错误
                return Err(AuthError::InvalidApiKey);
            }

        // 3. 缓存未命中，查 DB
        let api_key_data = self
            .auth_storage
            .get_api_key_by_prefix(key_prefix)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidApiKey)?;

        // 4. 检查状态
        if api_key_data.status == 0 {
            return Err(AuthError::InvalidApiKey);
        }

        // 5. 使用 SHA256 验证 key
        let input_hash = sha256_hex(api_key);
        if input_hash != api_key_data.key_hash {
            return Err(AuthError::InvalidApiKey);
        }

        let entity = ApiKeyEntity {
            key_prefix: api_key_data.key_prefix,
            key_hash: api_key_data.key_hash,
            user_id: api_key_data.user_id,
            service_name: api_key_data.service_name,
            scopes: api_key_data.scopes,
            description: api_key_data.description,
            status: api_key_data.status,
        };

        // 6. 写入 Redis 缓存（TTL 60 秒）
        if let Ok(json) = serde_json::to_string(&entity) {
            let _ = self
                .cache
                .ttl()
                .set_with_ttl(&cache_key, &json, Duration::from_secs(API_KEY_CACHE_TTL_SECS))
                .await;
        }

        Ok(entity)
    }

    /// 失效指定 `key_prefix` 的缓存（供 Pub/Sub 回调使用）。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - 待失效缓存的 API Key 前缀。
    pub async fn invalidate_cache(&self, key_prefix: &str) {
        let cache_key = format!("auth:api_key:{}", key_prefix);
        let _ = self.cache.ops().del(&cache_key).await;
        debug!(key_prefix = %key_prefix, "API Key 缓存已失效");
    }
}

/// 计算 SHA256 十六进制摘要。
fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}
