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
        Self {
            cache,
            auth_storage,
        }
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
            && let Ok(entity) = serde_json::from_str::<ApiKeyEntity>(&cached)
        {
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
                .set_with_ttl(
                    &cache_key,
                    &json,
                    Duration::from_secs(API_KEY_CACHE_TTL_SECS),
                )
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cmx_traits::auth::{ApiKeyData, OAuth2ClientData};
    use cmx_traits::error::TraitError;

    /// 辅助：尝试从环境变量获取 Redis URL。
    fn redis_url() -> Option<String> {
        std::env::var("CMX_TEST_REDIS_URL").ok()
    }

    /// 辅助：根据环境变量构造 `CacheManager`。
    async fn make_cache_manager() -> Option<CacheManager> {
        let url = redis_url()?;
        let config = cmx_buffer::RedisConfig::new(&url);
        let client = cmx_buffer::RedisClient::new(config).await.ok()?;
        Some(CacheManager::new(client))
    }

    /// Mock 的 `AuthStorageQuery`，仅 `get_api_key_by_prefix` 返回预设数据。
    struct MockAuthStorage {
        /// 预设的 API Key 数据（None 表示不存在）。
        api_key_data: Option<ApiKeyData>,
    }

    #[async_trait]
    impl AuthStorageQuery for MockAuthStorage {
        async fn upsert_api_key(
            &self,
            _key_prefix: &str,
            _key_hash: &str,
            _user_id: Option<&str>,
            _service_name: Option<&str>,
            _scopes: &[String],
            _description: Option<&str>,
        ) -> Result<(), TraitError> {
            Ok(())
        }

        async fn get_api_key_by_prefix(
            &self,
            _key_prefix: &str,
        ) -> Result<Option<ApiKeyData>, TraitError> {
            Ok(self.api_key_data.clone())
        }

        async fn record_token_event(
            &self,
            _event_type: &str,
            _user_id: &str,
            _jti: &str,
            _detail: &str,
        ) -> Result<(), TraitError> {
            Ok(())
        }

        async fn get_oauth2_client(
            &self,
            _client_id: &str,
        ) -> Result<Option<OAuth2ClientData>, TraitError> {
            Ok(None)
        }
    }

    /// 构造一个启用状态的 `ApiKeyData`。
    fn make_api_key_data(key_prefix: &str, key_hash: &str) -> ApiKeyData {
        ApiKeyData {
            key_prefix: key_prefix.to_string(),
            key_hash: key_hash.to_string(),
            user_id: Some("user-001".to_string()),
            service_name: Some("billing-service".to_string()),
            scopes: vec!["read".to_string(), "write".to_string()],
            description: Some("测试用 API Key".to_string()),
            status: 1,
        }
    }

    // ==================== 纯逻辑测试：sha256_hex ====================

    #[test]
    fn test_sha256_hex_known_input() {
        // 已知向量："hello" 的 SHA256
        let hash = sha256_hex("hello");
        assert_eq!(
            hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "SHA256('hello') 应与已知摘要一致"
        );
    }

    #[test]
    fn test_sha256_hex_empty_input() {
        // 空字符串的 SHA256 已知值
        let hash = sha256_hex("");
        assert_eq!(
            hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA256('') 应与已知摘要一致"
        );
    }

    #[test]
    fn test_sha256_hex_consistency() {
        // 相同输入应产生相同哈希
        let h1 = sha256_hex("cmx_sk_test123");
        let h2 = sha256_hex("cmx_sk_test123");
        assert_eq!(h1, h2, "相同输入应产生相同哈希");

        // 不同输入应产生不同哈希
        let h3 = sha256_hex("cmx_sk_test456");
        assert_ne!(h1, h3, "不同输入应产生不同哈希");
    }

    #[test]
    fn test_sha256_hex_output_is_64_chars_hex() {
        let hash = sha256_hex("any-input");
        assert_eq!(hash.len(), 64, "SHA256 十六进制摘要应为 64 字符");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "摘要应仅包含十六进制字符"
        );
    }

    // ==================== #[ignore] 测试：validate 流程（需 Redis） ====================

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_api_key_validate_short_key_returns_error() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };
        let storage = MockAuthStorage { api_key_data: None };
        let manager = ApiKeyManager::new(cache, std::sync::Arc::new(storage));

        // 短于 8 字符的 key 应直接返回 InvalidApiKey（不查 Redis/DB）
        let result = manager.validate("short").await;
        assert!(result.is_err(), "短 key 应返回错误");
        assert!(
            matches!(result.unwrap_err(), AuthError::InvalidApiKey),
            "期望 InvalidApiKey 错误"
        );
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_api_key_validate_success() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let api_key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456";
        let key_prefix = &api_key[..8];
        let key_hash = sha256_hex(api_key);
        let storage = MockAuthStorage {
            api_key_data: Some(make_api_key_data(key_prefix, &key_hash)),
        };
        let manager = ApiKeyManager::new(cache.clone(), std::sync::Arc::new(storage));

        // 清理可能残留的缓存
        let cache_key = format!("auth:api_key:{}", key_prefix);
        let _ = cache.ops().del(&cache_key).await;

        // 1. 首次校验：应命中 DB 并返回实体
        let entity = manager
            .validate(api_key)
            .await
            .expect("有效 API Key 校验应成功");
        assert_eq!(entity.key_prefix, key_prefix);
        assert_eq!(entity.key_hash, key_hash);
        assert_eq!(entity.status, 1);
        assert_eq!(entity.scopes, vec!["read", "write"]);

        // 2. 第二次校验：应命中 Redis 缓存（跳过 DB）
        let entity_cached = manager
            .validate(api_key)
            .await
            .expect("缓存命中时校验也应成功");
        assert_eq!(entity_cached.key_hash, key_hash);

        // 清理
        let _ = cache.ops().del(&cache_key).await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_api_key_validate_wrong_key_returns_error() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        // DB 中存储的是 valid_key 的哈希
        let valid_key = "cmx_sk_ValidKey1234567890";
        let key_prefix = &valid_key[..8];
        let stored_hash = sha256_hex(valid_key);
        let storage = MockAuthStorage {
            api_key_data: Some(make_api_key_data(key_prefix, &stored_hash)),
        };
        let manager = ApiKeyManager::new(cache.clone(), std::sync::Arc::new(storage));

        // 用相同 prefix 但不同明文（撞 prefix）校验应失败
        let wrong_key = "cmx_sk_WrongKey4567890XYZ";
        let cache_key = format!("auth:api_key:{}", key_prefix);
        let _ = cache.ops().del(&cache_key).await;

        let result = manager.validate(wrong_key).await;
        assert!(result.is_err(), "哈希不匹配应返回错误");
        assert!(
            matches!(result.unwrap_err(), AuthError::InvalidApiKey),
            "期望 InvalidApiKey 错误"
        );

        // 清理
        let _ = cache.ops().del(&cache_key).await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_api_key_validate_disabled_key_returns_error() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let api_key = "cmx_sk_DisabledKy0123456789";
        let key_prefix = &api_key[..8];
        let key_hash = sha256_hex(api_key);
        let mut data = make_api_key_data(key_prefix, &key_hash);
        // 禁用状态
        data.status = 0;
        let storage = MockAuthStorage {
            api_key_data: Some(data),
        };
        let manager = ApiKeyManager::new(cache.clone(), std::sync::Arc::new(storage));

        let cache_key = format!("auth:api_key:{}", key_prefix);
        let _ = cache.ops().del(&cache_key).await;

        let result = manager.validate(api_key).await;
        assert!(result.is_err(), "禁用状态的 Key 应返回错误");
        assert!(
            matches!(result.unwrap_err(), AuthError::InvalidApiKey),
            "期望 InvalidApiKey 错误"
        );

        // 清理
        let _ = cache.ops().del(&cache_key).await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_api_key_validate_nonexistent_prefix_returns_error() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        // DB 中不存在该 prefix
        let storage = MockAuthStorage { api_key_data: None };
        let manager = ApiKeyManager::new(cache.clone(), std::sync::Arc::new(storage));

        let api_key = "cmx_sk_NoSuchKey0123456789";
        let key_prefix = &api_key[..8];
        let cache_key = format!("auth:api_key:{}", key_prefix);
        let _ = cache.ops().del(&cache_key).await;

        let result = manager.validate(api_key).await;
        assert!(result.is_err(), "不存在的 prefix 应返回错误");
        assert!(
            matches!(result.unwrap_err(), AuthError::InvalidApiKey),
            "期望 InvalidApiKey 错误"
        );

        // 清理
        let _ = cache.ops().del(&cache_key).await;
    }
}
