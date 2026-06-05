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
    GlobalRegistry, GlobalConfigCenter,
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

支持两种配置方式：**环境变量**（推荐，兼容现有 `NACOS_*` 变量）和 **TOML 配置文件**。

### 3.1 环境变量（推荐）

#### 注册中心环境变量

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVICE_REGISTRY_TYPE` | `mock` | 注册中心类型：`mock` / `nacos` |
| `SERVICE_REGISTRY_ENABLED` | `false` | 是否启用服务注册 |
| `NACOS_SERVER_ADDR` | `127.0.0.1:8848` | Nacos 服务器地址 |
| `NACOS_NAMESPACE` | `""` | 命名空间 |
| `NACOS_APP_NAME` | `cmx-container` | 应用名称 |
| `NACOS_USERNAME` | - | 认证用户名（可选） |
| `NACOS_PASSWORD` | - | 认证密码（可选） |
| `NACOS_NAMING_SERVICE_NAME` | `cmx-server` | 注册的服务名称 |
| `NACOS_NAMING_GROUP_NAME` | `DEFAULT_GROUP` | 注册的服务分组 |
| `NACOS_REGISTER_SERVER_IP` | 自动检测 | 注册使用的 IP 地址 |
| `NACOS_REGISTER_SERVER_PORT` | `server.port` 配置值 | 注册使用的端口号 |

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
| `NACOS_NAMING_SERVICE_NAME` | 同时作为 `APP_ID` 的回退值 |

### 3.2 TOML 配置文件

在应用的 TOML 配置文件中添加：

```toml
# 服务注册中心配置
[service_registry]
# 注册中心类型
type = "nacos"
# 是否启用
enabled = true

# Nacos 注册中心配置
[service_registry.nacos]
server_addr = "127.0.0.1:8848"
namespace = ""
app_name = "cmx-container"
username = ""
password = ""
service_name = "cmx-server"
group_name = "DEFAULT_GROUP"
cluster_name = "DEFAULT"
weight = 1.0

# 配置中心配置
[config_center]
# 配置中心类型
type = "nacos"
# 是否启用
enabled = true

# Nacos 配置中心配置
[config_center.nacos]
server_addr = "127.0.0.1:8848"
namespace = ""
app_name = "cmx-container"
username = ""
password = ""

# 配置监听列表
[[config_center.listeners]]
data_id = "cmx-server.toml"
group = "DEFAULT_GROUP"

# 应用标识
[app]
id = "my-app"
```

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
export NACOS_NAMING_SERVICE_NAME=cmx-server
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

全局单例 `GlobalRegistry` 和 `GlobalConfigCenter` 定义在 `cmx-registry-config` crate 中，
任何依赖了该 crate 的模块都可以直接访问，无需通过 `web-server` 中转：

```rust
use cmx_registry_config::{GlobalRegistry, GlobalConfigCenter};

// 获取全局注册中心（必须在 init_infra() 之后调用）
let registry = GlobalRegistry::get();
registry.register(&instance).await?;

// 获取全局配置中心
let config_center = GlobalConfigCenter::get();
let config = config_center.get_config("app.toml", "DEFAULT_GROUP").await?;

// 检查是否已初始化
if GlobalRegistry::is_initialized() {
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
