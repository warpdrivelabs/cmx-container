//! cmx-buffer 分布式锁回归测试
//!
//! 使用 Mock Redis 后端（`MockRedisBackend`）测试 `LockManager` 和 `LockGuard`，
//! 覆盖加锁、解锁、续期、看门狗自动续期等核心场景，无需依赖真实 Redis 服务。

use cmx_buffer::client::{RedisClient, RedisConnectionRef};
use cmx_buffer::config::{CacheConfig, LockConfig, RedisConfig};
use cmx_buffer::lock::{LockManager, LockOptions};
use cmx_buffer::mock::{MockConnection, MockRedisBackend};
use std::sync::Arc;
use std::time::Duration;

/// 构造一个使用 Mock 后端的 LockManager，返回 (manager, backend)
///
/// `expire_seconds` 控制锁默认 TTL；`renew_threshold` 控制看门狗续期阈值。
fn setup_lock_manager(
    expire_seconds: u64,
    renew_threshold: f64,
) -> (LockManager, Arc<MockRedisBackend>) {
    let backend = Arc::new(MockRedisBackend::new());
    let connection = RedisConnectionRef::Mock(MockConnection(backend.clone()));

    let redis_config = RedisConfig::new("redis://mock").with_key_prefix("cmx:");
    let cache_config = CacheConfig::new();
    let mut lock_config = LockConfig::new()
        .with_expire(expire_seconds)
        .with_retry_interval(20);
    lock_config.renew_threshold = renew_threshold;

    let client = RedisClient::new_with_connection(
        connection,
        redis_config,
        cache_config,
        lock_config.clone(),
    );
    (LockManager::new(client, lock_config), backend)
}

/// 计算带前缀的完整锁键
fn full_lock_key(key: &str) -> String {
    format!("cmx:lock:{}", key)
}

// ==================== 加锁/解锁测试 ====================

#[tokio::test]
async fn test_try_lock_success() {
    // 加锁成功：首次 try_lock 应返回 Some(guard)，且 is_locked 返回 true
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    let guard = manager.try_lock("resource_1").await.expect("加锁应该成功");
    assert!(guard.is_some(), "首次加锁应返回 Some");

    let is_locked = manager.is_locked("resource_1").await.expect("查询锁状态失败");
    assert!(is_locked, "锁应该处于持有状态");

    // 主动释放以避免后台看门狗任务干扰后续测试
    guard.unwrap().unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_try_lock_conflict_when_held() {
    // 重复加锁失败：锁已被持有时，再次 try_lock 应返回 None
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    let guard = manager.try_lock("resource_2").await.expect("首次加锁应成功");
    assert!(guard.is_some(), "首次加锁应返回 Some");

    // 不释放，再次尝试加锁
    let second = manager
        .try_lock("resource_2")
        .await
        .expect("查询锁状态不应返回错误");
    assert!(second.is_none(), "锁已被持有时应返回 None");

    guard.unwrap().unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_unlock_then_relock() {
    // 解锁后可以重新加锁
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    let guard = manager.try_lock("resource_3").await.expect("首次加锁应成功");
    assert!(guard.is_some());

    // 主动释放
    guard.unwrap().unlock().await.expect("解锁失败");

    // 等待 unlock 异步任务完成（LockGuard::unlock 通过 tokio::spawn 间接释放，但这里走的是同步路径）
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 锁应该已经释放
    let is_locked = manager.is_locked("resource_3").await.expect("查询失败");
    assert!(!is_locked, "解锁后锁不应再持有");

    // 重新加锁应成功
    let guard2 = manager.try_lock("resource_3").await.expect("重新加锁应成功");
    assert!(guard2.is_some(), "释放后应能重新加锁");
    guard2.unwrap().unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_lock_ttl_expire_auto_release() {
    // 锁的 TTL 过期后自动释放
    //
    // 使用 lease_time 禁用看门狗，确保锁在固定时间后过期
    let (manager, backend) = setup_lock_manager(30, 0.3);

    let guard = manager
        .try_lock_with_options(
            "resource_4",
            LockOptions::new().with_lease_time(Duration::from_secs(1)),
        )
        .await
        .expect("加锁应成功");
    assert!(guard.is_some(), "加锁应返回 Some");

    let lock_key = full_lock_key("resource_4");
    assert!(
        backend.exists_raw(lock_key.as_bytes()).await,
        "加锁后 mock 中应存在锁键"
    );

    // 等待 TTL 过期（1 秒 + 余量）
    tokio::time::sleep(Duration::from_millis(1300)).await;

    // mock backend 在下一次访问时会清理过期 key
    let is_locked = manager.is_locked("resource_4").await.expect("查询失败");
    assert!(!is_locked, "TTL 过期后锁应自动释放");

    // 丢弃 guard 避免 Drop 再次触发 unlock（锁已过期，unlock 会失败但只是日志告警）
    let _ = guard;
}

// ==================== 续期测试 ====================

#[tokio::test]
async fn test_lock_extend_by_owner() {
    // 持有者续期成功：guard.extend 应返回 Ok，且 TTL 被延长
    let (manager, _backend) = setup_lock_manager(2, 0.3);

    let guard = manager.lock("resource_5").await.expect("加锁失败");

    // 续期到 10 秒
    guard.extend(Duration::from_secs(10)).await.expect("续期失败");

    // 验证剩余 TTL 被延长
    let remaining = manager
        .remaining_ttl("resource_5")
        .await
        .expect("查询 TTL 失败");
    assert!(remaining.is_some(), "锁应仍存在");
    let remaining_secs = remaining.unwrap().as_secs();
    assert!(
        (5..=10).contains(&remaining_secs),
        "续期后 TTL 应在 [5, 10] 秒之间，实际: {}",
        remaining_secs
    );

    // 主动释放以停止看门狗后台任务
    guard.unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_lock_extend_by_non_owner_fails() {
    // 非持有者无法续期：当锁值不匹配时，extend 应返回 Err
    //
    // 通过 set_raw 模拟"其他客户端持有锁"，使当前 guard 的 lock_value 不匹配
    let (manager, backend) = setup_lock_manager(5, 0.3);

    let guard = manager.lock("resource_6").await.expect("加锁失败");
    let original_value = guard.lock_value().to_string();

    // 模拟其他客户端覆盖了锁值（持有者变更）
    let lock_key = full_lock_key("resource_6");
    backend
        .set_raw(
            lock_key.as_bytes(),
            b"other_owner_value",
            Some(Duration::from_secs(5)),
        )
        .await;

    // 当前 guard 尝试续期，应因 lock_value 不匹配而失败
    let result = guard.extend(Duration::from_secs(10)).await;
    assert!(
        result.is_err(),
        "非持有者续期应失败，original_value={}, current=other_owner_value",
        original_value
    );

    // 主动释放以停止看门狗后台任务（unlock 也会因 lock_value 不匹配而失败，但只是返回 Err）
    let _ = guard.unlock().await;
}

// ==================== 看门狗自动续期测试 ====================

#[tokio::test]
async fn test_watchdog_auto_renewal() {
    // 看门狗在 TTL 到期前自动续期
    //
    // 配置：expire=4s, renew_threshold=0.75 → threshold=3s, renew_interval=2s
    // 时序：
    //   t=0:  加锁，ttl=4
    //   t=2:  看门狗触发，ttl=2 < 3，续期到 4
    //   t=3:  检查时 ttl 应在 (2, 4] 之间（说明已被续期，否则 ttl=1）
    let (manager, _backend) = setup_lock_manager(4, 0.75);

    let guard = manager.lock("resource_7").await.expect("加锁失败");

    // 等待 3 秒，让看门狗触发至少一次续期
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 验证锁仍然存在
    let is_locked = manager.is_locked("resource_7").await.expect("查询失败");
    assert!(is_locked, "看门狗应已续期，锁应仍存在");

    // 验证 TTL 被续期（若未续期，t=3 时 ttl=1；若已续期，t=3 时 ttl 应 > 1）
    let remaining = manager
        .remaining_ttl("resource_7")
        .await
        .expect("查询 TTL 失败");
    assert!(remaining.is_some(), "锁应存在且有剩余 TTL");
    let remaining_secs = remaining.unwrap().as_secs();
    assert!(
        remaining_secs > 1,
        "看门狗续期后 TTL 应 > 1 秒，实际: {}（未续期则应=1）",
        remaining_secs
    );

    // 主动释放以停止看门狗后台任务
    guard.unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_watchdog_keeps_lock_alive_beyond_ttl() {
    // 看门狗持续续期，使锁存活时间超过原始 TTL
    //
    // 配置：expire=4s, renew_threshold=0.75 → threshold=3s, renew_interval=2s
    // 等待 6 秒（> expire=4s），验证锁仍存在
    let (manager, _backend) = setup_lock_manager(4, 0.75);

    let guard = manager.lock("resource_8").await.expect("加锁失败");

    // 等待 6 秒，远超原始 TTL（4s）
    tokio::time::sleep(Duration::from_secs(6)).await;

    // 验证锁仍然存在（如果没有看门狗续期，4 秒后锁应已过期）
    let is_locked = manager.is_locked("resource_8").await.expect("查询失败");
    assert!(is_locked, "看门狗应已多次续期，锁应存活超过原始 TTL");

    // 主动释放以停止看门狗
    guard.unlock().await.expect("解锁失败");
}

// ==================== LockOptions 行为测试 ====================

#[tokio::test]
async fn test_lease_time_disables_watchdog() {
    // 指定 lease_time 时禁用看门狗，锁在 lease_time 后过期
    //
    // 对比 test_watchdog_keeps_lock_alive_beyond_ttl：这里指定 lease_time=2s，
    // 等待 3 秒后锁应已过期（无看门狗续期）
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    let guard = manager
        .try_lock_with_options(
            "resource_9",
            LockOptions::new().with_lease_time(Duration::from_secs(2)),
        )
        .await
        .expect("加锁应成功");
    assert!(guard.is_some());

    // 等待超过 lease_time
    tokio::time::sleep(Duration::from_secs(3)).await;

    let is_locked = manager.is_locked("resource_9").await.expect("查询失败");
    assert!(!is_locked, "指定 lease_time 时锁应在过期后释放");

    let _ = guard;
}

#[tokio::test]
async fn test_try_lock_with_wait_time_succeeds_after_release() {
    // try_lock_with_options 配合 wait_time：在锁释放后能获取到锁
    let (manager, _backend) = setup_lock_manager(2, 0.3);

    // 先用一个 lease_time 锁占用资源
    let holder = manager
        .try_lock_with_options(
            "resource_10",
            LockOptions::new().with_lease_time(Duration::from_secs(1)),
        )
        .await
        .expect("首次加锁应成功");
    assert!(holder.is_some());
    let holder = holder.unwrap();

    // 启动一个后台任务，在 500ms 后释放锁
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // 持有者主动释放锁
        let _ = holder.unlock().await;
    });

    // 用 wait_time=2s 等待获取锁
    let guard = manager
        .try_lock_with_options(
            "resource_10",
            LockOptions::new()
                .with_wait_time(Duration::from_secs(2))
                .with_retry_interval(Duration::from_millis(100)),
        )
        .await
        .expect("查询锁不应返回错误");

    assert!(guard.is_some(), "在 wait_time 内应能获取到锁");

    guard.unwrap().unlock().await.ok();
}

#[tokio::test]
async fn test_try_lock_with_wait_time_timeout() {
    // try_lock_with_options 配合 wait_time：超时未获取到锁返回 None
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    // 先占用锁（用 lease_time 禁用看门狗，确保锁不会自动续期）
    let holder = manager
        .try_lock_with_options(
            "resource_11",
            LockOptions::new().with_lease_time(Duration::from_secs(30)),
        )
        .await
        .expect("首次加锁应成功");
    assert!(holder.is_some());

    // 用很短的 wait_time 尝试获取，应超时返回 None
    let start = std::time::Instant::now();
    let result = manager
        .try_lock_with_options(
            "resource_11",
            LockOptions::new()
                .with_wait_time(Duration::from_millis(300))
                .with_retry_interval(Duration::from_millis(100)),
        )
        .await
        .expect("查询锁不应返回错误");

    let elapsed = start.elapsed();
    assert!(result.is_none(), "在 wait_time 内未获取到锁应返回 None");
    assert!(
        elapsed >= Duration::from_millis(250),
        "应至少等待接近 wait_time 的时间，实际: {:?}",
        elapsed
    );

    holder.unwrap().unlock().await.ok();
}

// ==================== LockGuard 行为测试 ====================

#[tokio::test]
async fn test_lock_guard_is_valid_and_key() {
    // 验证 LockGuard 的 is_valid / key / lock_value 访问器
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    let guard = manager.try_lock("resource_12").await.expect("加锁失败");
    assert!(guard.is_some());
    let guard = guard.unwrap();

    assert!(guard.is_valid(), "刚获取的锁应有效");
    assert_eq!(guard.key(), "resource_12");
    assert!(!guard.lock_value().is_empty(), "lock_value 不应为空");

    guard.unlock().await.expect("解锁失败");
}

#[tokio::test]
async fn test_lock_guard_drop_auto_releases() {
    // LockGuard 离开作用域后通过 Drop 自动释放
    let (manager, _backend) = setup_lock_manager(30, 0.3);

    {
        let _guard = manager.lock("resource_13").await.expect("加锁失败");
        assert!(manager.is_locked("resource_13").await.unwrap());
    }

    // Drop 触发异步释放任务，等待一会
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !manager.is_locked("resource_13").await.unwrap(),
        "Drop 后锁应被自动释放"
    );
}
