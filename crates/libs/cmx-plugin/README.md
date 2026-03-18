# cmx-plugin 插件生命周期管理

cmx-plugin 是 CMX 框架的核心插件管理模块，提供完整的插件生命周期管理功能，包括插件的安装、卸载、激活、停用、升级、降级、回滚等操作。

## 功能特性

- **插件生命周期管理** - 完整的安装、卸载、激活、停用、升级、降级、回滚支持
- **多源获取** - 支持从 Zip 文件、URL、注册表、本地目录获取插件
- **语义版本** - 完整的语义版本解析和依赖约束处理（支持 ^、~、范围、通配符）
- **依赖解析** - 自动依赖解析和循环依赖检测
- **多节点部署** - 支持串行、并行、滚动、蓝绿四种部署策略
- **安全验证** - 插件签名验证、完整性校验、权限检查
- **审计日志** - 完整的操作审计日志记录
- **Redis 缓存** - 集成 cmx-buffer 提供 Redis 缓存支持
- **内存缓存** - 本地内存缓存层，支持 TTL 和 LRU 淘汰
- **分布式锁** - 支持分布式环境下的部署锁
- **消息队列** - 基于 Redis Pub/Sub 的事件通知机制
- **节点管理** - 集群节点注册、心跳、健康检查
- **服务注册** - 插件服务注册与发现
- **权限控制** - 插件权限白名单/黑名单管理

## 模块结构

```
cmx-plugin/src/
├── lib.rs           # 模块入口和导出
├── error.rs         # 错误类型定义
├── types.rs         # 类型定义（请求/响应/配置）
├── version.rs       # 语义版本和依赖解析
├── registry.rs      # 插件注册表（ZIP加载/签名验证）
├── fetcher.rs       # 插件源获取器
├── manager.rs       # 插件管理器（核心逻辑）
├── deployment.rs    # 部署协调器
├── activation.rs    # 激活管理和WASM运行时
├── audit.rs         # 审计日志
├── security.rs      # 安全验证器
├── repository.rs    # 数据库仓库结构
├── transaction.rs   # 事务管理
├── db.rs            # 数据库抽象层
├── db_impl.rs       # 数据库实现（CmxPluginDatabase）
├── config.rs        # TOML配置解析
├── cache.rs         # Redis缓存管理
├── memory_cache.rs  # 本地内存缓存
├── message.rs       # 消息队列（事件通知）
├── node.rs          # 节点管理器
├── service.rs       # 服务注册表
└── permission.rs    # 权限检查器
```

## 核心组件

### PluginManager

插件管理器是核心组件，负责插件的完整生命周期管理。

```rust
use cmx_plugin::{PluginManager, PluginManagerConfig};

// 创建配置
let config = PluginManagerConfig {
    install_root: PathBuf::from("/path/to/plugins"),
    temp_root: PathBuf::from("/tmp/cmx-plugin"),
    backup_root: PathBuf::from("/path/to/backups"),
    default_db_id: "default".to_string(),
    enable_backup: true,
    max_backup_count: 5,
    require_signature: false,
    registry_url: None,
};

// 创建管理器
let manager = PluginManager::new(config)?;
```

### 安装插件

```rust
use cmx_plugin::{InstallRequest, PluginSource};

let request = InstallRequest {
    plugin_id: Some("my-plugin".to_string()),
    source: PluginSource::Zip {
        path: "/path/to/plugin.zip".to_string(),
    },
    target_db_id: Some("plugin_db".to_string()),
    target_db_type: None,
    target_nodes: None,
    config: None,
    force: false,
    skip_validation: false,
    operator: "admin".to_string(),
};

let response = manager.install(request).await?;
```

### 激活插件

```rust
use cmx_plugin::{ActivateRequest};

let request = ActivateRequest {
    plugin_id: "my-plugin".to_string(),
    config: None,
    operator: "admin".to_string(),
};

let response = manager.activate(request).await?;
```

## 节点管理

### 使用节点管理器

```rust
use cmx_plugin::{NodeManager, NodeManagerConfig, NodeInfo, NodeType, NodeSelectionStrategy};

// 创建节点管理器
let config = NodeManagerConfig {
    heartbeat_timeout_seconds: 30,
    health_check_interval_seconds: 10,
    selection_strategy: NodeSelectionStrategy::RoundRobin,
};
let node_manager = NodeManager::new(config);

// 注册节点
let node = NodeInfo::new("node-001", "192.168.1.100", 8080)
    .with_name("主节点")
    .with_type(NodeType::Master);
node_manager.register(node).await?;

// 心跳更新
node_manager.heartbeat("node-001").await?;

// 获取健康节点
let healthy_nodes = node_manager.get_healthy_nodes().await;

// 按策略选择节点
let selected = node_manager.select_node().await;
```

## 消息队列

### 事件发布与订阅

```rust
use cmx_plugin::{
    MessageQueue, MessageQueueBuilder, PluginEvent, PluginEventType,
    DeploymentEvent, NodeEvent, SystemEvent,
};

// 创建消息队列
let mq = MessageQueueBuilder::new()
    .enabled(true)
    .redis_url("redis://localhost:6379")
    .build();

// 连接
mq.connect(&cache_manager).await?;

// 发布插件事件
let event = PluginEvent::new(
    PluginEventType::Installed,
    "my-plugin",
    "1.0.0"
);
mq.publish_plugin_event(event).await?;

// 发布部署事件
let deploy_event = DeploymentEvent::new(
    DeploymentEventType::Completed,
    "op-123",
    "my-plugin",
    "1.0.0"
);
mq.publish_deployment_event(deploy_event).await?;

// 注册事件处理器
mq.register_handler("cmx:plugin:events", Box::new(|msg| {
    println!("收到消息: {:?}", msg);
)).await;
```

## 内存缓存

### 使用本地内存缓存

```rust
use cmx_plugin::{MemoryCache, MemoryCacheConfig, PluginMemoryCacheManager, CacheKeyBuilder};

// 创建内存缓存
let config = MemoryCacheConfig {
    max_entries: 10000,
    default_ttl_seconds: 300,
    cleanup_interval_seconds: 60,
    enable_lru: true,
};
let cache = MemoryCache::<String>::new(config);

// 设置缓存
cache.set("key", "value".to_string()).await;
cache.set_with_ttl("key2", "value2".to_string(), Some(Duration::from_secs(60))).await;

// 获取缓存
let value = cache.get("key").await;

// 使用插件内存缓存管理器
let plugin_cache = PluginMemoryCacheManager::with_default_config();

// 缓存插件信息
let key = CacheKeyBuilder::plugin_info("my-plugin");
plugin_cache.plugin_info().set(&key, cache_value).await;
```

## 服务注册

### 注册和发现插件服务

```rust
use cmx_plugin::{ServiceRegistry, ServiceDescriptor, ServiceInstance};

// 创建服务注册表
let registry = ServiceRegistry::new();

// 注册服务
let descriptor = ServiceDescriptor {
    service_id: "data-processor".to_string(),
    service_name: "数据处理服务".to_string(),
    version: "1.0.0".to_string(),
    plugin_id: "data-plugin".to_string(),
    description: Some("提供数据处理功能".to_string()),
    endpoints: vec!["/api/process".to_string()],
    metadata: HashMap::new(),
};

let instance = ServiceInstance {
    instance_id: "instance-001".to_string(),
    service_id: "data-processor".to_string(),
    plugin_id: "data-plugin".to_string(),
    node_id: "node-001".to_string(),
    endpoint: "http://localhost:8080/api/process".to_string(),
    status: "active".to_string(),
    metadata: HashMap::new(),
};

registry.register_service(descriptor).await?;
registry.register_instance(instance).await?;

// 发现服务
let instances = registry.get_service_instances("data-processor").await?;
```

## 权限控制

### 使用权限检查器

```rust
use cmx_plugin::{PermissionChecker, Permission, PermissionType, PermissionPolicy};

// 创建权限检查器
let checker = PermissionChecker::new(PermissionPolicy::Strict);

// 定义权限
let permissions = vec![
    Permission::FileSystem { 
        paths: vec!["/data/plugins".to_string()], 
        mode: "rw".to_string() 
    },
    Permission::Network { 
        hosts: vec!["api.example.com".to_string()], 
        ports: vec![443] 
    },
    Permission::Database { 
        databases: vec!["plugin_db".to_string()], 
        operations: vec!["read".to_string(), "write".to_string()] 
    },
];

// 检查权限
let result = checker.check_permission("my-plugin", &permissions, &PermissionType::FileSystem {
    path: "/data/plugins/file.txt".to_string(),
    operation: "read".to_string(),
}).await?;

if result.allowed {
    println!("权限检查通过");
}
```

## 版本约束

### 使用版本约束解析

```rust
use cmx_plugin::{VersionConstraint, VersionConstraintParser, SemanticVersion};

// 解析版本约束
let constraint = VersionConstraintParser::parse("^1.2.3")?;
// 等价于 >=1.2.3, <2.0.0

let constraint = VersionConstraintParser::parse("~1.2.3")?;
// 等价于 >=1.2.3, <1.3.0

let constraint = VersionConstraintParser::parse(">=1.0.0, <2.0.0")?;
// 范围约束

// 查找最佳版本
let versions = vec![
    SemanticVersion::parse("1.0.0")?,
    SemanticVersion::parse("1.2.0")?,
    SemanticVersion::parse("2.0.0")?,
];
let best = VersionConstraintParser::find_best_version(&constraint, &versions)?;
```

## TOML 配置解析

### 从配置文件初始化系统插件

```rust
use cmx_plugin::SystemPluginsConfig;

// TOML 配置文件内容
let toml_content = r#"
[settings]
auto_activate = true
default_db_id = "default"
install_timeout_seconds = 300

[required.my-plugin]
version = "^1.0.0"
source = { type = "registry", registry = "default" }

[optional.optional-plugin]
version = "~2.0.0"
source = { type = "url", url = "https://example.com/plugin.zip" }
"#;

// 解析配置
let config: SystemPluginsConfig = toml::from_str(toml_content)?;

// 初始化系统插件
manager.init_system_plugins_from_config(config).await?;
```

## 数据库集成

### 使用 CmxPluginDatabase

```rust
use cmx_plugin::{CmxPluginDatabase, PluginDatabase};
use cmx_database::DatabaseManager;

// 创建数据库服务
let db_service = CmxPluginDatabase::new(db_manager);

// 插入插件记录
let record = PluginDbRecord {
    plugin_id: "my-plugin".to_string(),
    name: "My Plugin".to_string(),
    version: "1.0.0".to_string(),
    status: "installed".to_string(),
    // ...
};
db_service.insert_plugin(&record).await?;

// 查询插件
let plugin = db_service.get_plugin("my-plugin").await?;
```

## 部署策略

### 串行部署

```rust
use cmx_plugin::types::DeploymentStrategy;

let strategy = DeploymentStrategy::Serial {
    continue_on_error: true,
};
```

### 并行部署

```rust
let strategy = DeploymentStrategy::Parallel {
    max_concurrent: 5,
};
```

### 滚动部署

```rust
let strategy = DeploymentStrategy::Rolling {
    batch_size: 3,
    wait_seconds: 10,
};
```

### 蓝绿部署

```rust
let strategy = DeploymentStrategy::BlueGreen {
    switch_at: Some("now".to_string()),
};
```

## 依赖

```toml
[dependencies]
cmx-plugin = { path = "..." }

cmx-core = "0.1"           # 核心数据模型
cmx-metadata = "0.1"       # 元数据管理
cmx-buffer = "0.1"         # Redis缓存和分布式锁
cmx-database = "0.1"       # 数据库抽象
```

## 错误处理

模块定义了丰富的错误类型：

```rust
use cmx_plugin::PluginError;

match result {
    Ok(response) => { /* 成功 */ }
    Err(PluginError::NotFound(msg)) => { /* 资源不存在 */ }
    Err(PluginError::Dependency(msg)) => { /* 依赖冲突 */ }
    Err(PluginError::Security(msg)) => { /* 安全验证失败 */ }
    Err(PluginError::Install(msg)) => { /* 安装失败 */ }
    Err(PluginError::Activate(msg)) => { /* 激活失败 */ }
    Err(PluginError::Permission(msg)) => { /* 权限不足 */ }
    Err(PluginError::Node(msg)) => { /* 节点错误 */ }
    Err(PluginError::Service(msg)) => { /* 服务错误 */ }
    Err(e) => { /* 其他错误 */ }
}
```

## 最佳实践

1. **Always 验证插件** - 安装前使用 SecurityValidator 验证插件
2. **启用审计日志** - 记录所有关键操作
3. **使用事务** - 复杂操作使用 TransactionManager
4. **配置回滚** - 设置备份策略以便出现问题时回滚
5. **使用缓存** - 高频访问的插件信息使用 Redis 缓存和内存缓存
6. **权限控制** - 生产环境启用权限检查
7. **节点管理** - 分布式环境使用 NodeManager 管理集群节点
8. **事件通知** - 使用消息队列进行跨节点事件通知

## 许可证

MIT
