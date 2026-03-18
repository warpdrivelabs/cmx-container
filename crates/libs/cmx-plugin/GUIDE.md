# cmx-plugin 完整使用指南

本文档详细介绍 cmx-plugin 的完整初始化流程和使用步骤。

## 目录

1. [系统架构概览](#1-系统架构概览)
2. [初始化流程](#2-初始化流程)
3. [核心组件初始化](#3-核心组件初始化)
4. [完整使用示例](#4-完整使用示例)
5. [插件生命周期流程](#5-插件生命周期流程)
6. [常见使用场景](#6-常见使用场景)
7. [高级功能](#7-高级功能)

---

## 1. 系统架构概览

```
┌─────────────────────────────────────────────────────────────────────┐
│                        应用层 (Your Application)                      │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      PluginManager (插件管理器)                       │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────┐  │
│  │   审计日志   │ │   安全验证   │ │   缓存管理   │ │ 数据库服务│  │
│  │ AuditLogger  │ │SecurityValid │ │CacheManager  │ │PluginDb   │  │
│  └──────────────┘ └──────────────┘ └──────────────┘ └───────────┘  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────┐  │
│  │   节点管理   │ │   消息队列   │ │   服务注册   │ │ 权限检查  │  │
│  │ NodeManager  │ │ MessageQueue │ │ServiceRegis  │ │Permission │  │
│  └──────────────┘ └──────────────┘ └──────────────┘ └───────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                    │
        ┌─────────────────────────────┼─────────────────────────────┐
        ▼                             ▼                             ▼
┌───────────────┐           ┌───────────────┐           ┌───────────────┐
│ ActivationMgr │           │DeploymentCoord│           │  Registry     │
│ (WASM运行时)  │           │(部署协调器)   │           │ (插件注册表)  │
└───────────────┘           └───────────────┘           └───────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        cmx-buffer (基础设施层)                        │
│  ┌────────────────────────┐  ┌────────────────────────────────────┐   │
│  │   Redis CacheManager   │  │      LockManager (分布式锁)        │   │
│  │   PubSub (消息队列)    │  │                                    │   │
│  └────────────────────────┘  └────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. 初始化流程

### 2.1 整体初始化顺序

```
1. 初始化 cmx-buffer (Redis 连接)
       │
       ▼
2. 创建/获取 DatabaseManager
       │
       ▼
3. 初始化 cmx-metadata (表定义)
       │
       ▼
4. 创建 PluginDatabase 实现
       │
       ▼
5. 创建 PluginCacheManager
       │
       ▼
6. 创建 NodeManager (节点管理)
       │
       ▼
7. 创建 MessageQueue (消息队列)
       │
       ▼
8. 创建 DeploymentCoordinator (带分布式锁)
       │
       ▼
9. 创建 PluginManager
       │
       ▼
10. 注册节点
       │
       ▼
11. 启动消息队列订阅
       │
       ▼
12. 系统就绪，可以开始使用
```

### 2.2 基础设施初始化

首先需要初始化基础设施层 (cmx-buffer):

```rust
// 基础设施初始化示例
async fn init_infrastructure() -> Result<(
    cmx_buffer::CacheManager,
    cmx_buffer::LockManager,
    cmx_database::DatabaseManager,
), Box<dyn std::error::Error>> {
    
    // 1. 初始化 Redis 连接 (cmx-buffer)
    let redis_config = cmx_buffer::RedisConfig {
        addr: "redis://localhost:6379".to_string(),
        password: None,
        db: Some(0),
        pool_size: 10,
    };
    
    let redis_client = cmx_buffer::RedisClient::new(redis_config)
        .await?;
    
    // 2. 创建缓存管理器和锁管理器
    let cache_manager = cmx_buffer::CacheManager::new(redis_client.clone());
    let lock_manager = cmx_buffer::LockManager::new(redis_client);
    
    // 3. 初始化数据库 (cmx-database)
    let db_config = cmx_database::DbConfig {
        db_type: cmx_database::DbType::Postgres,
        db_url: "postgresql://localhost:5432/cmx".to_string(),
        db_id: "default".to_string(),
        default: true,
        ..Default::default()
    };
    
    let db_manager = cmx_database::DatabaseManager::new(vec![db_config])
        .await?;
    
    Ok((cache_manager, lock_manager, db_manager))
}
```

---

## 3. 核心组件初始化

### 3.1 插件管理器初始化

```rust
use cmx_plugin::{
    PluginManager, PluginManagerConfig, PluginCacheManager, 
    DeploymentCoordinator, ActivationManager, NodeInfo, DeploymentNodeStatus,
    SecurityValidator, SecurityValidatorConfig, AuditLogger,
    NodeManager, NodeManagerConfig, MessageQueue, MessageQueueBuilder,
    ServiceRegistry, PermissionChecker, PermissionPolicy,
    CmxPluginDatabase,
};

// 配置参数
let config = PluginManagerConfig {
    install_root: PathBuf::from("/data/cmx/plugins"),
    temp_root: PathBuf::from("/tmp/cmx-plugin"),
    backup_root: PathBuf::from("/data/cmx/backups"),
    default_db_id: "default".to_string(),
    enable_backup: true,
    max_backup_count: 5,
    require_signature: false,
    registry_url: Some("https://plugins.example.com".to_string()),
};

// 创建各个组件
let audit_logger = Arc::new(AuditLogger::new());

let security_validator = Arc::new(SecurityValidator::new(
    SecurityValidatorConfig {
        require_signature: false,
        trusted_public_keys: vec![],
        verify_file_hash: true,
        max_plugin_size: 100 * 1024 * 1024,  // 100MB
        enable_sandbox: true,
        allowed_imports: vec!["env".to_string()],
    }
));

let cache_manager = Arc::new(PluginCacheManager::new(
    cache_manager,  // 来自 cmx-buffer
    lock_manager,   // 来自 cmx-buffer
));

let deployment_coordinator = Arc::new(
    DeploymentCoordinator::with_lock_manager(lock_manager)  // 注入分布式锁
);

let activation_manager = Arc::new(ActivationManager::new());

// 创建数据库服务
let db_service = Arc::new(CmxPluginDatabase::new(db_manager));

// 创建节点管理器
let node_manager = Arc::new(NodeManager::new(NodeManagerConfig {
    heartbeat_timeout_seconds: 30,
    health_check_interval_seconds: 10,
    selection_strategy: cmx_plugin::NodeSelectionStrategy::RoundRobin,
}));

// 创建消息队列
let message_queue = Arc::new(MessageQueueBuilder::new()
    .enabled(true)
    .redis_url("redis://localhost:6379")
    .build());

// 创建服务注册表
let service_registry = Arc::new(ServiceRegistry::new());

// 创建权限检查器
let permission_checker = Arc::new(PermissionChecker::new(PermissionPolicy::Strict));

// 创建完整的插件管理器
let plugin_manager = PluginManager::with_components(
    config,
    Some(activation_manager),
    Some(deployment_coordinator),
    Some(cache_manager),
    Some(db_service),
)?;
```

### 3.2 节点注册

在分布式环境下，需要注册当前节点：

```rust
// 注册当前节点
async fn register_node(
    node_manager: &NodeManager,
    coordinator: &DeploymentCoordinator
) -> Result<(), Box<dyn std::error::Error>> {
    // 注册到节点管理器
    let node = cmx_plugin::NodeInfo::new("node-001", "192.168.1.100", 8080)
        .with_name("主节点")
        .with_type(cmx_plugin::NodeType::Master);
    node_manager.register(node.clone()).await?;
    
    // 注册到部署协调器
    let deploy_node = cmx_plugin::DeploymentNodeInfo {
        node_id: "node-001".to_string(),
        node_name: "主节点".to_string(),
        host: "192.168.1.100".to_string(),
        port: 8080,
        status: cmx_plugin::DeploymentNodeStatus::Online,
    };
    coordinator.register_node(deploy_node).await?;
    
    println!("节点注册成功!");
    Ok(())
}
```

### 3.3 消息队列订阅

```rust
// 启动消息队列订阅
async fn start_message_queue(
    mq: &MessageQueue,
    cache_manager: &cmx_buffer::CacheManager
) -> Result<(), Box<dyn std::error::Error>> {
    // 连接到 Redis
    let mut mq = mq.clone();
    mq.connect(cache_manager).await?;
    
    // 注册事件处理器
    mq.register_handler("cmx:plugin:events", Box::new(|msg| {
        if let cmx_plugin::Message::PluginEvent(event) = msg {
            log::info!("收到插件事件: {:?} - {}", event.event_type, event.plugin_id);
        }
    })).await;
    
    mq.register_handler("cmx:plugin:deployment", Box::new(|msg| {
        if let cmx_plugin::Message::DeploymentEvent(event) = msg {
            log::info!("收到部署事件: {:?} - {}", event.event_type, event.operation_id);
        }
    })).await;
    
    // 启动订阅
    mq.start_subscriber().await?;
    
    // 在后台运行消息循环
    tokio::spawn(async move {
        mq.run().await;
    });
    
    Ok(())
}
```

---

## 4. 完整使用示例

### 4.1 应用启动流程

```rust
use cmx_plugin::*;
use std::path::PathBuf;
use std::sync::Arc;

pub struct PluginSystem {
    pub manager: Arc<PluginManager>,
    pub coordinator: Arc<DeploymentCoordinator>,
    pub node_manager: Arc<NodeManager>,
    pub message_queue: Arc<MessageQueue>,
    pub service_registry: Arc<ServiceRegistry>,
}

impl PluginSystem {
    /// 初始化插件系统
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("[1/8] 初始化基础设施...");
        
        // 初始化基础设施 (Redis + Database)
        let (cache_mgr, lock_mgr, db_mgr) = init_infrastructure().await?;
        
        println!("[2/8] 创建数据库服务...");
        let db_service = Arc::new(CmxPluginDatabase::new(db_mgr));
        
        println!("[3/8] 创建核心组件...");
        
        // 创建配置
        let config = PluginManagerConfig {
            install_root: PathBuf::from("/data/cmx/plugins"),
            temp_root: PathBuf::from("/tmp/cmx-plugin"),
            backup_root: PathBuf::from("/data/cmx/backups"),
            default_db_id: "default".to_string(),
            enable_backup: true,
            max_backup_count: 5,
            require_signature: false,
            registry_url: None,
        };
        
        // 创建各个组件
        let audit_logger = Arc::new(AuditLogger::new());
        let security_validator = Arc::new(SecurityValidator::default());
        let cache_manager = Arc::new(PluginCacheManager::new(cache_mgr.clone(), lock_mgr.clone()));
        
        let coordinator = Arc::new(
            DeploymentCoordinator::with_lock_manager(lock_mgr.clone())
        );
        
        let activation_manager = Arc::new(ActivationManager::new());
        
        let node_manager = Arc::new(NodeManager::new(NodeManagerConfig::default()));
        
        let message_queue = Arc::new(MessageQueueBuilder::new()
            .enabled(true)
            .redis_url("redis://localhost:6379")
            .build());
        
        let service_registry = Arc::new(ServiceRegistry::new());
        
        println!("[4/8] 创建插件管理器...");
        
        // 创建插件管理器
        let manager = Arc::new(PluginManager::with_components(
            config,
            Some(activation_manager),
            Some(coordinator.clone()),
            Some(cache_manager),
            Some(db_service),
        )?);
        
        println!("[5/8] 注册节点...");
        
        // 注册节点
        let node = NodeInfo::new("node-001", "192.168.1.100", 8080)
            .with_name("主节点")
            .with_type(NodeType::Master);
        node_manager.register(node).await?;
        
        coordinator.register_node(DeploymentNodeInfo {
            node_id: "node-001".to_string(),
            node_name: "主节点".to_string(),
            host: "192.168.1.100".to_string(),
            port: 8080,
            status: DeploymentNodeStatus::Online,
        }).await?;
        
        println!("[6/8] 启动消息队列...");
        
        // 启动消息队列
        let mut mq = (*message_queue).clone();
        mq.connect(&cache_mgr).await?;
        mq.start_subscriber().await?;
        
        println!("[7/8] 初始化系统插件...");
        
        // 从 TOML 配置初始化系统插件（可选）
        // manager.init_system_plugins_from_config(config).await?;
        
        println!("[8/8] 启动心跳任务...");
        
        // 启动心跳任务
        let nm = node_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                nm.heartbeat("node-001").await.ok();
            }
        });
        
        println!("✅ 插件系统初始化完成!");
        
        Ok(Self { 
            manager, 
            coordinator,
            node_manager,
            message_queue,
            service_registry,
        })
    }
}
```

### 4.2 完整的插件操作流程

```rust
impl PluginSystem {
    /// 安装插件 - 完整流程
    pub async fn install_plugin(&self, plugin_zip: &str) -> Result<InstallResponse, PluginError> {
        let request = InstallRequest {
            plugin_id: None,
            source: PluginSource::Zip {
                path: plugin_zip.to_string(),
            },
            target_db_id: Some("default".to_string()),
            target_db_type: None,
            target_nodes: Some(vec!["node-001".to_string()]),
            config: None,
            force: false,
            skip_validation: false,
            operator: "system".to_string(),
        };
        
        let response = self.manager.install(request).await?;
        
        // 发布安装事件
        let event = PluginEvent::new(
            PluginEventType::Installed,
            &response.plugin_id,
            &response.version
        );
        self.message_queue.publish_plugin_event(event).await.ok();
        
        println!("✅ 插件安装成功: {} v{}", response.plugin_id, response.version);
        Ok(response)
    }
    
    /// 激活插件 - 完整流程
    pub async fn activate_plugin(&self, plugin_id: &str) -> Result<ActivateResponse, PluginError> {
        let request = ActivateRequest {
            plugin_id: plugin_id.to_string(),
            config: None,
            operator: "system".to_string(),
        };
        
        let response = self.manager.activate(request).await?;
        
        // 发布激活事件
        let event = PluginEvent::new(
            PluginEventType::Activated,
            plugin_id,
            &response.version
        );
        self.message_queue.publish_plugin_event(event).await.ok();
        
        println!("✅ 插件激活成功: {}", plugin_id);
        Ok(response)
    }
    
    /// 升级插件 - 完整流程
    pub async fn upgrade_plugin(
        &self, 
        plugin_id: &str, 
        new_zip: &str
    ) -> Result<UpgradeResponse, PluginError> {
        let request = UpgradeRequest {
            plugin_id: plugin_id.to_string(),
            source: PluginSource::Zip {
                path: new_zip.to_string(),
            },
            strategy: None,
            force: false,
            operator: "system".to_string(),
        };
        
        let response = self.manager.upgrade(request).await?;
        
        // 发布升级事件
        let event = PluginEvent::new(
            PluginEventType::Upgraded,
            plugin_id,
            &response.to_version
        );
        self.message_queue.publish_plugin_event(event).await.ok();
        
        println!("✅ 插件升级成功: {} -> v{}", plugin_id, response.to_version);
        Ok(response)
    }
    
    /// 回滚插件 - 完整流程
    pub async fn rollback_plugin(
        &self, 
        plugin_id: &str, 
        target_version: &str
    ) -> Result<RollbackResponse, PluginError> {
        let request = RollbackRequest {
            plugin_id: plugin_id.to_string(),
            target_version: target_version.to_string(),
            force: false,
            operator: "system".to_string(),
        };
        
        let response = self.manager.rollback(request).await?;
        
        // 发布回滚事件
        let event = PluginEvent::new(
            PluginEventType::RolledBack,
            plugin_id,
            target_version
        );
        self.message_queue.publish_plugin_event(event).await.ok();
        
        println!("✅ 插件回滚成功: {} -> v{}", plugin_id, target_version);
        Ok(response)
    }
}
```

---

## 5. 插件生命周期流程

```
                    ┌─────────────┐
                    │   开始      │
                    └──────┬──────┘
                           │
                           ▼
              ┌────────────────────────────┐
              │   1. 安装 (Install)        │
              │  - 获取插件源               │
              │  - 安全验证                 │
              │  - 解析依赖                 │
              │  - 复制文件                 │
              │  - 创建数据库表              │
              │  - 保存到数据库              │
              │  - 更新缓存                  │
              │  - 发布事件                  │
              │  - 记录日志                  │
              └──────┬─────────────────────┘
                     │
                     ▼
              ┌────────────────────────────┐
              │   2. 激活 (Activate)       │
              │  - 加载 WASM               │
              │  - 初始化运行时             │
              │  - 注册服务                 │
              │  - 更新状态                 │
              │  - 发布事件                 │
              │  - 记录日志                 │
              └──────┬─────────────────────┘
                     │
                     ▼
              ┌────────────────────────────┐
              │   3. 运行 (Running)        │
              │  - 插件正常工作             │
              │  - 心跳检测                 │
              │  - 健康检查                 │
              └──────┬─────────────────────┘
                     │
           ┌─────────┼─────────┐
           │         │         │
           ▼         ▼         ▼
      ┌────────┐ ┌────────┐ ┌────────┐
      │ 升级   │ │ 停用   │ │ 卸载   │
      │Upgrade  │ │Deactive│ │Uninstall│
      └────┬───┘ └───┬────┘ └───┬────┘
           │         │         │
           ▼         │         │
      ┌─────────┐   │         │
      │ 回滚    │   │         │
      │ Rollback│   │         │
      └─────────┘   │         │
                    │         │
                    ▼         ▼
              ┌────────────────────────────┐
              │   结束                    │
              └────────────────────────────┘
```

---

## 6. 常见使用场景

### 6.1 单节点部署

```rust
// 简单场景：单节点，不需要分布式锁
let manager = PluginManager::new(config)?;

// 安装并激活
manager.install(request).await?;
manager.activate(activate_request).await?;
```

### 6.2 多节点部署

```rust
// 多节点场景：需要分布式锁
let coordinator = DeploymentCoordinator::with_lock_manager(lock_manager);

// 注册所有节点
coordinator.register_node(node1).await?;
coordinator.register_node(node2).await?;
coordinator.register_node(node3).await?;

// 串行部署到所有节点
let request = DeployRequest {
    plugin_id: "my-plugin".to_string(),
    version: "1.0.0".to_string(),
    strategy: DeploymentStrategy::Serial { continue_on_error: true },
    nodes: vec!["node1".to_string(), "node2".to_string(), "node3".to_string()],
};

let result = coordinator.deploy(request).await?;
```

### 6.3 滚动更新

```rust
// 滚动更新策略
let strategy = DeploymentStrategy::Rolling {
    batch_size: 2,
    wait_seconds: 30,
};

let request = DeployRequest {
    plugin_id: "my-plugin".to_string(),
    version: "2.0.0".to_string(),
    strategy,
    nodes: all_nodes,
};

let result = coordinator.deploy(request).await?;
```

### 6.4 蓝绿部署

```rust
// 蓝绿部署
let strategy = DeploymentStrategy::BlueGreen {
    switch_at: Some("2024-01-01T00:00:00Z".to_string()),
};

let request = DeployRequest {
    plugin_id: "my-plugin".to_string(),
    version: "2.0.0".to_string(),
    strategy,
    nodes: all_nodes,
};

let result = coordinator.deploy(request).await?;

// 验证新版本
verify_new_version().await?;

// 切换流量
coordinator.switch_to_green().await?;
```

---

## 7. 高级功能

### 7.1 内存缓存使用

```rust
use cmx_plugin::{MemoryCache, MemoryCacheConfig, PluginMemoryCacheManager, CacheKeyBuilder};

// 创建内存缓存管理器
let memory_cache = PluginMemoryCacheManager::new(MemoryCacheConfig {
    max_entries: 10000,
    default_ttl_seconds: 300,
    cleanup_interval_seconds: 60,
    enable_lru: true,
});

// 缓存插件信息
let key = CacheKeyBuilder::plugin_info("my-plugin");
memory_cache.plugin_info().set(&key, cache_value).await;

// 获取缓存
if let Some(value) = memory_cache.plugin_info().get(&key).await {
    println!("缓存命中: {:?}", value);
}

// 获取统计信息
let stats = memory_cache.total_stats().await;
println!("命中率: {:.2}%", stats.hit_rate() * 100.0);

// 定期清理过期缓存
tokio::spawn(async move {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        let removed = memory_cache.cleanup_all().await;
        log::debug!("清理过期缓存: {} 条", removed);
    }
});
```

### 7.2 服务注册与发现

```rust
use cmx_plugin::{ServiceRegistry, ServiceDescriptor, ServiceInstance, ServiceCallRequest};

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
for inst in instances {
    println!("服务实例: {} - {}", inst.instance_id, inst.endpoint);
}

// 调用服务
let request = ServiceCallRequest {
    service_id: "data-processor".to_string(),
    method: "POST".to_string(),
    path: "/api/process".to_string(),
    headers: HashMap::new(),
    body: Some(r#"{"data": "test"}"#.to_string()),
};
let response = registry.call_service(request).await?;
```

### 7.3 权限控制

```rust
use cmx_plugin::{PermissionChecker, Permission, PermissionType, PermissionPolicy};

// 创建权限检查器
let checker = PermissionChecker::new(PermissionPolicy::Strict);

// 定义插件权限
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

// 检查文件系统权限
let result = checker.check_permission("my-plugin", &permissions, &PermissionType::FileSystem {
    path: "/data/plugins/file.txt".to_string(),
    operation: "read".to_string(),
}).await?;

if result.allowed {
    println!("权限检查通过");
} else {
    println!("权限拒绝: {:?}", result.reason);
}

// 批量检查权限
let checks = vec![
    PermissionType::FileSystem { path: "/data/file.txt".to_string(), operation: "read".to_string() },
    PermissionType::Network { host: "api.example.com".to_string(), port: 443 },
];
let results = checker.check_permissions("my-plugin", &permissions, &checks).await?;
```

### 7.4 版本约束解析

```rust
use cmx_plugin::{VersionConstraint, VersionConstraintParser, SemanticVersion};

// 解析各种版本约束
let caret = VersionConstraintParser::parse("^1.2.3")?;  // >=1.2.3, <2.0.0
let tilde = VersionConstraintParser::parse("~1.2.3")?;  // >=1.2.3, <1.3.0
let range = VersionConstraintParser::parse(">=1.0.0, <2.0.0")?;
let or_constraint = VersionConstraintParser::parse("^1.0.0 || ^2.0.0")?;

// 检查版本是否满足约束
let version = SemanticVersion::parse("1.5.0")?;
if VersionConstraintParser::satisfies(&version, &caret)? {
    println!("版本满足约束");
}

// 从多个版本中找最佳匹配
let versions = vec![
    SemanticVersion::parse("1.0.0")?,
    SemanticVersion::parse("1.2.0")?,
    SemanticVersion::parse("1.5.0")?,
    SemanticVersion::parse("2.0.0")?,
];
let best = VersionConstraintParser::find_best_version(&caret, &versions)?;
println!("最佳版本: {}", best);
```

### 7.5 TOML 配置初始化

```rust
use cmx_plugin::SystemPluginsConfig;

// TOML 配置文件
let toml_content = r#"
[settings]
auto_activate = true
default_db_id = "default"
install_timeout_seconds = 300

[required.core-plugin]
version = "^1.0.0"
source = { type = "registry", registry = "default" }

[required.auth-plugin]
version = "~2.0.0"
source = { type = "url", url = "https://plugins.example.com/auth.zip" }
config = { timeout = 30, retries = 3 }

[optional.analytics-plugin]
version = ">=1.0.0"
source = { type = "directory", path = "/local/plugins/analytics" }
"#;

// 解析配置
let config: SystemPluginsConfig = toml::from_str(toml_content)?;

// 初始化系统插件
manager.init_system_plugins_from_config(config).await?;
```

---

## 8. 错误处理

```rust
match result {
    Ok(response) => {
        match response.success {
            true => println!("操作成功!"),
            false => println!("操作部分成功: {:?}", response.nodes),
        }
    }
    Err(PluginError::NotFound(msg)) => {
        println!("资源不存在: {}", msg);
    }
    Err(PluginError::Dependency(msg)) => {
        println!("依赖冲突: {}", msg);
        // 可能需要先安装依赖
    }
    Err(PluginError::Security(msg)) => {
        println!("安全验证失败: {}", msg);
        // 需要检查插件签名
    }
    Err(PluginError::Permission(msg)) => {
        println!("权限不足: {}", msg);
        // 需要检查权限配置
    }
    Err(PluginError::Node(msg)) => {
        println!("节点错误: {}", msg);
        // 需要检查节点状态
    }
    Err(PluginError::Service(msg)) => {
        println!("服务错误: {}", msg);
        // 需要检查服务注册
    }
    Err(e) => {
        println!("未知错误: {:?}", e);
    }
}
```

---

## 9. 配置参考

### 9.1 插件管理器配置

```rust
let config = PluginManagerConfig {
    install_root: PathBuf::from("/data/cmx/plugins"),
    temp_root: PathBuf::from("/tmp/cmx-plugin"),
    backup_root: PathBuf::from("/data/cmx/backups"),
    default_db_id: "default".to_string(),
    enable_backup: true,
    max_backup_count: 5,
    require_signature: false,
    registry_url: None,
};
```

### 9.2 节点管理器配置

```rust
let node_config = NodeManagerConfig {
    heartbeat_timeout_seconds: 30,
    health_check_interval_seconds: 10,
    selection_strategy: NodeSelectionStrategy::RoundRobin,
};
```

### 9.3 内存缓存配置

```rust
let cache_config = MemoryCacheConfig {
    max_entries: 10000,
    default_ttl_seconds: 300,
    cleanup_interval_seconds: 60,
    enable_lru: true,
};
```

---

## 10. 最佳实践

1. **Always 初始化完整组件** - 生产环境建议使用 `with_components` 初始化所有组件
2. **启用审计日志** - 记录所有操作便于问题排查
3. **配置备份** - 启用备份以便出现问题时回滚
4. **使用分布式锁** - 多节点部署时务必使用分布式锁
5. **验证插件** - 生产环境建议启用签名验证
6. **权限控制** - 生产环境启用权限检查
7. **节点心跳** - 定期发送心跳保持节点在线
8. **事件通知** - 使用消息队列进行跨节点事件通知
9. **内存缓存** - 高频数据使用内存缓存减少 Redis 压力
10. **错误处理** - 做好错误处理和重试逻辑

---

如有更多问题，请参考 [API 文档](./README.md)。
