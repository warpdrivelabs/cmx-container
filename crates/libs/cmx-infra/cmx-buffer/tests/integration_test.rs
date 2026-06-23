//! cmx-buffer 集成测试
//! 使用真实 Redis 进行测试

use cmx_buffer::cache::CacheManager;
use cmx_buffer::config::{LockConfig, RedisConfig};
use cmx_buffer::lock::LockManager;
use cmx_buffer::RedisClient;
use std::collections::HashMap;
use std::time::Duration;

const REDIS_HOST: &str = "192.168.137.95";
const REDIS_PORT: u16 = 32496;
const REDIS_DATABASE: u8 = 13;

fn get_redis_url() -> String {
    format!("redis://{}:{}/{}", REDIS_HOST, REDIS_PORT, REDIS_DATABASE)
}

async fn setup_client() -> RedisClient {
    let config = RedisConfig::new(get_redis_url())
        .with_key_prefix("cmx-buffer-test:");
    RedisClient::new(config).await.expect("Failed to create Redis client")
}

async fn cleanup_key(client: &RedisClient, key: &str) {
    let full_key = client.build_key(key);
    let mut conn = client.get_connection();
    let _: () = redis::cmd("DEL")
        .arg(&full_key)
        .query_async(&mut conn)
        .await
        .unwrap_or(());
}

// ==================== 缓存操作测试 ====================

#[tokio::test]
async fn test_cache_set_and_get() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_set_get";
    cleanup_key(&client, key).await;
    
    // 测试设置
    ops.set(key, "Hello Redis").await.unwrap();
    
    // 测试获取
    let value = ops.get(key).await.unwrap();
    assert_eq!(value, Some("Hello Redis".to_string()));
    
    // 清理
    cleanup_key(&client, key).await;
}

#[tokio::test]
async fn test_cache_get_not_exists() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_not_exists";
    cleanup_key(&client, key).await;
    
    let value = ops.get(key).await.unwrap();
    assert_eq!(value, None);
}

#[tokio::test]
async fn test_cache_del() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_del";
    cleanup_key(&client, key).await;
    
    // 先设置
    ops.set(key, "to be deleted").await.unwrap();
    
    // 验证存在
    assert!(ops.exists(key).await.unwrap());
    
    // 删除
    let deleted = ops.del(key).await.unwrap();
    assert!(deleted);
    
    // 验证已删除
    let exists = ops.exists(key).await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_cache_exists() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_exists";
    cleanup_key(&client, key).await;
    
    // 不存在
    assert!(!ops.exists(key).await.unwrap());
    
    // 设置后存在
    ops.set(key, "value").await.unwrap();
    assert!(ops.exists(key).await.unwrap());
    
    // 清理
    cleanup_key(&client, key).await;
}

#[tokio::test]
async fn test_cache_set_ex() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_set_ex";
    cleanup_key(&client, key).await;
    
    // 设置 2 秒过期
    ops.set_ex(key, "expires soon", Duration::from_secs(2)).await.unwrap();
    
    // 验证存在
    assert!(ops.exists(key).await.unwrap());
    
    // 等待过期
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // 验证已过期
    assert!(!ops.exists(key).await.unwrap());
    
    cleanup_key(&client, key).await;
}

// ==================== 序列化测试 ====================

#[tokio::test]
async fn test_cache_serialization() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_serialization";
    cleanup_key(&client, key).await;
    
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct User {
        id: u32,
        name: String,
    }
    
    let user = User {
        id: 1,
        name: "Alice".to_string(),
    };
    
    // 设置序列化
    ops.set_serialized(key, &user).await.unwrap();
    
    // 获取反序列化
    let retrieved: Option<User> = ops.get_deserialized(key).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), user);
    
    // 清理
    cleanup_key(&client, key).await;
}

// ==================== 批量操作测试 ====================

#[tokio::test]
async fn test_cache_mget() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    // 清理测试键
    for i in 0..3u8 {
        cleanup_key(&client, &format!("test_mget_{}", i)).await;
    }
    
    // 设置多个值
    let mut items = HashMap::new();
    items.insert("test_mget_0", "value0");
    items.insert("test_mget_1", "value1");
    items.insert("test_mget_2", "value2");
    ops.mset(items).await.unwrap();
    
    // 批量获取
    let keys = vec!["test_mget_0", "test_mget_1", "test_mget_2"];
    let values = ops.mget(&keys).await.unwrap();
    
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].as_deref(), Some("value0"));
    assert_eq!(values[1].as_deref(), Some("value1"));
    assert_eq!(values[2].as_deref(), Some("value2"));
    
    // 清理
    for i in 0..3u8 {
        cleanup_key(&client, &format!("test_mget_{}", i)).await;
    }
}

#[tokio::test]
async fn test_cache_incr() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ops = cache.ops();
    
    let key = "test_incr";
    cleanup_key(&client, key).await;
    
    // 自增
    let count = ops.incr(key, 1).await.unwrap();
    assert_eq!(count, 1);
    
    // 再次自增
    let count = ops.incr(key, 5).await.unwrap();
    assert_eq!(count, 6);
    
    // 自减
    let count = ops.decr(key, 2).await.unwrap();
    assert_eq!(count, 4);
    
    // 清理
    cleanup_key(&client, key).await;
}

// ==================== TTL 操作测试 ====================

#[tokio::test]
async fn test_ttl_expire() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ttl = cache.ttl();
    let ops = cache.ops();
    
    let key = "test_ttl_expire";
    cleanup_key(&client, key).await;
    
    // 设置值
    ops.set(key, "value").await.unwrap();
    
    // 设置过期时间
    ttl.expire(key, Duration::from_secs(2)).await.unwrap();
    
    // 验证 TTL
    let ttl_result = ttl.ttl(key).await.unwrap();
    assert!(ttl_result.is_some());
    assert!(ttl_result.unwrap().as_secs() <= 2);
    
    // 等待过期
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // 验证已过期
    assert!(!ops.exists(key).await.unwrap());
    
    cleanup_key(&client, key).await;
}

#[tokio::test]
async fn test_ttl_persist() {
    let client = setup_client().await;
    let cache = CacheManager::new(client.clone());
    let ttl = cache.ttl();
    let ops = cache.ops();
    
    let key = "test_ttl_persist";
    cleanup_key(&client, key).await;
    
    // 设置带过期
    ops.set_ex(key, "permanent", Duration::from_secs(10)).await.unwrap();
    
    // 移除过期
    ttl.persist(key).await.unwrap();
    
    // 验证永不过期
    let ttl_result = ttl.ttl(key).await.unwrap();
    assert!(ttl_result.is_none());
    
    // 清理
    cleanup_key(&client, key).await;
}

// ==================== 分布式锁测试 ====================

#[tokio::test]
async fn test_lock_try_lock() {
    let client = setup_client().await;
    let lock_config = LockConfig::new().with_expire(5);
    let lock_manager = LockManager::new(client.clone(), lock_config);
    
    let key = "test_lock_try";
    cleanup_key(&client, &format!("lock:{}", key)).await;
    
    // 第一次获取锁
    let guard = lock_manager.try_lock(key).await.unwrap();
    assert!(guard.is_some());
    
    // 第二次获取锁（应该失败）
    let result = lock_manager.try_lock(key).await.unwrap();
    assert!(result.is_none());
    
    // 释放锁（Drop 自动释放）
    drop(guard);
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 清理
    cleanup_key(&client, &format!("lock:{}", key)).await;
}

#[tokio::test]
async fn test_lock_guard() {
    let client = setup_client().await;
    let lock_config = LockConfig::new().with_expire(5);
    let lock_manager = LockManager::new(client.clone(), lock_config);
    
    let key = "test_lock_guard";
    cleanup_key(&client, &format!("lock:{}", key)).await;
    
    // 获取锁守卫
    let _guard = lock_manager.lock(key).await.unwrap();
    
    // 验证锁存在
    let is_locked = lock_manager.is_locked(key).await.unwrap();
    assert!(is_locked);
    
    // 手动释放
    _guard.unlock().await.unwrap();
    
    // 验证锁已释放
    let is_locked = lock_manager.is_locked(key).await.unwrap();
    assert!(!is_locked);
    
    // 清理
    cleanup_key(&client, &format!("lock:{}", key)).await;
}

#[tokio::test]
async fn test_lock_extend() {
    let client = setup_client().await;
    let lock_config = LockConfig::new().with_expire(2);
    let lock_manager = LockManager::new(client.clone(), lock_config);
    
    let key = "test_lock_extend";
    cleanup_key(&client, &format!("lock:{}", key)).await;
    
    // 获取锁
    let guard = lock_manager.lock(key).await.unwrap();
    
    // 延长锁时间
    guard.extend(Duration::from_secs(10)).await.unwrap();
    
    // 验证剩余时间
    let remaining = lock_manager.remaining_ttl(key).await.unwrap();
    assert!(remaining.is_some());
    assert!(remaining.unwrap().as_secs() >= 5);
    
    // 清理
    drop(guard);
    cleanup_key(&client, &format!("lock:{}", key)).await;
}

#[tokio::test]
async fn test_lock_auto_release() {
    let client = setup_client().await;
    let lock_config = LockConfig::new().with_expire(2);
    let lock_manager = LockManager::new(client.clone(), lock_config);
    
    let key = "test_lock_auto";
    cleanup_key(&client, &format!("lock:{}", key)).await;
    
    {
        let _guard = lock_manager.lock(key).await.unwrap();
        assert!(lock_manager.is_locked(key).await.unwrap());
    }
    
    // guard 作用域结束，应该自动释放
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!lock_manager.is_locked(key).await.unwrap());
    
    // 清理
    cleanup_key(&client, &format!("lock:{}", key)).await;
}
