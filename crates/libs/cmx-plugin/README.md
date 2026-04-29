# cmx-plugin

> 插件注册表、ZIP 加载、签名验证、生命周期管理模块。

## 项目简介

cmx-plugin 是 cmx-container 项目的插件管理层，提供插件的安装、卸载、激活、升级、降级、回滚等生命周期管理功能，以及集群部署、安全验证、审计日志等能力。

## 快速开始

### 安装

```toml
[dependencies]
cmx-plugin = "0.1.0"
```

### 核心示例

```rust
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};

async fn init() {
    GlobalPluginManager::initialize(Default::default()).await.unwrap();
    let manager = GlobalPluginManager::get();

    manager.install("plugin.zip").await?;
    manager.activate("plugin_id").await?;
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 插件注册表 | 插件信息的存储和查询 |
| ZIP 加载 | 从 ZIP 包加载插件 |
| 签名验证 | 插件签名安全验证 |
| 生命周期管理 | 安装、卸载、激活、停用、升级、降级、回滚 |
| 集群支持 | 节点管理、部署协调、状态同步 |
| 审计日志 | 操作审计记录 |

## 模块结构

```
cmx-plugin
├── src/
│   ├── lib.rs              # 库入口
│   ├── audit/              # 审计模块
│   ├── cluster/            # 集群模块
│   ├── common/             # 通用定义
│   ├── config/             # 配置模块
│   ├── core/               # 核心模块
│   ├── domain/             # 领域模型
│   ├── error.rs            # 错误类型
│   ├── fetcher/            # 获取器模块
│   ├── host_functions.rs
│   ├── infrastructure/     # 基础设施层
│   ├── runtime/            # 运行时模块
│   ├── security/           # 安全模块
│   ├── service/            # 服务层
│   └── traits_impl.rs
└── Cargo.toml
```

## 核心类型

### PluginInfo

插件信息结构体，包含插件 ID、名称、版本、状态等。

### SemanticVersion

语义化版本，支持 `major.minor.patch` 格式。

### PluginStatus

插件状态枚举：
- `Unknown`: 未知
- `Installing`: 安装中
- `Installed`: 已安装
- `Activating`: 激活中
- `Active`: 已激活
- `Deactivating`: 停用中
- `Inactive`: 已停用
- `Uninstalling`: 卸载中
- `UpgradeFailed`: 升级失败
- `RollbackFailed`: 回滚失败

## 使用指南

### 一、全局插件管理器

#### 1.1 初始化

```rust
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = PluginManagerSettings::default();
    GlobalPluginManager::initialize(settings).await?;

    Ok(())
}
```

#### 1.2 获取管理器实例

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();
    println!("Manager: {:?}", manager);

    // 作为 trait 对象使用
    let query = GlobalPluginManager::get_as_plugin_query();

    Ok(())
}
```

### 二、插件安装

#### 2.1 从 ZIP 文件安装

```rust
use cmx_plugin::{GlobalPluginManager, DeployRequest};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let deploy_request = DeployRequest {
        plugin_id: "my-plugin".to_string(),
        version: "1.0.0".to_string(),
        wasm_path: PathBuf::from("/plugins/my-plugin/1.0.0/plugin.wasm"),
        manifest: None,
    };

    let result = manager.install(&deploy_request).await?;
    println!("Installed plugin: {:?}", result);

    Ok(())
}
```

#### 2.2 从远程源安装

```rust
use cmx_plugin::{GlobalPluginManager, PluginSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let source = PluginSource::remote()
        .url("https://plugins.example.com/my-plugin-1.0.0.zip")
        .checksum("sha256:abc123...")
        .build()?;

    let result = manager.install_from_source("my-plugin", "1.0.0", &source).await?;

    Ok(())
}
```

#### 2.3 批量安装

```rust
use cmx_plugin::{GlobalPluginManager, PluginManifest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let manifests = vec![
        PluginManifest { id: "plugin-a".to_string(), version: "1.0.0".to_string() },
        PluginManifest { id: "plugin-b".to_string(), version: "2.0.0".to_string() },
        PluginManifest { id: "plugin-c".to_string(), version: "1.5.0".to_string() },
    ];

    for manifest in manifests {
        manager.install_manifest(&manifest).await?;
    }

    Ok(())
}
```

### 三、插件激活

#### 3.1 激活插件

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    manager.activate("plugin_id").await?;
    println!("Plugin activated successfully");

    Ok(())
}
```

#### 3.2 批量激活

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let plugin_ids = vec!["plugin-a", "plugin-b", "plugin-c"];
    manager.activate_all(&plugin_ids).await?;

    Ok(())
}
```

#### 3.3 停用插件

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    manager.deactivate("plugin_id").await?;
    println!("Plugin deactivated");

    Ok(())
}
```

### 四、插件升级

#### 4.1 升级插件

```rust
use cmx_plugin::{GlobalPluginManager, UpgradeRequest};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let upgrade_request = UpgradeRequest {
        plugin_id: "my-plugin".to_string(),
        new_version: "2.0.0".to_string(),
        wasm_path: PathBuf::from("/plugins/my-plugin/2.0.0/plugin.wasm"),
        manifest: None,
        backup_current: true,
    };

    manager.upgrade(&upgrade_request).await?;
    println!("Plugin upgraded successfully");

    Ok(())
}
```

#### 4.2 自动升级检查

```rust
use cmx_plugin::{GlobalPluginManager, VersionCheckStrategy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let strategy = VersionCheckStrategy::Auto {
        check_interval_seconds: 3600,
        auto_upgrade_patch: true,
        auto_upgrade_minor: false,
        auto_upgrade_major: false,
    };

    manager.set_upgrade_strategy("plugin_id", strategy).await?;

    Ok(())
}
```

### 五、插件回滚

#### 5.1 回滚到上一版本

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    manager.rollback("plugin_id").await?;
    println!("Plugin rolled back successfully");

    Ok(())
}
```

#### 5.2 回滚到指定版本

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    manager.rollback_to_version("plugin_id", "1.0.0").await?;
    println!("Plugin rolled back to 1.0.0");

    Ok(())
}
```

### 六、插件卸载

#### 6.1 卸载插件

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    manager.uninstall("plugin_id").await?;
    println!("Plugin uninstalled");

    Ok(())
}
```

#### 6.2 强制卸载

```rust
use cmx_plugin::{GlobalPluginManager, UninstallOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let options = UninstallOptions {
        force: true,
        remove_data: true,
        remove_configs: true,
    };

    manager.uninstall_with_options("plugin_id", &options).await?;

    Ok(())
}
```

### 七、插件查询

#### 7.1 查询插件信息

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 获取单个插件信息
    if let Some(info) = manager.get_plugin_info("plugin_id").await? {
        println!("Plugin: {} v{}", info.id, info.version);
        println!("Status: {:?}", info.status);
        println!("Installed at: {:?}", info.installed_at);
    }

    Ok(())
}
```

#### 7.2 查询插件列表

```rust
use cmx_plugin::{GlobalPluginManager, PluginStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 获取所有插件
    let all_plugins = manager.list_plugins().await?;

    // 获取活跃插件
    let active_plugins: Vec<_> = manager
        .list_plugins()
        .await?
        .into_iter()
        .filter(|p| p.status == PluginStatus::Active)
        .collect();

    println!("Total plugins: {}", all_plugins.len());
    println!("Active plugins: {}", active_plugins.len());

    Ok(())
}
```

#### 7.3 搜索插件

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 按名称搜索
    let results = manager.search_plugins("user").await?;

    // 按标签搜索
    let tagged = manager.find_by_tag("authentication").await?;

    // 按版本范围搜索
    let version_range = manager.find_by_version_range("1.0.0", "2.0.0").await?;

    Ok(())
}
```

### 八、集群管理

#### 8.1 加入集群节点

```rust
use cmx_plugin::{GlobalPluginManager, NodeInfo};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let node = NodeInfo {
        id: "node-001".to_string(),
        host: "192.168.1.101".to_string(),
        port: 8080,
        labels: vec!["production".to_string()],
    };

    manager.add_node(&node).await?;
    println!("Node added to cluster");

    Ok(())
}
```

#### 8.2 同步插件到节点

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 同步单个插件到所有节点
    manager.sync_plugin_to_cluster("plugin_id").await?;

    // 同步到指定节点
    manager.sync_plugin_to_node("plugin_id", "node-002").await?;

    Ok(())
}
```

#### 8.3 集群状态同步

```rust
use cmx_plugin::{GlobalPluginManager, SyncOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let options = SyncOptions {
        full_sync: false,
        timeout_seconds: 30,
    };

    manager.sync_cluster_state(&options).await?;

    Ok(())
}
```

### 九、安全验证

#### 9.1 签名验证

```rust
use cmx_plugin::{GlobalPluginManager, SignatureVerifier};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let verifier = SignatureVerifier::new()
        .with_public_key("/keys/plugin-public.pem")
        .with_algorithm(SignatureAlgorithm::Ed25519)
        .build()?;

    let is_valid = verifier.verify("plugin_id").await?;
    println!("Signature valid: {}", is_valid);

    Ok(())
}
```

#### 9.2 权限检查

```rust
use cmx_plugin::{GlobalPluginManager, Permission, PermissionContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let context = PermissionContext {
        plugin_id: "plugin_id".to_string(),
        requested_permissions: vec![
            Permission::DatabaseRead,
            Permission::DatabaseWrite,
            Permission::NetworkAccess,
        ],
    };

    let result = manager.check_permissions(&context).await?;

    if result.granted.contains(&Permission::DatabaseWrite) {
        println!("Database write permission granted");
    }

    Ok(())
}
```

### 十、审计日志

#### 10.1 查看审计日志

```rust
use cmx_plugin::{GlobalPluginManager, AuditLogFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let filter = AuditLogFilter {
        plugin_id: Some("plugin_id".to_string()),
        action: None,
        from_time: None,
        to_time: None,
        limit: 100,
    };

    let logs = manager.get_audit_logs(&filter).await?;

    for log in logs {
        println!("[{}] {} - {}: {:?}",
            log.timestamp,
            log.action,
            log.plugin_id,
            log.details
        );
    }

    Ok(())
}
```

#### 10.2 导出审计日志

```rust
use cmx_plugin::{GlobalPluginManager, AuditLogFilter, ExportFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let filter = AuditLogFilter {
        plugin_id: None,
        action: None,
        from_time: Some("2024-01-01T00:00:00Z".parse()?),
        to_time: Some("2024-12-31T23:59:59Z".parse()?),
        limit: 10000,
    };

    manager.export_audit_logs(&filter, ExportFormat::Json, "audit_logs.json").await?;

    Ok(())
}
```

### 十一、生命周期钩子

#### 11.1 注册生命周期监听器

```rust
use cmx_plugin::{GlobalPluginManager, PluginLifecycleListener};
use async_trait::async_trait;

struct MyLifecycleListener;

#[async_trait]
impl PluginLifecycleListener for MyLifecycleListener {
    async fn on_installed(&self, plugin_id: &str, version: &str) {
        println!("Plugin {} v{} installed", plugin_id, version);
    }

    async fn on_activated(&self, plugin_id: &str) {
        println!("Plugin {} activated", plugin_id);
    }

    async fn on_deactivated(&self, plugin_id: &str) {
        println!("Plugin {} deactivated", plugin_id);
    }

    async fn on_upgraded(&self, plugin_id: &str, from: &str, to: &str) {
        println!("Plugin {} upgraded from {} to {}", plugin_id, from, to);
    }

    async fn on_uninstalled(&self, plugin_id: &str) {
        println!("Plugin {} uninstalled", plugin_id);
    }

    async fn on_error(&self, plugin_id: &str, error: &str) {
        eprintln!("Plugin {} error: {}", plugin_id, error);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();
    manager.register_lifecycle_listener(Box::new(MyLifecycleListener)).await?;

    Ok(())
}
```

### 十二、错误处理

```rust
use cmx_plugin::{PluginError, GlobalPluginManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    match manager.activate("nonexistent").await {
        Ok(_) => println!("Activated"),
        Err(e) => {
            match e {
                PluginError::PluginNotFound(id) => {
                    eprintln!("Plugin not found: {}", id);
                }
                PluginError::AlreadyActive(id) => {
                    eprintln!("Plugin already active: {}", id);
                }
                PluginError::DependencyMissing { plugin, missing } => {
                    eprintln!("Plugin {} missing dependency: {}", plugin, missing);
                }
                PluginError::VersionConflict { plugin, expected, actual } => {
                    eprintln!("Version conflict: expected {} but got {}", expected, actual);
                }
                PluginError::SignatureInvalid(id) => {
                    eprintln!("Invalid signature for plugin: {}", id);
                }
                PluginError::InsufficientPermissions => {
                    eprintln!("Insufficient permissions");
                }
                _ => {
                    eprintln!("Unknown error: {}", e);
                }
            }
        }
    }

    Ok(())
}
```

### 十三、完整示例

```rust
use cmx_plugin::{
    GlobalPluginManager, PluginManagerSettings,
    DeployRequest, DeploySource, UpgradeRequest,
    PluginLifecycleListener, AuditLogFilter,
};
use async_trait::async_trait;
use std::path::PathBuf;

struct ProductionListener;

#[async_trait]
impl PluginLifecycleListener for ProductionListener {
    async fn on_installed(&self, plugin_id: &str, version: &str) {
        tracing::info!("Plugin installed: {} v{}", plugin_id, version);
    }

    async fn on_activated(&self, plugin_id: &str) {
        tracing::info!("Plugin activated: {}", plugin_id);
    }

    async fn on_error(&self, plugin_id: &str, error: &str) {
        tracing::error!("Plugin error: {} - {}", plugin_id, error);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化插件管理器
    let settings = PluginManagerSettings::default();
    GlobalPluginManager::initialize(settings).await?;

    let manager = GlobalPluginManager::get();

    // 2. 注册生命周期监听器
    manager.register_lifecycle_listener(Box::new(ProductionListener)).await?;

    // 3. 安装插件
    let deploy_request = DeployRequest {
        plugin_id: "user-service".to_string(),
        version: "1.0.0".to_string(),
        wasm_path: PathBuf::from("/plugins/user-service/1.0.0/plugin.wasm"),
        manifest: None,
    };

    manager.install(&deploy_request).await?;

    // 4. 激活插件
    manager.activate("user-service").await?;

    // 5. 查询插件状态
    if let Some(info) = manager.get_plugin_info("user-service").await? {
        println!("Plugin {} is now: {:?}", info.id, info.status);
    }

    // 6. 升级插件
    let upgrade = UpgradeRequest {
        plugin_id: "user-service".to_string(),
        new_version: "2.0.0".to_string(),
        wasm_path: PathBuf::from("/plugins/user-service/2.0.0/plugin.wasm"),
        manifest: None,
        backup_current: true,
    };

    manager.upgrade(&upgrade).await?;

    // 7. 查看审计日志
    let filter = AuditLogFilter {
        plugin_id: Some("user-service".to_string()),
        action: None,
        from_time: None,
        to_time: None,
        limit: 50,
    };

    let logs = manager.get_audit_logs(&filter).await?;
    println!("Recent audit logs: {} entries", logs.len());

    println!("All operations completed successfully!");
    Ok(())
}
```
