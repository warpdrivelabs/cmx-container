//! 密码历史校验。
//!
//! 使用 Redis Hash 存储最近 5 条密码哈希，禁止重复使用。

use cmx_buffer::CacheManager;

use crate::error::Result;
use crate::password::Argon2Hasher;

/// 最大保留的密码历史条数。
const MAX_HISTORY: usize = 5;

/// 密码历史校验器。
///
/// 使用 Redis Hash 存储用户最近 5 条密码哈希，禁止修改密码时重复使用历史密码。
pub struct PasswordHistory {
    /// Redis 缓存管理器。
    cache: CacheManager,

    /// Argon2 哈希器（用于校验明文密码是否匹配历史哈希）。
    hasher: Argon2Hasher,
}

impl PasswordHistory {
    /// 创建新的密码历史校验器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `hasher` - Argon2 哈希器实例。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `PasswordHistory` 实例。
    pub fn new(cache: CacheManager, hasher: Argon2Hasher) -> Self {
        Self { cache, hasher }
    }

    /// 检查密码是否在历史中重复。
    ///
    /// 遍历用户最近 `MAX_HISTORY` 条密码哈希，校验明文密码是否匹配任一历史记录。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `plain` - 待检查的明文密码。
    ///
    /// # Returns
    ///
    /// 密码在历史中存在时返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当 Redis 读取失败时返回 `AuthInfraError`。
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

    /// 记录密码哈希到历史。
    ///
    /// 使用时间戳作为 Hash field 保证唯一性和时间有序性，
    /// 超过 `MAX_HISTORY` 条时自动删除最旧的记录。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `hash` - 待记录的密码哈希字符串。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入或删除失败时返回 `AuthInfraError`。
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
