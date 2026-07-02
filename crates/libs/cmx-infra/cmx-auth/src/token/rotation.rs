//! Refresh Token Rotation Lua 原子操作。
//!
//! 使用 Redis Lua 脚本实现"检查旧 token 是否存在 + 原子删除"，
//! 消除 exists → revoke → store 三步操作的竞态条件。
//! 新 token 的创建由 `issue_token_pair` 的 `store_refresh_token` 完成。

use cmx_buffer::CacheManager;

use crate::error::Result;

/// Refresh Token Rotation Lua 脚本。
///
/// 原子操作：检查旧 token 是否存在 → 删除旧 token → 从 index 移除旧 jti。
///
/// # KEYS
///
/// - `KEYS[1]` = `auth:{user_id}:refresh:{old_jti}`
/// - `KEYS[2]` = `auth:{user_id}:refresh_index`
///
/// # ARGV
///
/// - `ARGV[1]` = `old_jti`
///
/// # 返回值
///
/// - `1`: 成功（旧 token 存在并已删除）
/// - `0`: 失败（旧 token 不存在，可能重放）
pub const ROTATE_LUA_SCRIPT: &str = r#"
-- 检查旧 token 是否存在
if redis.call('EXISTS', KEYS[1]) == 0 then
    return 0
end

-- 删除旧 token
redis.call('DEL', KEYS[1])

-- 从 index 移除旧 jti
redis.call('SREM', KEYS[2], ARGV[1])

return 1
"#;

/// Refresh Token Rotation 管理器。
///
/// 通过 Lua 脚本原子执行"检查旧 jti → 删除旧 token → 从 index 移除旧 jti"，
/// 防止并发场景下 Refresh Token 重放攻击。
pub struct RefreshRotation {
    /// Redis 缓存管理器。
    cache: CacheManager,
}

impl RefreshRotation {
    /// 创建新的 `RefreshRotation` 管理器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `RefreshRotation` 实例。
    pub fn new(cache: CacheManager) -> Self {
        Self { cache }
    }

    /// 原子检查并删除旧 Refresh Token。
    ///
    /// 使用 Lua 脚本原子执行：检查旧 jti 是否存在 → 删除旧 key → 从 index 移除旧 jti。
    /// 新 token 的创建由 `store_refresh_token` 完成。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `old_jti` - 待轮换的旧 Refresh Token JTI。
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - 旧 token 存在并已删除。
    /// * `Ok(false)` - 旧 token 不存在（可能重放攻击）。
    ///
    /// # Errors
    ///
    /// 当 Lua 脚本执行失败时返回 `AuthInfraError`。
    pub async fn rotate_refresh_token(&self, user_id: &str, old_jti: &str) -> Result<bool> {
        let refresh_key = format!("auth:{{{}}}:refresh:{}", user_id, old_jti);
        let index_key = format!("auth:{{{}}}:refresh_index", user_id);

        let keys = &[refresh_key.as_str(), index_key.as_str()];
        let args = &[old_jti];

        let result = self
            .cache
            .script()
            .eval_with_fallback(ROTATE_LUA_SCRIPT, keys, args)
            .await?;

        Ok(matches!(result, redis::Value::Int(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotate_lua_script_contains_required_operations() {
        // 验证 Lua 脚本包含关键操作（纯字符串校验，不依赖 Redis）
        assert!(
            ROTATE_LUA_SCRIPT.contains("EXISTS"),
            "脚本应包含 EXISTS 检查旧 token 是否存在"
        );
        assert!(
            ROTATE_LUA_SCRIPT.contains("DEL"),
            "脚本应包含 DEL 删除旧 token"
        );
        assert!(
            ROTATE_LUA_SCRIPT.contains("SREM"),
            "脚本应包含 SREM 从 index 集合移除旧 jti"
        );
        // 检查返回值约定：1=成功，0=失败（不存在）
        assert!(ROTATE_LUA_SCRIPT.contains("return 1"), "脚本成功时应返回 1");
        assert!(
            ROTATE_LUA_SCRIPT.contains("return 0"),
            "脚本失败（token 不存在）时应返回 0"
        );
    }

    #[test]
    fn test_rotate_lua_script_uses_keys_and_argv() {
        // 验证脚本使用 KEYS[1]/KEYS[2]/ARGV[1]，与 rotate_refresh_token 的传参一致
        assert!(
            ROTATE_LUA_SCRIPT.contains("KEYS[1]"),
            "脚本应使用 KEYS[1]（refresh_key）"
        );
        assert!(
            ROTATE_LUA_SCRIPT.contains("KEYS[2]"),
            "脚本应使用 KEYS[2]（index_key）"
        );
        assert!(
            ROTATE_LUA_SCRIPT.contains("ARGV[1]"),
            "脚本应使用 ARGV[1]（old_jti）"
        );
    }

    /// 辅助：尝试从环境变量获取 Redis URL，未配置时返回 None。
    fn redis_url() -> Option<String> {
        std::env::var("CMX_TEST_REDIS_URL").ok()
    }

    /// 辅助：根据环境变量构造 `CacheManager`，未配置 Redis 时返回 None。
    async fn make_cache_manager() -> Option<CacheManager> {
        let url = redis_url()?;
        let config = cmx_buffer::RedisConfig::new(&url);
        let client = cmx_buffer::RedisClient::new(config).await.ok()?;
        Some(CacheManager::new(client))
    }

    /// 辅助：存储一个 refresh token（模拟 TokenManager::store_refresh_token 的最小逻辑）。
    async fn store_refresh_token(
        cache: &CacheManager,
        user_id: &str,
        jti: &str,
        device: &str,
    ) -> Result<()> {
        use std::time::Duration;
        let key = format!("auth:{{{}}}:refresh:{}", user_id, jti);
        cache
            .ttl()
            .set_with_ttl(&key, device, Duration::from_secs(3600))
            .await?;
        let index_key = format!("auth:{{{}}}:refresh_index", user_id);
        cache.set().sadd_one(&index_key, jti).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_refresh_rotation_normal() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return, // 无 Redis 时跳过（被 #[ignore] 标记，不会自动运行）
        };

        let user_id = "rotation-test-user-normal";
        let old_jti = "old-jti-normal";
        let new_jti = "new-jti-normal";

        // 清理可能残留的数据
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh:{}", user_id, old_jti))
            .await;
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh:{}", user_id, new_jti))
            .await;
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh_index", user_id))
            .await;

        // 1. 存储旧 refresh token
        store_refresh_token(&cache, user_id, old_jti, "web")
            .await
            .expect("存储旧 refresh token 失败");

        // 2. 执行轮换
        let rotation = RefreshRotation::new(cache.clone());
        let result = rotation
            .rotate_refresh_token(user_id, old_jti)
            .await
            .expect("轮换调用失败");

        // 3. 旧 token 应被成功轮换（返回 true）
        assert!(result, "存在的旧 token 轮换应返回 true");

        // 4. 旧 token key 应已被删除（不存在）
        let old_key = format!("auth:{{{}}}:refresh:{}", user_id, old_jti);
        let exists = cache
            .ops()
            .exists(&old_key)
            .await
            .expect("查询旧 token 是否存在失败");
        assert!(!exists, "轮换后旧 token key 应被删除");

        // 5. 存储新 refresh token，新令牌应生效
        store_refresh_token(&cache, user_id, new_jti, "web")
            .await
            .expect("存储新 refresh token 失败");
        let new_key = format!("auth:{{{}}}:refresh:{}", user_id, new_jti);
        let new_exists = cache
            .ops()
            .exists(&new_key)
            .await
            .expect("查询新 token 是否存在失败");
        assert!(new_exists, "新 refresh token 应存储成功");

        // 清理
        let _ = cache.ops().del(&new_key).await;
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh_index", user_id))
            .await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_refresh_rotation_replay_rejected() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let user_id = "rotation-test-user-replay";
        let old_jti = "old-jti-replay";

        // 清理
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh:{}", user_id, old_jti))
            .await;
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh_index", user_id))
            .await;

        // 1. 存储旧 refresh token
        store_refresh_token(&cache, user_id, old_jti, "web")
            .await
            .expect("存储旧 refresh token 失败");

        let rotation = RefreshRotation::new(cache.clone());

        // 2. 并发执行两次轮换（同一个 old_jti）
        // 由于 Lua 脚本在 Redis 端原子执行，仅一次应返回 true
        let (r1, r2) = tokio::join!(
            rotation.rotate_refresh_token(user_id, old_jti),
            rotation.rotate_refresh_token(user_id, old_jti),
        );

        let r1 = r1.expect("第一次轮换调用失败");
        let r2 = r2.expect("第二次轮换调用失败");

        // 3. 恰好一次成功
        let success_count = [r1, r2].iter().filter(|&&v| v).count();
        assert_eq!(
            success_count, 1,
            "并发轮换同一 refresh_token 应只成功一次，实际成功 {} 次 (r1={}, r2={})",
            success_count, r1, r2
        );

        // 清理
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh_index", user_id))
            .await;
    }

    #[tokio::test]
    #[ignore = "需要真实 Redis 实例，设置 CMX_TEST_REDIS_URL=redis://127.0.0.1:6379 后运行: cargo test -p cmx-auth -- --ignored"]
    async fn test_refresh_rotation_nonexistent_returns_false() {
        let cache = match make_cache_manager().await {
            Some(c) => c,
            None => return,
        };

        let user_id = "rotation-test-user-nonexistent";
        let nonexistent_jti = "does-not-exist";

        // 清理（确保 key 不存在）
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh:{}", user_id, nonexistent_jti))
            .await;
        let _ = cache
            .ops()
            .del(&format!("auth:{{{}}}:refresh_index", user_id))
            .await;

        let rotation = RefreshRotation::new(cache.clone());

        // 轮换不存在的 token 应返回 false（可能重放攻击）
        let result = rotation
            .rotate_refresh_token(user_id, nonexistent_jti)
            .await
            .expect("轮换调用失败");

        assert!(!result, "不存在的旧 token 轮换应返回 false（重放检测）");
    }
}
