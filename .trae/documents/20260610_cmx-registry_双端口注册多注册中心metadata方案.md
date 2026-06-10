# cmx-registry 双端口注册多注册中心 metadata 方案

## 一、背景与问题

cmx 服务同时暴露两个端口：

* **RESTful 服务端口**：8080（axum HTTP server）

* **RPC 端口**：9090（volo gRPC server）

使用注册中心时，需要将两个端口信息都注册到注册中心，使服务消费者能够：

* 通过 HTTP 端口调用 RESTful API

* 通过 gRPC 端口发起 RPC 调用

当前工程仅实现了 Nacos 注册中心，需要评估其他注册中心（etcd、Consul、ZooKeeper）是否也能支持双端口注册。

## 二、现状分析

### 2.1 当前实现（已完成）

当前代码**已经通过 metadata 机制实现了双端口注册**：

**注册侧**（[infra\_init.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/infra_init.rs)）：

```rust
// HTTP 端口作为 ServiceInstance.port 注册
let port = resolve_register_port(); // 8080
let mut instance = registry_config.build_instance(ip, port);

// gRPC 端口通过 metadata 附加
if let Some(rpc) = load_rpc_config() {
    if rpc.enabled {
        instance.metadata.insert("grpc_port".to_string(), rpc.grpc.port.to_string());
    }
}
registry.register(&instance).await;
```

**发现侧**（[discover.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-rpc/src/discover.rs)）：

```rust
fn instances_to_volo(instances: &[ServiceInstance]) -> Vec<Arc<Instance>> {
    instances.iter().filter_map(|i| {
        // 优先使用 metadata 中的 grpc_port，回退到 ServiceInstance.port
        let port = i.metadata
            .get("grpc_port")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(i.port);
        // ...
    }).collect()
}
```

**数据模型**（[trait\_rs.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-registry-config/src/registry/trait_rs.rs)）：

```rust
pub struct ServiceInstance {
    pub ip: String,
    pub port: u16,                        // HTTP 端口
    pub service_name: String,
    pub group_name: Option<String>,
    pub cluster_name: Option<String>,
    pub weight: f64,
    pub metadata: HashMap<String, String>, // 已有，含 grpc_port
    pub healthy: bool,
    pub ephemeral: bool,
}
```

### 2.2 现有架构图

```
                    注册中心 (Nacos)
                        |
          注册时: port=8080, metadata.grpc_port=9090
                        |
                        v
              +-------------------+
              | ServiceInstance   |
              | Cache (全局单例)   |
              +-------------------+
                   /           \
                  /             \
                 v               v
    +------------------+  +-------------------+
    | Web Server       |  | gRPC Server       |
    | (axum, :8080)    |  | (volo, :9090)     |
    +------------------+  +-------------------+
                                  ^
                    RegistryAwareDiscover
                    (优先用 metadata.grpc_port)
                                  |
                                  v
                        +------------------+
                        | VoloGrpcClient   |
                        +------------------+
```

## 三、各注册中心 metadata 支持调研

### 3.1 Nacos（已实现）

| 特性          | 说明                                      |
| ----------- | --------------------------------------- |
| metadata 类型 | `HashMap<String, String>`，原生 KV 映射      |
| 存储方式        | 服务实例的 `metadata` 字段，直接 KV 存储            |
| 容量限制        | 无硬性限制，建议总大小 < 1KB                       |
| 查询支持        | 控制台和 API 均可查看                           |
| 适配方式        | **零转换**，`ServiceInstance.metadata` 直接映射 |

**结论**：✅ 完美支持，当前实现已正确工作。

### 3.2 Consul

| 特性   | 说明                                         |
| ---- | ------------------------------------------ |
| Tags | `string[]`，标签数组，支持 `key=value` 格式          |
| Meta | `map<string,string>`，原生 KV 映射（Consul 1.0+） |
| 推荐方式 | 使用 `Meta` 字段，语义与 Nacos metadata 一致         |
| 备选方式 | 使用 `Tags`，格式如 `grpc_port=9090`             |

**适配映射**：

```
ServiceInstance.metadata  →  Consul Service.Meta
"grpc_port" => "9090"    →  Meta: {"grpc_port": "9090"}
```

**结论**：✅ 完美支持，通过 `Meta` 字段直接映射，无需格式转换。

### 3.3 etcd

| 特性          | 说明                                                  |
| ----------- | --------------------------------------------------- |
| 数据模型        | 纯 KV 存储，无"服务实例"原生概念                                 |
| metadata 方式 | 将实例信息（含 metadata）序列化为 JSON 存入 value                 |
| 注册模式        | key = `/services/{name}/{instance_id}`，value = JSON |

**适配映射**：

```
ServiceInstance 序列化为 JSON 存入 etcd value：
key:   /services/cmx-server/192.168.1.100:8080
value: {"ip":"192.168.1.100","port":8080,"metadata":{"grpc_port":"9090"},...}
```

**结论**：✅ 支持。etcd 本身就是 KV 存储，value 可以存储任意结构化数据，metadata 自然包含在 JSON 中。

### 3.4 ZooKeeper

| 特性          | 说明                                                    |
| ----------- | ----------------------------------------------------- |
| 数据模型        | 树形 znode，每个节点可存 byte\[] 数据                            |
| metadata 方式 | 将实例信息序列化为 JSON 存入 znode data                          |
| 注册模式        | 临时节点路径 = `/services/{name}/{instance_id}`，data = JSON |
| Dubbo 实践    | Dubbo 在 ZK 中存储 JSON 格式的 URL 参数，含 metadata             |

**适配映射**：

```
ServiceInstance 序列化为 JSON 存入 znode data：
path: /services/cmx-server/192.168.1.100:8080
data: {"ip":"192.168.1.100","port":8080,"metadata":{"grpc_port":"9090"},...}
```

**结论**：✅ 支持。与 etcd 类似，znode data 可存储任意字节数据，metadata 包含在 JSON 中。

### 3.5 汇总对比

| 注册中心      | metadata 支持       | 映射方式      | 转换复杂度 | 可行性   |
| --------- | ----------------- | --------- | ----- | ----- |
| Nacos     | 原生 HashMap        | 直接映射      | 零     | ✅ 已实现 |
| Consul    | 原生 Meta + Tags    | Meta 直接映射 | 零     | ✅ 可行  |
| etcd      | JSON value        | 序列化/反序列化  | 低     | ✅ 可行  |
| ZooKeeper | znode data (JSON) | 序列化/反序列化  | 低     | ✅ 可行  |

**核心结论**：所有主流注册中心都能支持 metadata 传递双端口信息。当前 `ServiceInstance.metadata: HashMap<String, String>` 的抽象设计是正确的，可以跨注册中心通用。

## 四、方案设计

### 4.1 设计原则

1. **统一抽象**：`ServiceInstance.metadata` 作为跨注册中心的统一 metadata 载体
2. **注册侧注入**：在 `register_service()` 中统一注入 metadata，与具体注册中心实现解耦
3. **发现侧消费**：在 `instances_to_volo()` 中统一消费 metadata，与具体注册中心实现解耦
4. **各实现适配**：每个注册中心实现负责 `ServiceInstance.metadata` 与其原生 metadata 格式的双向转换

### 4.2 metadata 规范

定义标准 metadata key，所有注册中心实现必须支持：

| Key         | 类型     | 说明          | 示例            |
| ----------- | ------ | ----------- | ------------- |
| `grpc_port` | string | gRPC 服务端口   | `"9090"`      |
| `version`   | string | 服务版本号（预留）   | `"1.0.0"`     |
| `protocol`  | string | 支持的协议列表（预留） | `"http,grpc"` |

### 4.3 各注册中心实现的适配要求

每个注册中心实现（`impl ServiceRegistry`）必须确保：

1. **注册时**：`ServiceInstance.metadata` 完整写入注册中心的原生 metadata 字段
2. **查询/订阅时**：从注册中心读取原生 metadata，完整还原到 `ServiceInstance.metadata`
3. **注销时**：能通过 `ip + port + service_name` 准确定位并删除实例

具体适配方式：

| 注册中心      | 注册时 metadata 写入                                       | 查询时 metadata 读取                                       |
| --------- | ----------------------------------------------------- | ----------------------------------------------------- |
| Nacos     | `instance.metadata` → `NacosServiceInstance.metadata` | `NacosServiceInstance.metadata` → `instance.metadata` |
| Consul    | `instance.metadata` → `ServiceEntry.Meta`             | `ServiceEntry.Meta` → `instance.metadata`             |
| etcd      | `ServiceInstance` 整体序列化为 JSON value                   | JSON value 反序列化为 `ServiceInstance`                    |
| ZooKeeper | `ServiceInstance` 整体序列化为 JSON znode data              | JSON znode data 反序列化为 `ServiceInstance`               |

### 4.4 当前代码需修复的问题

#### 问题 1：shutdown\_infra 注销时未携带 grpc\_port metadata

**文件**：[infra\_init.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/infra_init.rs)

**现状**：`register_service()` 中注入了 `grpc_port` 到 metadata，但 `shutdown_infra()` 中构建注销实例时未注入。

**影响**：Nacos 注销时主要匹配 `ip + port + service_name`，功能上不受影响。但对于 etcd/ZooKeeper 等基于 JSON 匹配的实现，可能导致注销失败。

**修复**：在 `shutdown_infra()` 中也注入 `grpc_port` metadata，保持与注册时一致。

#### 问题 2：Nacos 反向转换时 group\_name 丢失

**文件**：[nacos.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-registry-config/src/registry/nacos.rs)

**现状**：`convert_from_nacos_instance()` 中 `group_name: None`，未从 Nacos 实例中恢复 group\_name。

**影响**：查询到的实例缺少 group\_name 信息，可能影响分组过滤。

**修复**：从 Nacos 响应中提取 group\_name（Nacos SDK 的 `group_name` 字段或从 `serviceName` 中解析 `group_name@@service_name` 格式）。

## 五、实施计划

### 步骤 1：修复 Nacos 反向转换 group\_name 丢失

**修改文件**：`crates/libs/cmx-infra/cmx-registry-config/src/registry/nacos.rs`

在 `convert_from_nacos_instance()` 中恢复 group\_name：

```rust
// 从 serviceName 解析 group_name（Nacos 格式：group_name@@service_name）
let (group_name, service_name) = match &nacos_instance.service_name {
    Some(name) if name.contains("@@") => {
        let parts: Vec<&str> = name.splitn(2, "@@").collect();
        (Some(parts[0].to_string()), parts[1].to_string())
    }
    other => (None, other.clone().unwrap_or_default()),
};

ServiceInstance {
    ip: nacos_instance.ip.clone(),
    port: nacos_instance.port as u16,
    service_name,
    group_name,
    // ... 其余字段不变
}
```

### 步骤 2：为未来注册中心实现编写适配指南

在 `ServiceRegistry` trait 的文档注释中明确 metadata 适配要求，确保后续实现 Consul/etcd/ZooKeeper 时遵循统一规范。

**修改文件**：`crates/libs/cmx-infra/cmx-registry-config/src/registry/trait_rs.rs`

在 `ServiceRegistry` trait 上添加文档注释，说明 metadata 处理要求。

### 步骤 3：激活 RegistryConfig.metadata 死字段 + 统一 grpc\_port 注入

**问题**：`RegistryConfig.metadata` 当前是死字段——`from_env()` 和 `from_config_manager()` 都初始化为 `HashMap::new()`，从未从配置源加载。`grpc_port` 是在 `register_service()` 中手动 `instance.metadata.insert()` 注入的，绕过了 `RegistryConfig.metadata`。

**重要说明**：`grpc_port` 的值来源于 `[rpc.grpc] port = 9090`（gRPC 服务器监听端口），**不是**让用户在 `[registry.metadata]` 中手动配置。`[registry.metadata]` 是给用户放自定义 metadata 的（如 version、region 等），`grpc_port` 由代码自动从 RPC 配置中读取并注入。

数据流：

```
[rpc.grpc] port = 9090  →  代码读取 rpc.grpc.port  →  自动注入 metadata["grpc_port"]  →  注册到注册中心
```

**修改文件 1**：`crates/libs/cmx-infra/cmx-registry-config/src/config.rs`

在 `from_config_manager()` 中从配置文件加载 `[registry.metadata]` 段（用户自定义 metadata）：

```rust
// 在 from_config_manager() 中，metadata 初始化处替换 HashMap::new()：
metadata: config_manager.get_string_map("registry.metadata").unwrap_or_default(),
```

**修改文件 2**：`crates/web/web-server/src/config/infra_init.rs`

将 `grpc_port` 注入逻辑统一到 `RegistryConfig.metadata` 层面，在 `build_instance()` 调用前注入：

```rust
fn register_service(registry: &Arc<dyn ServiceRegistry>) {
    let port = resolve_register_port();
    let ip = resolve_register_ip();
    let mut registry_config = RegistryConfig::from_env();
    // 从 RPC 配置自动注入 grpc_port 到 metadata（无需用户手动配置）
    if let Some(rpc) = load_rpc_config() {
        if rpc.enabled {
            registry_config.metadata.insert("grpc_port".to_string(), rpc.grpc.port.to_string());
        }
    }
    let instance = registry_config.build_instance(ip.clone(), port);
    registry.register(&instance).await;
}
```

同样在 `shutdown_infra()` 中也统一注入，解决注销时 metadata 不一致的问题。

**修改文件 3**：`config/config_template.toml`

添加 metadata 配置段模板（仅用于用户自定义 metadata，grpc\_port 自动注入）：

```toml
[registry.metadata]
# grpc_port 由 [rpc.grpc].port 自动注入，无需手动配置
# 以下为自定义 metadata 示例：
# version = "1.0.0"
# region = "cn-east"
```

**效果**：

* `RegistryConfig.metadata` 不再是死字段，用户可在配置文件中添加自定义 metadata

* `grpc_port` 注入逻辑统一到一处，注册和注销保持一致

* 用户自定义 metadata 和系统自动注入的 metadata 合并后一起注册

## 六、验证步骤

1. **编译验证**：`rtk cargo check` 确保无编译错误
2. **功能验证**：启动服务后检查 Nacos 控制台，确认实例 metadata 中包含 `grpc_port`
3. **注销验证**：优雅关闭服务后，确认实例从 Nacos 正确注销
4. **发现验证**：通过 RPC 客户端调用，确认能正确从 metadata 获取 `grpc_port` 并连接

## 七、决策与假设

| 决策项             | 选择                                  | 理由                                     |
| --------------- | ----------------------------------- | -------------------------------------- |
| metadata 存储方式   | `HashMap<String, String>`           | 当前设计已满足所有注册中心需求                        |
| 主端口选择           | HTTP 8080 作为 `ServiceInstance.port` | HTTP 是服务的主入口，gRPC 是辅助协议                |
| metadata key 命名 | `grpc_port`                         | 语义清晰，与协议名对应                            |
| 是否注册为两个独立服务     | 否                                   | 同一服务实例暴露两种协议，应注册为一个实例                  |
| Consul 适配方式     | 使用 `Meta` 而非 `Tags`                 | `Meta` 是原生 KV 映射，与 Nacos metadata 语义一致 |

