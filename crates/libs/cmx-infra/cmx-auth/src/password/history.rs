//! 密码历史校验
//!
//! 使用 Redis Hash 存储最近 5 条密码哈希，禁止重复使用。

use cmx_buffer::CacheManager;

use crate::error::Result;
use crate::password::Argon2Hasher;

const MAX_HISTORY: usize = 5;

/// 密码历史校验器
pub struct PasswordHistory {
    cache: CacheManager,
    hasher: Argon2Hasher,
}

impl PasswordHistory {
    pub fn new(cache: CacheManager, hasher: Argon2Hasher) -> Self {
        Self { cache, hasher }
    }

    /// 检查密码是否在历史中重复
    pub async fn is_reused(&self, user_id: &str, plain: &str) -> Result<bool> {
        let key = format!("auth:{{{}}}:pwd_history", user_id);
        let histories = self.cache.hash().hvals(&key).await?;

        for hash in &histories {
            if self.hasher.verify(plain, hash).unwrap_or(false) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 记录密码哈希到历史
    pub async fn record(&self, user_id: &str, hash: &str) -> Result<()> {
        let key = format!("auth:{{{}}}:pwd_history", user_id);
        let hash_ops = self.cache.hash();

        // 使用时间戳作为 field，确保唯一性和时间有序性
        let now = chrono::Utc::now().timestamp_millis();
        let field = format!("{}", now);
        hash_ops.hset(&key, &field, hash).await?;

        // 超出 MAX_HISTORY 时删除最旧的记录
        let count = hash_ops.hlen(&key).await?;
        if count > MAX_HISTORY as u64 {
            // 获取所有 field，排序后删除最旧的
            let fields = hash_ops.hkeys(&key).await?;
            let mut sorted_fields: Vec<String> = fields;
            sorted_fields.sort();

            // 删除最旧的 (count - MAX_HISTORY) 条
            let to_delete = count as usize - MAX_HISTORY;
            if to_delete > 0 && to_delete <= sorted_fields.len() {
                let delete_fields: Vec<&str> =
                    sorted_fields[..to_delete].iter().map(|s| s.as_str()).collect();
                hash_ops.hdel(&key, &delete_fields).await?;
            }
        }

        Ok(())
    }
}
