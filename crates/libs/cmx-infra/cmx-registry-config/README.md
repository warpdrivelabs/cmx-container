# cmx-registry-config 使用指南

> 注册中心与配置中心可扩展抽象层，支持 Nacos、Mock 及后续扩展（Consul、Etcd、Apollo 等）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 一、快速开始

### 1.1 添加依赖

在 crate 的 `Cargo.toml` 中添加：

```toml
# 内部依赖 - 注册中心与配置中心
cmx-registry-config = { workspace = true }
```

### 1.2 引入

```rust
use cmx_registry_config::{
    // 工厂函数
    create_registry, create_config_center,
    // 全局单例访问器
    GlobalServiceRegistry, GlobalConfigCenter,
    // trait
    ServiceRegistry, ConfigCenter,
    // 数据类型
    ServiceInstance, RegistryConfig, ConfigCenterFullConfig,
    // 错误类型
    RegistryError, ConfigCenterError,
};
```

---

## 二、核心概念

### 2.1 两个独立 trait

| Trait | 职责 | 核心方法 |
|-------|------|----------|
| `ServiceRegistry` | 微服务实例的注册、注销、发现 | `register()`, `deregister()`, `query_instances()` |
| `ConfigCenter` | 远程配置的获取和变更监听 | `get_config()`, `listen()` |

两者完全独立，可分别启用、分别选择不同实现类型。

### 2.2 动态派发

通过工厂函数 + `Arc<dyn Trait>` 实现运行时动态派发：

```rust
// 工厂函数根据配置类型返回对应实现（均为 async 函数）
let registry: Arc<dyn ServiceRegistry> = create_registry(&config).await?;
let config_center: Arc<dyn ConfigCenter> = create_config_center(&config, None).await?;
```

### 2.3 已有实现

| 实现类型 | 注册中心 | 配置中心 | 说明 |
|----------|---------|---------|------|
| `mock` | `MockRegistry` | `MockConfigCenter` | 内存级实现，默认值，适合本地开发和测试 |
| `nacos` | `NacosRegistry` | `NacosConfigCenter` | 基于 nacos-sdk 的完整实现 |

---

## 三、配置方式

通过 **环境变量** 配置，兼容现有 `NACOS_*` 变量。

### 3.1 注册中心环境变量

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVICE_REGISTRY_TYPE` | `mock`（`NACOS_ENABLED=true` 时默认 `nacos`） | 注册中心类型：`mock` / `nacos` |
| `SERVICE_REGISTRY_ENABLED` | `false` | 是否启用服务注册 |
| `SERVICE_REGISTRY_NAME` | `cmx-server` | 注册的服务名称（同时作为 `app_id` 的回退值） |
| `SERVICE_REGISTRY_GROUP` | `DEFAULT_GROUP` | 注册分组（回退 `NACOS_NAMING_GROUP_NAME`） |
| `SERVICE_REGISTRY_CLUSTER` | `DEFAULT` | 注册集群名 |
| `SERVICE_REGISTRY_WEIGHT` | `1.0` | 实例权重 |
| `NACOS_SERVER_ADDR` | `127.0.0.1:8848` | Nacos 服务器地址 |
| `NACOS_NAMESPACE` | `""` | 命名空间 |
| `NACOS_APP_NAME` | `cmx-container` | 应用名称 |
| `NACOS_USERNAME` | - | 认证用户名（可选） |
| `NACOS_PASSWORD` | - | 认证密码（可选） |
| `NACOS_NAMING_GROUP_NAME` | `DEFAULT_GROUP` | 注册的服务分组 |
| `SERVICE_REGISTRY_IP` | 自动检测 | 注册使用的 IP 地址（优先于 NACOS_REGISTER_SERVER_IP） |
| `SERVICE_REGISTRY_PORT` | `server.port` 配置值 | 注册使用的端口号（优先于 NACOS_REGISTER_SERVER_PORT） |
| `NACOS_REGISTER_SERVER_IP` | 自动检测 | 注册使用的 IP 地址（兼容旧变量） |
| `NACOS_REGISTER_SERVER_PORT` | `server.port` 配置值 | 注册使用的端口号（兼容旧变量） |

#### 配置中心环境变量

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CONFIG_CENTER_TYPE` | `mock` | 配置中心类型：`mock` / `nacos` |
| `CONFIG_CENTER_ENABLED` | `false` | 是否启用配置中心 |
| `NACOS_CONFIG_DATA_ID` | - | 配置 Data ID |
| `NACOS_CONFIG_GROUP` | `DEFAULT_GROUP` | 配置 Group |

#### 应用标识

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `APP_ID` | `default` | 应用隔离标识 |

#### 兼容旧环境变量

为保持向后兼容，旧 `NACOS_*` 变量仍然有效：

| 旧变量 | 新映射 |
|--------|--------|
| `NACOS_ENABLED=true` | 自动启用注册中心（需 `NACOS_NAMING_ENABLED`≠false，其默认 true）并将类型设为 `nacos`；配置中心还需叠加 `NACOS_CONFIG_ENABLED=true` 才自动启用 |
| `NACOS_NAMING_ENABLED` | 映射为 `SERVICE_REGISTRY_ENABLED` 的兼容开关（默认 true） |
| `NACOS_CONFIG_ENABLED` | 映射为 `CONFIG_CENTER_ENABLED` |
| `NACOS_NAMING_SERVICE_NAME` | 映射为 `SERVICE_REGISTRY_NAME` 的回退值 |

---

## 四、典型场景

### 4.1 本地开发（默认 Mock 模式）

无需设置任何环境变量，默认使用 Mock 实现：

```bash
# 直接启动，所有注册和配置操作在内存中完成
cargo run
```

### 4.2 连接 Nacos（环境变量方式）

```bash
export NACOS_ENABLED=true
export NACOS_SERVER_ADDR=192.168.1.100:8848
export NACOS_NAMESPACE=dev
export NACOS_USERNAME=nacos
export NACOS_PASSWORD=nacos
export SERVICE_REGISTRY_NAME=cmx-server
export NACOS_CONFIG_ENABLED=true
export NACOS_CONFIG_DATA_ID=cmx-server.toml
export NACOS_CONFIG_GROUP=DEFAULT_GROUP

cargo run
```

### 4.3 仅启用注册中心

```bash
export SERVICE_REGISTRY_TYPE=nacos
export SERVICE_REGISTRY_ENABLED=true
export NACOS_SERVER_ADDR=192.168.1.100:8848

cargo run
```

### 4.4 仅启用配置中心

```bash
export CONFIG_CENTER_TYPE=nacos
export CONFIG_CENTER_ENABLED=true
export NACOS_SERVER_ADDR=192.168.1.100:8848
export NACOS_CONFIG_DATA_ID=cmx-server.toml

cargo run
```

### 4.5 Docker / Kubernetes 部署

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: cmx-config
data:
  SERVICE_REGISTRY_TYPE: "nacos"
  SERVICE_REGISTRY_ENABLED: "true"
  SERVICE_REGISTRY_NAME: "cmx-server"
  NACOS_SERVER_ADDR: "nacos.default.svc.cluster.local:8848"
  NACOS_NAMESPACE: "production"
  CONFIG_CENTER_TYPE: "nacos"
  CONFIG_CENTER_ENABLED: "true"
  NACOS_CONFIG_DATA_ID: "cmx-server.toml"
  APP_ID: "cmx-server-prod"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cmx-container
spec:
  template:
    spec:
      containers:
        - name: cmx-container
          envFrom:
            - configMapRef:
                name: cmx-config
          env:
            - name: NACOS_USERNAME
              valueFrom:
                secretKeyRef:
                  name: nacos-credentials
                  key: username
            - name: NACOS_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: nacos-credentials
                  key: password
```

---

## 五、API 使用示例

### 5.1 通过工厂函数创建实例

```rust
use std::sync::Arc;
use cmx_registry_config::{
    create_registry, create_config_center,
    RegistryConfig, ConfigCenterFullConfig,
    ServiceRegistry, ConfigCenter, ConfigChangeCallback,
};

// 从环境变量加载配置
let registry_config = RegistryConfig::from_env();
let cc_config = ConfigCenterFullConfig::from_env();

// 创建注册中心实例（返回 Arc<dyn Trait>，async 函数）
let registry: Arc<dyn ServiceRegistry> = create_registry(&registry_config).await?;

// 创建配置中心实例。
// 第二个参数为可选的配置变更处理器：若提供，工厂函数会为每个 listener 自动注册此处理器；
// 若为 None，则调用方需自行通过 ConfigCenter::listen 注册。
let change_handler: Option<ConfigChangeCallback> = None;
let config_center: Arc<dyn ConfigCenter> = create_config_center(&cc_config, change_handler).await?;
```

### 5.2 服务注册

```rust
use cmx_registry_config::ServiceInstance;

let instance = ServiceInstance {
    ip: "192.168.1.10".to_string(),
    port: 8080,
    service_name: "cmx-server".to_string(),
    group_name: Some("DEFAULT_GROUP".to_string()),
    cluster_name: Some("DEFAULT".to_string()),
    weight: 1.0,
    metadata: Default::default(),
    healthy: true,
    ephemeral: true,
};

registry.register(&instance).await?;
```

### 5.3 服务注销

```rust
registry.deregister(&instance).await?;
```

### 5.4 查询服务实例

```rust
let instances = registry
    .query_instances("cmx-server", Some("DEFAULT_GROUP"), vec!["DEFAULT".to_string()])
    .await?;

for inst in &instances {
    println!("{}:{} (healthy={})", inst.ip, inst.port, inst.healthy);
}
```

### 5.5 获取远程配置

```rust
let content = config_center
    .get_config("cmx-server.toml", "DEFAULT_GROUP")
    .await?;
println!("配置内容: {}", content);
```

### 5.6 监听配置变更

```rust
use cmx_registry_config::ConfigChangeCallback;

let callback: ConfigChangeCallback = Arc::new(|content: &str| {
    println!("配置已变更: {}", content);
});

config_center
    .listen("cmx-server.toml", "DEFAULT_GROUP", callback)
    .await?;
```

### 5.7 全局配置变更通知器

```rust
use std::sync::Arc;
use cmx_registry_config::{
    GlobalChangeNotifier, ConfigChangeEvent, ConfigChangeListener,
};

// 1. 初始化（通常在应用启动时调用一次）
GlobalChangeNotifier::initialize();

// 2. 注册结构化监听器
struct MyListener {
    // interested_keys 返回 &[String]，非空列表需自持数据（如 Vec<String>）
    keys: Vec<String>,
}
impl ConfigChangeListener for MyListener {
    fn name(&self) -> &str { "my-listener" }

    fn interested_keys(&self) -> &[String] {
        // 空切片表示监听所有变更；非空切片按 key 前缀过滤
        &self.keys
    }

    fn on_change(&self, event: &ConfigChangeEvent) {
        println!("收到配置变更，变更的 keys: {:?}", event.changed_keys);
        println!("新配置内容:\n{}", event.raw_content);
    }
}

GlobalChangeNotifier::add_listener(Arc::new(MyListener { keys: vec![] }));

// 3. 通知所有监听器（通常由 ConfigReloader 在完成配置替换后调用）
let event = ConfigChangeEvent {
    changed_keys: vec!["server.port".to_string()],
    raw_content: "server.port = 9090".to_string(),
};
GlobalChangeNotifier::notify_listeners(&event);
```

### 5.8 在任意 crate 中访问全局实例

全局单例 `GlobalServiceRegistry` 和 `GlobalConfigCenter` 定义在 `cmx-registry-config` crate 中，
任何依赖了该 crate 的模块都可以直接访问（初始化由 `cmx-service-base` 的 `init_infra()` 完成）：

```rust
use cmx_registry_config::{GlobalServiceRegistry, GlobalConfigCenter};

// 获取全局注册中心（必须在 init_infra() 之后调用）
let registry = GlobalServiceRegistry::get();
registry.register(&instance).await?;

// 获取全局配置中心
let config_center = GlobalConfigCenter::get();
let config = config_center.get_config("app.toml", "DEFAULT_GROUP").await?;

// 检查是否已初始化
if GlobalServiceRegistry::is_initialized() {
    // ...
}
```

---

## 六、测试支持

### 6.1 使用 Mock 实现

```rust
use cmx_registry_config::{MockRegistry, MockConfigCenter, ServiceInstance};

#[tokio::test]
async fn test_service_registration() {
    let registry = MockRegistry::new();
    let instance = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 8080,
        service_name: "test-service".to_string(),
        group_name: None,
        cluster_name: None,
        weight: 1.0,
        metadata: Default::default(),
        healthy: true,
        ephemeral: true,
    };

    // 注册
    registry.register(&instance).await.unwrap();

    // 查询
    let instances = registry
        .query_instances("test-service", None, vec![])
        .await
        .unwrap();
    assert_eq!(instances.len(), 1);

    // 注销
    registry.deregister(&instance).await.unwrap();
    let instances = registry
        .query_instances("test-service", None, vec![])
        .await
        .unwrap();
    assert_eq!(instances.len(), 0);
}
```

### 6.2 MockConfigCenter 测试辅助方法

```rust
use cmx_registry_config::MockConfigCenter;

#[tokio::test]
async fn test_config_center() {
    let center = MockConfigCenter::new();

    // 注入预设配置（同步方法，无需 .await）
    center.set_config("app.toml", "DEFAULT_GROUP", "server.port = 9090");

    // 获取配置
    let content = center.get_config("app.toml", "DEFAULT_GROUP").await.unwrap();
    assert_eq!(content, "server.port = 9090");

    // 模拟配置变更通知（同步方法，无需 .await）
    center.simulate_change("app.toml", "DEFAULT_GROUP", "server.port = 8080");
}
```

---

## 六.5 服务实例变化与配置变化监听指南

本节详细介绍两种变更通知机制：注册中心的服务实例变化、配置中心的配置内容变化。

### 1. 监听服务实例变化

#### 1.1 核心 API：`ServiceRegistry::subscribe_instances`

| 方法 | 用途 | 备注 |
|------|------|------|
| `subscribe_instances(service_name, callback)` | 订阅服务实例变更 | 每个 service_name 只订阅一次（实现层去重） |

**完整示例**：

```rust
use std::sync::Arc;
use cmx_registry_config::{GlobalServiceRegistry, ServiceInstance};

// 1. 拉取初始实例列表并注册变更回调
let registry = GlobalServiceRegistry::get();
let callback: cmx_registry_config::InstanceChangeCallback = Arc::new(
    |service_name: String, instances: Vec<ServiceInstance>| {
        info!(
            "服务 {} 实例变更，当前 {} 个",
            service_name,
            instances.len()
        );
        for inst in &instances {
            info!("  - {}:{} (healthy={})", inst.ip, inst.port, inst.healthy);
        }
    },
);
registry
    .subscribe_instances("cmx-server", callback)
    .await?;

// 2. 主动查询当前实例
let instances = registry
    .query_instances("cmx-server", None, vec![])
    .await?;
println!("当前 {} 个实例", instances.len());
```

**callback 签名**：

```rust
pub type InstanceChangeCallback =
    Arc<dyn Fn(String, Vec<ServiceInstance>) + Send + Sync>;
```

参数说明：
- `String`：发生变更的服务名
- `Vec<ServiceInstance>`：变更后的实例列表（已过滤不健康实例）

#### 1.2 全局缓存读取（轻量场景）

无需订阅变更时，可直接读取缓存：

```rust
use cmx_registry_config::GlobalServiceInstanceCache;

let cache = GlobalServiceInstanceCache::get();

// 同步读快照
if let Some(instances) = cache.get("cmx-server") {
    println!("缓存中有 {} 个实例", instances.len());
}

// 按需拉取：缓存为空时执行传入的拉取闭包（泛型 F: FnOnce() -> Fut）
let instances = cache
    .get_or_fetch("cmx-server", || async {
        GlobalServiceRegistry::get()
            .query_instances("cmx-server", None, vec![])
    })
    .await?;
```

#### 1.3 与 volo gRPC 客户端集成

volo 负载均衡器自动监听实例变化，无需手动处理：

```rust
// cmx-rpc 内部已实现（client/infra.rs + discover.rs）
// 1. subscribe_instances 注册 no-op callback
// 2. start_watch 注册 discover callback
// 3. cache.update 时 volo 收到 Change 事件并刷新负载均衡器
//
// 业务侧无需关心，直接使用 cmx-rpcs 各皮肤 crate 生成的客户端即可
```

#### 1.4 监听机制原理

```
注册中心 (Nacos/Consul)
    │ 推送变更事件
    ▼
NacosInstanceListener::event
    │ cache.update(service_name, healthy_instances)
    ▼
ServiceInstanceCache::update
    │ 遍历 subscribers[service_name] 列表
    ├─→ no-op callback（subscribe_instances 注册）
    └─→ discover callback（start_watch 注册）→ volo LB
```

#### 1.5 完整生产级示例

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use cmx_registry_config::{GlobalServiceRegistry, ServiceInstance};

async fn watch_service_changes() -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::channel::<Vec<ServiceInstance>>(100);

    let callback: cmx_registry_config::InstanceChangeCallback = Arc::new(
        move |_service: String, instances: Vec<ServiceInstance>| {
            // 异步发送最新实例列表
            let tx = tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(instances).await;
            });
        },
    );

    GlobalServiceRegistry::get()
        .subscribe_instances("cmx-server", callback)
        .await?;

    // 消费实例变更事件
    while let Some(instances) = rx.recv().await {
        info!("业务逻辑处理：路由表更新 {:?} 个实例", instances.len());
    }
    Ok(())
}
```

---

### 2. 监听配置变化

#### 2.1 核心 API：`ConfigCenter::listen`

| 方法 | 用途 | 备注 |
|------|------|------|
| `listen(data_id, group, callback)` | 订阅指定配置项变更 | 同一 data_id 多次调用会重复触发回调 |
| `get_config(data_id, group)` | 主动拉取配置 | 不订阅变更通知 |

**完整示例**：

```rust
use std::sync::Arc;
use cmx_registry_config::{GlobalConfigCenter, ConfigChangeCallback};

let config_center = GlobalConfigCenter::get();

// 主动拉取
let content = config_center
    .get_config("cmx-server.toml", "DEFAULT_GROUP")
    .await?;
info!("当前配置:\n{}", content);

// 注册变更回调
let callback: ConfigChangeCallback = Arc::new(|content: &str| {
    info!("配置已变更:\n{}", content);
    // 在此触发配置热更新
});

config_center
    .listen("cmx-server.toml", "DEFAULT_GROUP", callback)
    .await?;
```

**callback 签名**：

```rust
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;
```

参数说明：
- `&str`：配置变更后的完整内容（已发布版本）

#### 2.2 全局通知器（推荐）

使用 `GlobalChangeNotifier` 集中管理多个配置订阅者，通过实现 `ConfigChangeListener` trait 注册结构化监听器：

```rust
use std::sync::Arc;
use cmx_registry_config::{
    GlobalChangeNotifier, ConfigChangeEvent, ConfigChangeListener,
};

// 1. 初始化（应用启动时调用一次）
GlobalChangeNotifier::initialize();

// 2. 实现结构化监听器
struct DatabaseListener {
    keys: Vec<String>,
}
impl ConfigChangeListener for DatabaseListener {
    fn name(&self) -> &str { "database-listener" }

    fn interested_keys(&self) -> &[String] {
        // 仅监听 database 前缀的配置变更，空切片则监听所有变更
        // （返回 &[String]，非空前缀列表需自持 Vec<String>）
        &self.keys
    }

    fn on_change(&self, event: &ConfigChangeEvent) {
        info!("数据库配置变更: {:?}", event.changed_keys);
        // 执行数据库连接池重建等业务逻辑
    }
}

// 3. 注册监听器（前缀过滤示例：keys = vec!["database".to_string()]）
GlobalChangeNotifier::add_listener(Arc::new(DatabaseListener { keys: vec!["database".to_string()] }));

// 4. 移除监听器（按 name 匹配）
GlobalChangeNotifier::remove_listener("database-listener");
```

**API 总览**：

| 方法 | 用途 |
|------|------|
| `initialize()` | 初始化全局通知器（幂等） |
| `add_listener(listener)` | 注册结构化监听器（实现 `ConfigChangeListener` trait） |
| `remove_listener(name)` | 按 `name()` 移除指定监听器 |
| `notify_listeners(event)` | 通知所有监听器（按 `interested_keys` 过滤） |

**ConfigChangeListener trait**：

```rust
pub trait ConfigChangeListener: Send + Sync {
    /// 监听器名称，用于日志和 remove_listener 匹配
    fn name(&self) -> &str;

    /// 感兴趣的配置键前缀列表
    /// - 空切片：监听所有变更
    /// - 非空切片：仅当 changed_keys 中任一 key 以某个 prefix 开头时触发 on_change
    fn interested_keys(&self) -> &[String] { &[] }

    /// 配置变更回调
    fn on_change(&self, event: &ConfigChangeEvent);
}
```

#### 2.3 监听机制原理

```
Nacos Server 配置变更
    │ 长轮询/推送
    ▼
ConfigCenter::listen 回调（change_handler）
    │ ConfigReloader::reload(content)
    │   ├─ 解析新配置 → 计算 changed_keys
    │   └─ 原子替换全局 ConfigManager
    ▼
GlobalChangeNotifier::notify_listeners(event)
    │ 遍历 listeners，按 interested_keys 过滤
    └─→ 触发匹配的 ConfigChangeListener::on_change
```

#### 2.4 配置中心 listeners 配置（推荐）

通过 `ConfigCenterFullConfig::listeners` 统一声明订阅项，`create_config_center` 工厂函数会自动为每个 listener 注册 `change_handler`：

```rust
use std::sync::Arc;
use cmx_registry_config::{
    ConfigCenterFullConfig, GlobalChangeNotifier, ConfigChangeEvent,
    ConfigChangeListener, ConfigChangeCallback, ConfigReloader,
};

// 1. 初始化全局通知器
GlobalChangeNotifier::initialize();

// 2. 构造配置变更处理器（负责解析配置 → 替换 ConfigManager → 通知监听器）
let config_file_path = std::env::var("CONFIG_FILE").ok();
let reloader = Arc::new(ConfigReloader::new(config_file_path));
let change_handler: Option<ConfigChangeCallback> = Some(Arc::new(move |content: &str| {
    let reloader = reloader.clone();
    let content = content.to_string();
    tokio::spawn(async move {
        if let Ok(changed_keys) = reloader.reload(&content).await {
            let event = ConfigChangeEvent {
                changed_keys,
                raw_content: content,
            };
            GlobalChangeNotifier::notify_listeners(&event);
        }
    });
}));

// 3. 创建配置中心时注入 change_handler，工厂函数自动为每个 listener 注册
let cc_config = ConfigCenterFullConfig::from_env();
let config_center = create_config_center(&cc_config, change_handler).await?;

// 4. 业务侧注册结构化监听器
struct BusinessListener;
impl ConfigChangeListener for BusinessListener {
    fn name(&self) -> &str { "business" }
    fn on_change(&self, event: &ConfigChangeEvent) {
        info!("业务配置变更: {:?}", event.changed_keys);
    }
}
GlobalChangeNotifier::add_listener(Arc::new(BusinessListener));
```

#### 2.5 完整生产级示例

```rust
use std::sync::Arc;
use cmx_registry_config::{
    GlobalChangeNotifier, ConfigChangeEvent, ConfigChangeListener,
    ConfigChangeCallback, ConfigReloader, ConfigCenterFullConfig,
    create_config_center,
};

async fn setup_config_watcher() -> anyhow::Result<()> {
    let config = ConfigCenterFullConfig::from_env();

    // 1. 初始化全局通知器
    GlobalChangeNotifier::initialize();

    // 2. 注册业务监听器（结构化，按 interested_keys 过滤）
    struct AuditListener;
    impl ConfigChangeListener for AuditListener {
        fn name(&self) -> &str { "audit" }
        fn on_change(&self, event: &ConfigChangeEvent) {
            info!("[audit] 配置变更记录，变更 keys: {:?}", event.changed_keys);
        }
    }
    GlobalChangeNotifier::add_listener(Arc::new(AuditListener));

    // 3. 构造配置变更处理器并创建配置中心
    let config_file_path = std::env::var("CONFIG_FILE").ok();
    let reloader = Arc::new(ConfigReloader::new(config_file_path));
    let change_handler: Option<ConfigChangeCallback> = Some(Arc::new(move |content: &str| {
        let reloader = reloader.clone();
        let content = content.to_string();
        tokio::spawn(async move {
            match reloader.reload(&content).await {
                Ok(changed_keys) => {
                    let event = ConfigChangeEvent {
                        changed_keys,
                        raw_content: content,
                    };
                    GlobalChangeNotifier::notify_listeners(&event);
                }
                Err(e) => {
                    tracing::warn!("配置热更新失败: {}", e);
                }
            }
        });
    }));

    let _config_center = create_config_center(&config, change_handler).await?;

    // 4. 主动拉取初始配置（可选）
    for listener in &config.listeners {
        let content = _config_center
            .get_config(&listener.data_id, &listener.group)
            .await?;
        info!("初始配置 {}:\n{}", listener.data_id, content);
    }
    Ok(())
}
```

---

### 3. 监听器注册最佳实践

| 场景 | 推荐方式 | 原因 |
|------|---------|------|
| **应用启动热更新** | `create_config_center(config, change_handler)` + `ConfigReloader` | 工厂函数自动注册，change_handler 负责解析配置并通知监听器 |
| **按配置键前缀过滤** | 实现 `ConfigChangeListener`，覆写 `interested_keys()` | 避免无关配置触发回调 |
| **业务模块订阅变更** | `GlobalChangeNotifier::add_listener(Arc::new(MyListener))` | 结构化监听器，类型安全，支持按 key 过滤 |
| **gRPC 服务发现** | `subscribe_instances` + `start_watch` | volo 自动接管负载均衡 |
| **本地开发无注册中心** | `MockRegistry` + 手动 `cache.update` | 内存级，无需外部依赖 |

### 4. 完整订阅流程图

```text
┌─────────────────────────────────────────────────────────┐
│  应用启动                                                 │
│  ├─ init_infra()（cmx-service-base）                  │
│  │   ├─ create_registry_with_cache                     │
│  │   ├─ GlobalChangeNotifier::initialize()             │
│  │   ├─ build_config_change_handler()                  │
│  │   │   └─ 构造 ConfigReloader + ConfigChangeCallback │
│  │   ├─ create_config_center(config, change_handler)   │
│  │   │   └─ 工厂函数为每个 listener 自动注册 change_handler │
│  │   ├─ ServiceListSyncer 启动（run + shutdown）       │
│  │   └─ 业务模块通过 add_listener 注册 ConfigChangeListener │
│  │                                                      │
│  └─ init_rpc()                                          │
│      ├─ warmup subscribe_instances                      │
│      └─ VoloGrpcClient 创建（内部 subscribe + watch）    │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  运行期                                                   │
│  ├─ Nacos 服务实例推送                                   │
│  │   └─ cache.update → 触发 no-op + discover callbacks │
│  │                                                      │
│  └─ Nacos 配置变更推送                                  │
│      └─ change_handler 被触发                           │
│          ├─ ConfigReloader::reload(content)             │
│          │   └─ 解析配置 → 计算 changed_keys            │
│          │   └─ 原子替换全局 ConfigManager              │
│          └─ GlobalChangeNotifier::notify_listeners(event)│
│              └─ 按 interested_keys 过滤并触发监听器     │
└─────────────────────────────────────────────────────────┘
```

---

## 七、扩展新实现

以添加 Consul 注册中心为例：

### 步骤 1：新增实现文件

创建 `src/registry/consul.rs`：

```rust
use async_trait::async_trait;
use crate::error::RegistryError;
use super::registry_traits::{ServiceInstance, ServiceRegistry};

pub struct ConsulRegistry {
    // consul client
}

impl ConsulRegistry {
    pub fn new(config: &ConsulConfig) -> Result<Self, RegistryError> {
        // 初始化 Consul 客户端
        todo!()
    }
}

#[async_trait]
impl ServiceRegistry for ConsulRegistry {
    async fn register(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        // 调用 Consul API 注册服务
        todo!()
    }

    async fn deregister(&self, instance: &ServiceInstance) -> Result<(), RegistryError> {
        todo!()
    }

    async fn query_instances(
        &self, service_name: &str, group_name: Option<&str>, clusters: Vec<String>,
    ) -> Result<Vec<ServiceInstance>, RegistryError> {
        todo!()
    }

    fn is_enabled(&self) -> bool { true }
}
```

### 步骤 2：注册到工厂函数

在 `src/registry/mod.rs` 的 `create_registry_with_cache()` 中添加分支
（`create_registry()` 内部委托它实现）：

```rust
match config.registry_type.as_str() {
    "nacos" => {
        let registry = NacosRegistry::new_with_cache(&config.nacos, cache.clone()).await?;
        Ok((Arc::new(registry), cache))
    }
    "consul" => {
        let registry = ConsulRegistry::new_with_cache(&config.consul, cache.clone()).await?;  // 新增
        Ok((Arc::new(registry), cache))
    }
    "mock" => Ok((Arc::new(MockRegistry::new_with_cache(cache.clone())), cache)),
    other => Err(RegistryError::UnsupportedType(other.to_string())),
}
```

### 步骤 3：添加配置结构

在 `src/config_model.rs` 中新增 Consul 配置。

### 无需修改任何调用方代码

---

## 八、架构说明

```
cmx-registry-config/src/
├── lib.rs                   # 模块入口，re-export 公共 API
├── error.rs                 # RegistryError + ConfigCenterError
├── config_model.rs          # 配置模型，环境变量加载
├── config_source.rs         # RemoteConfigSource (config::Source 实现)
├── notifier.rs              # GlobalChangeNotifier + ConfigChangeListener
├── reloader.rs              # ConfigReloader（解析新配置 → changed_keys，async reload）
├── global_registry.rs       # GlobalServiceRegistry 全局单例（set/get/is_initialized）
├── global_config_center.rs  # GlobalConfigCenter 全局单例
├── global_instance_cache.rs # GlobalServiceInstanceCache 全局单例
├── utils.rs                 # 工具函数
├── tests.rs                 # 集成测试
├── registry/
│   ├── mod.rs               # 工厂 create_registry / create_registry_with_cache
│   ├── registry_traits.rs   # ServiceRegistry trait + ServiceInstance + InstanceChangeCallback
│   ├── instance_cache.rs    # ServiceInstanceCache（实例缓存 + 变更回调分发）
│   ├── service_list_syncer.rs # ServiceListSyncer（服务列表定时同步）
│   ├── nacos.rs             # NacosRegistry
│   └── mock.rs              # MockRegistry
└── config_center/
    ├── mod.rs               # 工厂函数 create_config_center()
    ├── config_traits.rs     # ConfigCenter trait + ConfigChangeCallback
    ├── nacos.rs             # NacosConfigCenter + NacosListenerAdapter
    └── mock.rs              # MockConfigCenter
```

### 配置优先级（从高到低）

1. 环境变量
2. 远程配置中心拉取的内容
3. 本地 TOML 配置文件
4. 代码默认值
