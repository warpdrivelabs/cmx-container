# cmx-registry-config 使用指南

> 注册中心与配置中心可扩展抽象层，支持 Nacos、Mock 及后续扩展（Consul、Etcd、Apollo 等）。

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
// 工厂函数根据配置类型返回对应实现
let registry: Arc<dyn ServiceRegistry> = create_registry(&config)?;
let config_center: Arc<dyn ConfigCenter> = create_config_center(&config)?;
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
| `SERVICE_REGISTRY_TYPE` | `mock` | 注册中心类型：`mock` / `nacos` |
| `SERVICE_REGISTRY_ENABLED` | `false` | 是否启用服务注册 |
| `SERVICE_REGISTRY_NAME` | `cmx-server` | 注册的服务名称（同时作为 `app_id` 的回退值） |
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
| `NACOS_ENABLED=true` | 自动启用注册中心 + 配置中心，类型设为 `nacos` |
| `NACOS_NAMING_ENABLED` | 映射为 `SERVICE_REGISTRY_ENABLED` |
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
    ServiceRegistry, ConfigCenter,
};

// 从环境变量加载配置
let registry_config = RegistryConfig::from_env();
let cc_config = ConfigCenterFullConfig::from_env();

// 创建实例（返回 Arc<dyn Trait>）
let registry: Arc<dyn ServiceRegistry> = create_registry(&registry_config)?;
let config_center: Arc<dyn ConfigCenter> = create_config_center(&cc_config)?;
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
use cmx_registry_config::GlobalChangeNotifier;

// 初始化（通常在应用启动时调用一次）
GlobalChangeNotifier::initialize();

// 注册回调
GlobalChangeNotifier::register("my-handler", Arc::new(|content: &str| {
    println!("收到配置变更: {}", content);
}));

// 通知所有处理器（通常由 ConfigCenter 的 listen 回调触发）
GlobalChangeNotifier::notify("新的配置内容...");
```

### 5.8 在任意 crate 中访问全局实例

全局单例 `GlobalServiceRegistry` 和 `GlobalConfigCenter` 定义在 `cmx-registry-config` crate 中，
任何依赖了该 crate 的模块都可以直接访问，无需通过 `web-server` 中转：

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

    // 注入预设配置
    center
        .set_config("app.toml", "DEFAULT_GROUP", "server.port = 9090")
        .await;

    // 获取配置
    let content = center.get_config("app.toml", "DEFAULT_GROUP").await.unwrap();
    assert_eq!(content, "server.port = 9090");

    // 模拟配置变更通知
    center
        .simulate_change("app.toml", "DEFAULT_GROUP", "server.port = 8080")
        .await;
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
    |service_name: &str, instances: &[ServiceInstance]| {
        info!(
            "服务 {} 实例变更，当前 {} 个",
            service_name,
            instances.len()
        );
        for inst in instances {
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
    Arc<dyn Fn(&str, &[ServiceInstance]) + Send + Sync>;
```

参数说明：
- `&str`：发生变更的服务名
- `&[ServiceInstance]`：变更后的实例列表（已过滤不健康实例）

#### 1.2 全局缓存读取（轻量场景）

无需订阅变更时，可直接读取缓存：

```rust
use cmx_registry_config::GlobalServiceInstanceCache;

let cache = GlobalServiceInstanceCache::get();

// 同步读快照
if let Some(instances) = cache.get("cmx-server") {
    println!("缓存中有 {} 个实例", instances.len());
}

// 按需拉取：缓存为空时触发注册中心拉取
let instances = cache.get_or_fetch("cmx-server").await?;
```

#### 1.3 与 volo gRPC 客户端集成

volo 负载均衡器自动监听实例变化，无需手动处理：

```rust
// cmx-rpc 内部已实现
// 1. subscribe_instances 注册 no-op callback
// 2. start_watch 注册 discover callback
// 3. cache.update 时 volo 收到 Change 事件并刷新负载均衡器
//
// 业务侧无需关心，调用 VoloGrpcClient::get_client 即可
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
        move |_service: &str, instances: &[ServiceInstance]| {
            // 异步发送最新实例列表
            let instances = instances.to_vec();
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

使用 `GlobalChangeNotifier` 集中管理多个配置订阅者：

```rust
use cmx_registry_config::GlobalChangeNotifier;

// 1. 初始化（应用启动时调用一次）
GlobalChangeNotifier::initialize();

// 2. 注册 keyed handler
GlobalChangeNotifier::register("config-reloader", Arc::new(|content: &str| {
    info!("收到配置变更，准备热更新");
    // 调用 ConfigReloader::reload
}));

// 3. 注册 typed listener（按 data_id/group 精确匹配）
GlobalChangeNotifier::add_listener(
    "cmx-server.toml",
    "DEFAULT_GROUP",
    Arc::new(|event| {
        info!("cmx-server.toml 变更:\n{}", event.content);
    }),
);
```

**API 总览**：

| 方法 | 用途 |
|------|------|
| `initialize()` | 初始化全局通知器（幂等） |
| `register(key, callback)` | 注册全局 handler（任何配置变更都触发） |
| `unregister(key)` | 移除指定 handler |
| `add_listener(data_id, group, callback)` | 注册精确匹配的 listener |
| `remove_listener(data_id, group)` | 移除指定 listener |
| `notify(content)` | 触发所有 handler（仅 handler） |
| `notify_listeners(event)` | 触发所有 listener 和 handler |

#### 2.3 监听机制原理

```
Nacos Server 配置变更
    │ 长轮询/推送
    ▼
ConfigCenter::listen 回调
    │ GlobalChangeNotifier::notify(content)
    ▼
ChangeNotifier::handlers（keyed handler）
    ├─→ config-reloader（应用启动时注册）
    └─→ 其他业务 handler
    
ChangeNotifier::listeners（typed listener）
    └─→ 精确匹配 (data_id, group) 后触发
```

#### 2.4 配置中心 listeners 配置（推荐）

通过 `ConfigCenterFullConfig::listeners` 统一声明订阅项，应用启动时自动注册：

```rust
use cmx_registry_config::{ConfigCenterFullConfig, GlobalConfigCenter, GlobalChangeNotifier};

// 1. 声明订阅
let cc_config = ConfigCenterFullConfig::from_env();
GlobalChangeNotifier::initialize();

// 2. 应用层注册 typed listener
for listener in &cc_config.listeners {
    let data_id = listener.data_id.clone();
    let group = listener.group.clone();
    GlobalChangeNotifier::add_listener(
        &data_id,
        &group,
        Arc::new(move |event| {
            info!("{} 变更:\n{}", data_id, event.content);
        }),
    );
}

// 3. create_config_center 时自动注册 SDK 级别监听
//    （推荐通过工厂函数而非手动 listen，避免双重触发）
```

#### 2.5 完整生产级示例

```rust
use std::sync::Arc;
use cmx_registry_config::{
    GlobalChangeNotifier, GlobalConfigCenter, ConfigChangeEvent,
};

async fn setup_config_watcher() -> anyhow::Result<()> {
    let center = GlobalConfigCenter::get();
    let config = cmx_registry_config::ConfigCenterFullConfig::from_env();

    // 1. 全局 keyed handler（不区分配置项）
    GlobalChangeNotifier::initialize();
    GlobalChangeNotifier::register(
        "audit-logger",
        Arc::new(|content: &str| {
            info!("[audit] 配置变更记录");
        }),
    );

    // 2. typed listener（按 data_id 精确匹配）
    for listener in &config.listeners {
        let data_id = listener.data_id.clone();
        let group = listener.group.clone();

        // 全局 typed listener
        GlobalChangeNotifier::add_listener(
            &data_id,
            &group,
            Arc::new(move |event: &ConfigChangeEvent| {
                info!("{} 变更:\n{}", data_id, event.content);
            }),
        );

        // SDK 级别监听（工厂函数内部会自动调用）
        // 此处也可手动 listen，但会与 create_config_center 内的自动注册重复
    }

    // 3. 主动拉取初始配置
    for listener in &config.listeners {
        let content = center
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
| **应用启动热更新** | `GlobalChangeNotifier::register(key, ...)` | 集中管理，支持 unregister |
| **按 data_id 精确处理** | `GlobalChangeNotifier::add_listener(data_id, group, ...)` | 避免无关配置触发回调 |
| **gRPC 服务发现** | `subscribe_instances` + `start_watch` | volo 自动接管负载均衡 |
| **本地开发无注册中心** | `MockRegistry` + 手动 `cache.update` | 内存级，无需外部依赖 |
| **避免双重触发** | 仅在 `create_config_center` 工厂函数内调用 `listen` | 工厂封装完整职责 |

### 4. 完整订阅流程图

```text
┌─────────────────────────────────────────────────────────┐
│  应用启动                                                 │
│  ├─ init_infra()                                        │
│  │   ├─ create_registry_with_cache                     │
│  │   ├─ create_config_center (内部自动 listen)         │
│  │   ├─ start_service_list_syncer                       │
│  │   └─ setup_config_listener                          │
│  │       └─ GlobalChangeNotifier::initialize + register │
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
│      └─ GlobalChangeNotifier::notify                    │
│          └─ 触发所有 handlers + 匹配 typed listeners    │
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
use super::trait_rs::{ServiceInstance, ServiceRegistry};

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

在 `src/registry/mod.rs` 的 `create_registry()` 中添加分支：

```rust
match config.registry_type.as_str() {
    "nacos" => Ok(Arc::new(NacosRegistry::new(&config.nacos)?)),
    "consul" => Ok(Arc::new(ConsulRegistry::new(&config.consul)?)),  // 新增
    "mock" => Ok(Arc::new(MockRegistry::new())),
    other => Err(RegistryError::UnsupportedType(other.to_string())),
}
```

### 步骤 3：添加配置结构

在 `src/config.rs` 中新增 Consul 配置。

### 无需修改任何调用方代码

---

## 八、架构说明

```
cmx-registry-config/src/
├── lib.rs                  # 模块入口，re-export 公共 API
├── error.rs                # RegistryError + ConfigCenterError
├── config.rs               # 配置模型，环境变量加载
├── config_source.rs        # RemoteConfigSource (config::Source 实现)
├── notifier.rs             # 配置变更通知器
├── registry/
│   ├── mod.rs              # 工厂函数 create_registry()
│   ├── trait_rs.rs         # ServiceRegistry trait + ServiceInstance
│   ├── nacos.rs            # NacosRegistry
│   └── mock.rs             # MockRegistry
└── config_center/
    ├── mod.rs              # 工厂函数 create_config_center()
    ├── trait_rs.rs         # ConfigCenter trait + ConfigChangeCallback
    ├── nacos.rs            # NacosConfigCenter + NacosListenerAdapter
    └── mock.rs             # MockConfigCenter
```

### 配置优先级（从高到低）

1. 环境变量
2. 远程配置中心拉取的内容
3. 本地 TOML 配置文件
4. 代码默认值
