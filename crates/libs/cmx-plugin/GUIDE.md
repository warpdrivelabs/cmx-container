# cmx-plugin 完整使用指南

本文档详细介绍 cmx-plugin 的完整初始化流程和使用步骤。

## 目录

1. [系统架构概览](#1-系统架构概览)
2. [初始化流程](#2-初始化流程)
3. [核心组件初始化](#3-核心组件初始化)
4. [完整使用示例](#4-完整使用示例)
5. [插件生命周期流程](#5-插件生命周期流程)
6. [常见使用场景](#6-常见使用场景)

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
6. 创建 DeploymentCoordinator (带分布式锁)
       │
       ▼
7. 创建 PluginManager
       │
       ▼
8. 注册节点 (可选)
       │
       ▼
9. 系统就绪，可以开始使用
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
    DeploymentCoordinator, ActivationManager, NodeInfo, NodeStatus,
    SecurityValidator, SecurityValidatorConfig, AuditLogger,
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

// 创建完整的插件管理器
let plugin_manager = PluginManager::with_components(
    config,
    Some(activation_manager),
    Some(deployment_coordinator),
    Some(cache_manager),
    Some(my_db_service),  // 实现 PluginDatabase trait
)?;
```

### 3.2 节点注册 (可选但推荐)

在分布式环境下，需要注册当前节点：

```rust
// 注册当前节点
async fn register_node(coordinator: &DeploymentCoordinator) -> Result<(), Box<dyn std::error::Error>> {
    let node = NodeInfo {
        node_id: "node-001".to_string(),
        node_name: "主节点".to_string(),
        host: "192.168.1.100".to_string(),
        port: 8080,
        status: NodeStatus::Online,
    };
    
    coordinator.register_node(node).await?;
    println!("节点注册成功!");
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
}

impl PluginSystem {
    /// 初始化插件系统
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("[1/6] 初始化基础设施...");
        
        // 初始化基础设施 (Redis)
        let (cache_mgr, lock_mgr) = init_redis().await?;
        
        println!("[2/6] 初始化数据库...");
        
        // 初始化数据库
        let db_manager = init_database().await?;
        
        // 创建自定义数据库服务
        let db_service = Arc::new(MyPluginDatabase::new(db_manager.clone()));
        
        println!("[3/6] 创建组件...");
        
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
        
        let cache_manager = Arc::new(PluginCacheManager::new(cache_mgr, lock_mgr));
        
        let coordinator = Arc::new(
            DeploymentCoordinator::with_lock_manager(lock_mgr)
        );
        
        let activation_manager = Arc::new(ActivationManager::new());
        
        println!("[4/6] 创建插件管理器...");
        
        // 创建插件管理器
        let manager = Arc::new(PluginManager::with_components(
            config,
            Some(activation_manager),
            Some(coordinator.clone()),
            Some(cache_manager),
            Some(db_service),
        )?);
        
        println!("[5/6] 注册节点...");
        
        // 注册节点
        coordinator.register_node(NodeInfo {
            node_id: "node-001".to_string(),
            node_name: "主节点".to_string(),
            host: "192.168.1.100".to_string(),
            port: 8080,
            status: NodeStatus::Online,
        }).await?;
        
        println!("[6/6] 初始化系统插件...");
        
        // 初始化系统默认插件 (可选)
        // manager.init_system_plugins(config).await?;
        
        println!("✅ 插件系统初始化完成!");
        
        Ok(Self { manager, coordinator })
    }
}

// Redis 初始化辅助函数
async fn init_redis() -> Result<(
    cmx_buffer::CacheManager,
    cmx_buffer::LockManager,
), Box<dyn std::error::Error>> {
    let redis_config = cmx_buffer::RedisConfig {
        addr: "redis://localhost:6379".to_string(),
        password: None,
        db: Some(0),
        pool_size: 10,
    };
    
    let redis_client = cmx_buffer::RedisClient::new(redis_config).await?;
    
    Ok((
        cmx_buffer::CacheManager::new(redis_client.clone()),
        cmx_buffer::LockManager::new(redis_client),
    ))
}

// 数据库初始化辅助函数
async fn init_database() -> Result<cmx_database::DatabaseManager, Box<dyn std::error::Error>> {
    let db_config = cmx_database::DbConfig {
        db_type: cmx_database::DbType::Postgres,
        db_url: "postgresql://localhost:5432/cmx".to_string(),
        db_id: "default".to_string(),
        default: true,
        pool_config: cmx_database::PoolConfig::default(),
        health_check_interval: 60,
        health_check_timeout: 5,
    };
    
    let db_manager = cmx_database::DatabaseManager::new(vec![db_config]).await?;
    Ok(db_manager)
}
```

### 4.2 完整的插件安装流程

```rust
impl PluginSystem {
    /// 安装插件 - 完整流程
    pub async fn install_plugin(&self, plugin_zip: &str) -> Result<InstallResponse, PluginError> {
        
        // 构建安装请求
        let request = InstallRequest {
            plugin_id: None,  // 从 manifest.json 自动获取
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
        
        // 执行安装
        let response = self.manager.install(request).await?;
        
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
        
        println!("✅ 插件回滚成功: {} -> v{}", plugin_id, target_version);
        
        Ok(response)
    }
    
    /// 卸载插件 - 完整流程
    pub async fn uninstall_plugin(&self, plugin_id: &str) -> Result<UninstallResponse, PluginError> {
        
        let request = UninstallRequest {
            plugin_id: plugin_id.to_string(),
            force: false,
            operator: "system".to_string(),
        };
        
        let response = self.manager.uninstall(request).await?;
        
        println!("✅ 插件卸载成功: {}", plugin_id);
        
        Ok(response)
    }
}
```

### 4.3 实际使用示例

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    // 1. 初始化插件系统
    let system = PluginSystem::new().await?;
    
    // 2. 安装插件 (从 ZIP 文件)
    let install_response = system.install_plugin("/path/to/my-plugin.zip").await?;
    println!("安装响应: {:?}", install_response);
    
    // 3. 激活插件
    let activate_response = system.activate_plugin(&install_response.plugin_id).await?;
    println!("激活响应: {:?}", activate_response);
    
    // 4. 列出所有已安装插件
    let plugins = system.manager.list_plugins(PluginFilter::default()).await?;
    println!("已安装插件: {:?}", plugins);
    
    // 5. 升级插件
    let upgrade_response = system.upgrade_plugin(
        &install_response.plugin_id, 
        "/path/to/my-plugin-v2.zip"
    ).await?;
    println!("升级响应: {:?}", upgrade_response);
    
    // 6. 如果升级失败，回滚
    if !upgrade_response.success {
        system.rollback_plugin(
            &install_response.plugin_id,
            &install_response.version
        ).await?;
    }
    
    // 7. 卸载插件
    let uninstall_response = system.uninstall_plugin(&install_response.plugin_id).await?;
    println!("卸载响应: {:?}", uninstall_response);
    
    Ok(())
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
              │  - 记录日志                  │
              └──────┬─────────────────────┘
                     │
                     ▼
              ┌────────────────────────────┐
              │   2. 激活 (Activate)       │
              │  - 加载 WASM               │
              │  - 初始化运行时             │
              │  - 更新状态                 │
              │  - 记录日志                 │
              └──────┬─────────────────────┘
                     │
                     ▼
              ┌────────────────────────────┐
              │   3. 运行 (Running)        │
              │  - 插件正常工作             │
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

## 7. 错误处理

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
    Err(PluginError::Install(msg)) => {
        println!("安装失败: {}", msg);
        // 可能需要清理残留文件
    }
    Err(PluginError::Activate(msg)) => {
        println!("激活失败: {}", msg);
        // 可能需要检查 WASM 运行时
    }
    Err(e) => {
        println!("未知错误: {:?}", e);
    }
}
```

---

## 8. 配置参考

### 8.1 插件管理器配置

```rust
let config = PluginManagerConfig {
    // 插件安装目录
    install_root: PathBuf::from("/data/cmx/plugins"),
    
    // 临时文件目录
    temp_root: PathBuf::from("/tmp/cmx-plugin"),
    
    // 备份目录
    backup_root: PathBuf::from("/data/cmx/backups"),
    
    // 默认数据库 ID
    default_db_id: "default".to_string(),
    
    // 是否启用备份
    enable_backup: true,
    
    // 最大备份数量
    max_backup_count: 5,
    
    // 是否要求签名验证
    require_signature: false,
    
    // 插件注册表 URL
    registry_url: None,
};
```

### 8.2 安全验证配置

```rust
let security_config = SecurityValidatorConfig {
    // 是否要求签名
    require_signature: false,
    
    // 受信任的公钥
    trusted_public_keys: vec![],
    
    // 是否验证文件哈希
    verify_file_hash: true,
    
    // 最大插件大小 (100MB)
    max_plugin_size: 100 * 1024 * 1024,
    
    // 是否启用沙箱
    enable_sandbox: true,
    
    // 允许的导入函数
    allowed_imports: vec!["env".to_string()],
};
```

---

## 9. 最佳实践

1. **Always 初始化完整组件** - 生产环境建议使用 `with_components` 初始化所有组件
2. **启用审计日志** - 记录所有操作便于问题排查
3. **配置备份** - 启用备份以便出现问题时回滚
4. **使用分布式锁** - 多节点部署时务必使用分布式锁
5. **验证插件** - 生产环境建议启用签名验证
6. **错误处理** - 做好错误处理和重试逻辑

---

如有更多问题，请参考 [API 文档](./README.md)。
