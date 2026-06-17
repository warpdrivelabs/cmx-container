//! API Key 管理器

use cmx_buffer::CacheManager;
use cmx_traits::{AuthError, AuthStorageQuery};

use super::entity::ApiKeyEntity;

/// API Key 管理器
pub struct ApiKeyManager {
    cache: CacheManager,
    auth_storage: std::sync::Arc<dyn AuthStorageQuery>,
}

impl ApiKeyManager {
    /// 创建新的 API Key 管理器
    pub fn new(cache: CacheManager, auth_storage: std::sync::Arc<dyn AuthStorageQuery>) -> Self {
        Self { cache, auth_storage }
    }

    /// 验证 API Key
    ///
    /// 通过 key_prefix 查找对应的哈希，然后比对
    pub async fn validate(&self, api_key: &str) -> Result<ApiKeyEntity, AuthError> {
        // 1. 提取 key_prefix（格式：cmx_xxxxxxxx...）
        let key_prefix = if api_key.len() >= 8 {
            &api_key[..8]
        } else {
            return Err(AuthError::InvalidApiKey);
        };

        // 2. 先查本地缓存
        let cache_key = format!("auth:api_key:{}", key_prefix);
        if self
            .cache
            .ops()
            .get(&cache_key)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .is_some()
        {
            // 缓存命中，需要反序列化（简化：直接查数据库）
        }

        // 3. 通过 AuthStorageQuery 查询 API Key
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
        // 对比 SHA256(api_key) == stored_hash
        let input_hash = sha256_hex(api_key);
        if input_hash != api_key_data.key_hash {
            return Err(AuthError::InvalidApiKey);
        }

        Ok(ApiKeyEntity {
            key_prefix: api_key_data.key_prefix,
            key_hash: api_key_data.key_hash,
            user_id: api_key_data.user_id,
            service_name: api_key_data.service_name,
            scopes: api_key_data.scopes,
            description: api_key_data.description,
            status: api_key_data.status,
        })
    }
}

/// 计算 SHA256 十六进制摘要
fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}
