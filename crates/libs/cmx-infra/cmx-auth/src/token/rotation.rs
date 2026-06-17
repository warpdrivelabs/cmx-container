//! Refresh Token Rotation Lua 原子操作
//!
//! 使用 Redis Lua 脚本实现"检查旧 token 是否存在 + 原子删除"，
//! 消除 exists → revoke → store 三步操作的竞态条件。
//! 新 token 的创建由 issue_token_pair 的 store_refresh_token 完成。

use cmx_buffer::CacheManager;

use crate::error::Result;

/// Refresh Token Rotation Lua 脚本
///
/// 原子操作：检查旧 token 是否存在 → 删除旧 token → 从 index 移除旧 jti
///
/// # KEYS
/// - KEYS[1] = auth:{user_id}:refresh:{old_jti}
/// - KEYS[2] = auth:{user_id}:refresh_index
///
/// # ARGV
/// - ARGV[1] = old_jti
///
/// # 返回值
/// - 1: 成功（旧 token 存在并已删除）
/// - 0: 失败（旧 token 不存在，可能重放）
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
    /// 创建新的 RefreshRotation 管理器
    pub fn new(cache: CacheManager) -> Self {
        Self { cache }
    }

    /// 原子检查并删除旧 Refresh Token
    ///
    /// 使用 Lua 脚本原子执行：检查旧 jti 是否存在 → 删除旧 key → 从 index 移除旧 jti。
    /// 新 token 的创建由 `store_refresh_token` 完成。
    ///
    /// # 返回
    /// - `Ok(true)`: 旧 token 存在并已删除
    /// - `Ok(false)`: 旧 token 不存在（可能重放攻击）
    pub async fn rotate_refresh_token(
        &self,
        user_id: &str,
        old_jti: &str,
    ) -> Result<bool> {
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
