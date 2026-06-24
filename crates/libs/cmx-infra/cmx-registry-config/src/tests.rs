//! cmx-registry-config 核心单元测试。

use std::sync::{Arc, Mutex};

use crate::config::RegistryConfig;
use crate::config_center::{ConfigCenter, MockConfigCenter};
use crate::notifier::{ChangeNotifier, ConfigChangeEvent};
use crate::registry::{MockRegistry, ServiceInstance, ServiceInstanceCache, ServiceRegistry};

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 创建测试用 ServiceInstance。
fn make_instance(ip: &str, port: u16, service_name: &str) -> ServiceInstance {
    ServiceInstance {
        ip: ip.to_string(),
        port,
        service_name: service_name.to_string(),
        group_name: None,
        cluster_name: None,
        weight: 1.0,
        metadata: Default::default(),
        healthy: true,
        ephemeral: true,
    }
}

// ===========================================================================
// MockRegistry 测试
// ===========================================================================

#[tokio::test]
async fn mock_registry_register_query_deregister() {
    let registry = MockRegistry::default();

    let inst1 = make_instance("10.0.0.1", 8080, "svc-a");
    let inst2 = make_instance("10.0.0.2", 8080, "svc-a");

    // 注册两个实例
    registry.register(&inst1).await.unwrap();
    registry.register(&inst2).await.unwrap();

    // 查询验证数量
    let instances = registry
        .query_instances("svc-a", None, vec![])
        .await
        .unwrap();
    assert_eq!(instances.len(), 2);

    // 注销一个
    registry.deregister(&inst1).await.unwrap();
    let instances = registry
        .query_instances("svc-a", None, vec![])
        .await
        .unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].ip, "10.0.0.2");

    // 注销最后一个
    registry.deregister(&inst2).await.unwrap();
    let instances = registry
        .query_instances("svc-a", None, vec![])
        .await
        .unwrap();
    assert_eq!(instances.len(), 0);
}

#[tokio::test]
async fn mock_registry_multi_service_isolation() {
    let registry = MockRegistry::default();

    // 不同服务注册相同 ip:port
    let inst_a = make_instance("10.0.0.1", 8080, "svc-a");
    let inst_b = make_instance("10.0.0.1", 8080, "svc-b");

    registry.register(&inst_a).await.unwrap();
    registry.register(&inst_b).await.unwrap();

    // 查询各自服务
    let a_instances = registry
        .query_instances("svc-a", None, vec![])
        .await
        .unwrap();
    assert_eq!(a_instances.len(), 1);

    let b_instances = registry
        .query_instances("svc-b", None, vec![])
        .await
        .unwrap();
    assert_eq!(b_instances.len(), 1);

    // 注销 svc-a 不应影响 svc-b（P1-3 修复后）
    registry.deregister(&inst_a).await.unwrap();
    let b_instances = registry
        .query_instances("svc-b", None, vec![])
        .await
        .unwrap();
    assert_eq!(b_instances.len(), 1, "注销 svc-a 不应影响 svc-b");
}

#[tokio::test]
async fn mock_registry_get_service_list() {
    let registry = MockRegistry::default();

    registry
        .register(&make_instance("10.0.0.1", 8080, "svc-a"))
        .await
        .unwrap();
    registry
        .register(&make_instance("10.0.0.2", 8080, "svc-b"))
        .await
        .unwrap();
    registry
        .register(&make_instance("10.0.0.3", 8080, "svc-a"))
        .await
        .unwrap();

    let list = registry.get_service_list().await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&"svc-a".to_string()));
    assert!(list.contains(&"svc-b".to_string()));
}

// ===========================================================================
// MockConfigCenter 测试
// ===========================================================================

#[tokio::test]
async fn mock_config_center_set_get() {
    let center = MockConfigCenter::new();

    center
        .set_config("app.toml", "DEFAULT_GROUP", "server.port = 9090");

    let content = center
        .get_config("app.toml", "DEFAULT_GROUP")
        .await
        .unwrap();
    assert_eq!(content, "server.port = 9090");
}

#[tokio::test]
async fn mock_config_center_get_not_found() {
    let center = MockConfigCenter::new();

    let result = center.get_config("missing.toml", "DEFAULT_GROUP").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn mock_config_center_listen_and_simulate_change() {
    let center = MockConfigCenter::new();

    let received = Arc::new(Mutex::new(String::new()));
    let received_clone = received.clone();

    let callback = Arc::new(move |content: &str| {
        *received_clone.lock().unwrap() = content.to_string();
    });

    center
        .listen("app.toml", "DEFAULT_GROUP", callback)
        .await
        .unwrap();

    // 模拟变更
    center
        .simulate_change("app.toml", "DEFAULT_GROUP", "server.port = 7070");

    // 验证回调被触发
    assert_eq!(*received.lock().unwrap(), "server.port = 7070");
}

// ===========================================================================
// ServiceInstanceCache 测试
// ===========================================================================

#[tokio::test]
async fn instance_cache_update_triggers_subscriber() {
    let cache = ServiceInstanceCache::new();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    cache.subscribe(
        "svc-a",
        Arc::new(move |_name, instances: &[ServiceInstance]| {
            let mut guard = received_clone.lock().unwrap();
            guard.extend(instances.iter().cloned());
        }),
    );

    // 更新缓存应触发回调
    let inst = make_instance("10.0.0.1", 8080, "svc-a");
    cache.update("svc-a", vec![inst.clone()]);

    let guard = received.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].ip, "10.0.0.1");
}

#[tokio::test]
async fn instance_cache_get_or_fetch_hit_and_miss() {
    let cache = ServiceInstanceCache::new();

    // 缓存未命中 -> fetch
    let inst = make_instance("10.0.0.1", 8080, "svc-a");
    let inst_clone = inst.clone();
    let result = cache
        .get_or_fetch("svc-a", || async {
            Ok::<_, crate::error::RegistryError>(vec![inst_clone.clone()])
        })
        .await
        .unwrap();
    assert_eq!(result.len(), 1);

    // 缓存命中 -> 直接返回（fetch 不应被调用）
    let result = cache
        .get_or_fetch("svc-a", || async {
            Err::<Vec<_>, _>(crate::error::RegistryError::QueryFailed(
                "不应执行".to_string(),
            ))
        })
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
}

// ===========================================================================
// ChangeNotifier 测试
// ===========================================================================

#[test]
fn change_notifier_listener_with_interested_keys_filter() {
    let notifier = ChangeNotifier::new();

    let call_count = Arc::new(Mutex::new(0u32));
    let call_count_clone = call_count.clone();

    struct DbListener {
        call_count: Arc<Mutex<u32>>,
    }

    impl crate::notifier::ConfigChangeListener for DbListener {
        fn name(&self) -> &str {
            "db-listener"
        }

        fn interested_keys(&self) -> &[String] {
            static KEYS: std::sync::LazyLock<Vec<String>> =
                std::sync::LazyLock::new(|| vec!["database".to_string()]);
            &KEYS
        }

        fn on_change(&self, _event: &ConfigChangeEvent) {
            *self.call_count.lock().unwrap() += 1;
        }
    }

    notifier.add_listener(Arc::new(DbListener {
        call_count: call_count_clone,
    }));

    // 不感兴趣的 key -> 不触发
    let event1 = ConfigChangeEvent {
        changed_keys: vec!["server.port".to_string()],
        raw_content: "".to_string(),
    };
    notifier.notify_listeners(&event1);
    assert_eq!(*call_count.lock().unwrap(), 0);

    // 感兴趣的 key -> 触发
    let event2 = ConfigChangeEvent {
        changed_keys: vec!["database.url".to_string()],
        raw_content: "".to_string(),
    };
    notifier.notify_listeners(&event2);
    assert_eq!(*call_count.lock().unwrap(), 1);
}

#[test]
fn change_notifier_listener_panic_isolation() {
    let notifier = ChangeNotifier::new();

    let received = Arc::new(Mutex::new(String::new()));
    let received_clone = received.clone();

    // 第一个监听器会 panic
    struct PanicListener;
    impl crate::notifier::ConfigChangeListener for PanicListener {
        fn name(&self) -> &str { "panic-listener" }
        fn on_change(&self, _event: &ConfigChangeEvent) {
            panic!("listener panic");
        }
    }

    // 第二个监听器应正常执行
    struct NormalListener {
        received: Arc<Mutex<String>>,
    }
    impl crate::notifier::ConfigChangeListener for NormalListener {
        fn name(&self) -> &str { "normal-listener" }
        fn on_change(&self, event: &ConfigChangeEvent) {
            *self.received.lock().unwrap() = event.raw_content.clone();
        }
    }

    notifier.add_listener(Arc::new(PanicListener));
    notifier.add_listener(Arc::new(NormalListener { received: received_clone }));

    // notify_listeners 不应 panic，第二个监听器应正常执行
    let event = ConfigChangeEvent {
        changed_keys: vec!["server.port".to_string()],
        raw_content: "config after panic".to_string(),
    };
    notifier.notify_listeners(&event);

    assert_eq!(*received.lock().unwrap(), "config after panic");
}

// ===========================================================================
// RegistryConfig::service_name() 优先级测试
// ===========================================================================

#[test]
fn config_service_name_priority() {
    use std::env;

    // 清理可能存在的环境变量
    unsafe {
        env::remove_var("SERVICE_REGISTRY_NAME");
        env::remove_var("NACOS_NAMING_SERVICE_NAME");
    }

    // 无环境变量 -> 默认值
    let config = RegistryConfig::from_env();
    assert_eq!(config.service_name(), "cmx-server");

    // 设置 NACOS_NAMING_SERVICE_NAME
    unsafe {
        env::set_var("NACOS_NAMING_SERVICE_NAME", "nacos-service");
    }
    let config = RegistryConfig::from_env();
    assert_eq!(config.service_name(), "nacos-service");

    // 设置 SERVICE_REGISTRY_NAME -> 优先级更高
    unsafe {
        env::set_var("SERVICE_REGISTRY_NAME", "registry-service");
    }
    let config = RegistryConfig::from_env();
    assert_eq!(config.service_name(), "registry-service");

    // 清理
    unsafe {
        env::remove_var("SERVICE_REGISTRY_NAME");
        env::remove_var("NACOS_NAMING_SERVICE_NAME");
    }
}

// ===========================================================================
// RegistryConfig::build_instance() 测试
// ===========================================================================

#[test]
fn config_build_instance() {
    let config = RegistryConfig::from_env();
    let instance = config.build_instance("10.0.0.1".to_string(), 8080);

    assert_eq!(instance.ip, "10.0.0.1");
    assert_eq!(instance.port, 8080);
    assert!(!instance.service_name.is_empty());
}
