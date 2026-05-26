# cmx-plugin

> 插件注册表、ZIP 加载、签名验证、生命周期管理模块。

## 项目简介

cmx-plugin 是 cmx-container 项目的插件管理层，提供插件的安装、卸载、激活、升级、降级、回滚等生命周期管理功能，以及集群部署、安全验证、审计日志等能力。

核心设计理念：
- **分层架构**：持久化层（PluginPersistence）→ 运行时层（RuntimeOps）→ 事件发布层（EventPublisher），由编排器（PluginOperationExecutor）统一协调
- **单一写入原则**：数据库操作仅由接收 API 请求的节点执行，其他节点仅做运行时同步
- **幂等操作**：所有运行时同步操作天然幂等，无需分布式锁或请求去重

## 快速开始

### 安装

```toml
[dependencies]
cmx-plugin = { workspace = true }
```

### 核心示例

```rust
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};

async fn init() {
    // 使用默认配置初始化
    GlobalPluginManager::initialize(Default::default()).await.unwrap();

    // 获取全局实例
    let manager = GlobalPluginManager::get();

    // 部署插件（自动判断安装/升级/覆盖安装）
    let response = manager.deploy(deploy_request).await.unwrap();
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 插件注册表 | 基于 Registry + Contexts 的内存级插件信息管理 |
| ZIP 加载 | 从 ZIP 包加载插件，支持 Local/Remote/Marketplace 三种来源 |
| 签名验证 | Ed25519 签名安全验证 |
| 生命周期管理 | 安装、卸载、升级、降级、覆盖安装（智能部署） |
| 集群同步 | 基于 Redis Pub/Sub 的跨实例通知 + 定时对账任务 |
| 对账补偿 | DB vs Registry vs 本地文件三层状态对比，自动补偿差异 |
| 审计日志 | 操作审计记录，支持按插件/操作/时间范围过滤 |
| 插件市场 | 插件发布、搜索、下载、评分统计 |

## 模块结构

```
cmx-plugin
├── core/                   # 核心模块
│   ├── manager.rs          # 插件管理器（核心协调器）
│   ├── registry.rs         # 插件注册表（内存级）
│   └── context.rs          # 插件上下文
├── domain/                 # 领域模型
│   ├── plugin.rs           # PluginInfo、PluginSource、PluginStatus 等
│   ├── version.rs          # SemanticVersion 语义化版本
│   └── dependency.rs       # 依赖检查模型
├── service/                # 服务层
│   ├── executor.rs         # 插件操作编排器（统一编排持久化→运行时→事件）
│   ├── persistence.rs      # 持久化操作层（仅 DB + 文件系统）
│   ├── runtime_ops.rs      # 运行时操作层（仅内存注册/卸载 + 文件同步）
│   ├── event_publisher.rs  # 统一事件发布器（GlobalEventBus + Redis）
│   ├── install.rs          # 安装服务
│   ├── upgrade.rs          # 升级服务
│   ├── downgrade.rs        # 降级服务
│   ├── uninstall.rs        # 卸载服务
│   ├── deploy.rs           # 部署服务（智能安装/升级/覆盖安装）
│   ├── plugin_sync.rs      # Redis 通知处理器（跨实例运行时同步）
│   ├── reconciliation.rs   # 定时对账任务
│   ├── initializer.rs      # 启动时插件同步
│   ├── auto_install.rs     # 自动安装服务
│   ├── marketplace_publisher.rs # 插件市场发布
│   ├── data_parser.rs      # 数据解析
│   ├── service_parser.rs   # 服务定义解析
│   ├── record_builder.rs   # 数据库记录构建
│   └── utils.rs            # 工具函数（DDL 执行等）
├── cluster/                # 集群模块
│   ├── node.rs             # 节点管理器
│   └── notification.rs     # Redis Pub/Sub 通知器
├── infrastructure/         # 基础设施层
│   ├── database/           # 数据库（PluginRepository、VersionHistory、SchemaManager）
│   ├── cache/              # 多层缓存（Memory + Redis）
│   ├── storage/            # 文件存储 + 备份管理
│   └── messaging/          # 消息
├── security/               # 安全模块
│   ├── validator.rs        # 安全验证器
│   └── signature.rs        # 签名验证器
├── runtime/                # 运行时模块
│   ├── activation.rs       # 激活管理器
│   └── service_registry.rs # 服务注册表
├── config/                 # 配置模块
│   ├── settings.rs         # PluginManagerSettings 等
│   └── loader.rs           # 配置加载器
├── audit/                  # 审计模块
│   ├── logger.rs           # 审计日志记录器
│   └── record.rs           # 审计记录
├── fetcher/                # 获取器模块
│   ├── source.rs           # 插件来源定义
│   ├── local.rs            # 本地获取器
│   ├── remote.rs           # 远程获取器
│   ├── marketplace_fetcher.rs # 市场获取器
│   └── storage.rs          # 存储获取器
├── marketplace/            # 插件市场模块
│   ├── model.rs            # 市场数据模型
│   ├── repository.rs       # 市场数据仓库
│   ├── service.rs          # 市场服务
│   └── stats.rs            # 统计服务
├── common/                 # 通用工具
│   ├── definition.rs       # 插件定义解析
│   ├── dependency.rs       # 依赖检查工具
│   ├── package.rs          # 包处理工具
│   ├── scanner.rs          # 本地插件扫描
│   ├── service.rs          # 服务工具
│   └── source_utils.rs     # 来源构建工具
├── error.rs                # 错误类型定义
├── host_functions.rs       # 插件宿主函数
└── traits_impl.rs          # Trait 实现
```

## 核心类型

### PluginManager

插件管理器，核心协调器，统一管理插件生命周期操作。通过 `GlobalPluginManager` 全局单例访问。

### PluginOperationExecutor

插件操作编排器，统一编排 **持久化 → 运行时 → 审计日志 → 事件发布** 的完整流程。

当前节点接收 API 请求后执行完整流程，发布 GlobalEventBus 事件 + Redis 跨实例通知。

### RuntimeOps

运行时操作层，仅负责内存注册/卸载、缓存更新和文件同步，不涉及任何数据库写操作。核心方法：

| 方法 | 说明 |
|------|------|
| `register_plugin` | 注册插件到 Registry + Contexts + Cache |
| `update_plugin` | 更新插件内存信息（升级/降级后） |
| `unregister_plugin` | 从内存注销插件 |
| `register_from_db` | 从数据库查询并注册（其他节点通知场景） |
| `sync_and_register` | 同步文件并注册（幂等：已注册且版本一致则跳过） |
| `force_resync_and_register` | 强制重新同步并注册（覆盖安装场景） |
| `unregister_and_cleanup` | 注销并清理本地文件 |
| `sync_plugin_files` | 从来源同步插件文件到本地目录（原子性下载策略） |

### PluginPersistence

持久化操作层，只负责数据库操作（DML + DDL）和源文件处理，不涉及内存注册、缓存更新、事件发布。

### EventPublisher

统一事件发布器，封装 GlobalEventBus（进程内事件）和 Redis PluginNotifier（跨实例通知）。

## 使用指南

### 一、全局插件管理器

#### 1.1 初始化

```rust
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式1：使用默认配置初始化
    GlobalPluginManager::initialize(Default::default()).await?;

    // 方式2：使用自定义配置初始化
    let settings = PluginManagerSettings {
        plugin_root: std::path::PathBuf::from("./plugins"),
        reconciliation_interval_secs: 60,
        ..Default::default()
    };
    GlobalPluginManager::initialize(settings).await?;

    // 方式3：注入外部依赖（数据库、缓存、分布式锁、PubSub）
    use cmx_database::DatabaseManager;
    use cmx_buffer::CacheManager;
    use std::sync::Arc;

    let db = Arc::new(DatabaseManager::new(Default::default()));
    let cache = Arc::new(CacheManager::new(Default::default()));
    GlobalPluginManager::initialize_with_deps(
        Default::default(),
        Some(db),
        Some(cache),
        None,
        None,
    ).await?;

    Ok(())
}
```

#### 1.2 获取管理器实例

```rust
use cmx_plugin::GlobalPluginManager;

// 获取静态引用（PluginManager 内部已实现细粒度锁，无需 await）
let manager = GlobalPluginManager::get();

// 获取 Arc 引用（用于异步任务共享所有权）
let arc_manager = GlobalPluginManager::get_arc();

// 作为 PluginQuery trait 对象使用（依赖注入场景）
let query = GlobalPluginManager::get_as_plugin_query();

// 检查是否已初始化
if GlobalPluginManager::is_initialized() {
    println!("插件管理器已就绪");
}
```

### 二、插件部署（智能安装/升级/覆盖安装）

#### 2.1 部署插件

```rust
use cmx_plugin::{GlobalPluginManager, DeployRequest, PluginSource};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 从本地 ZIP 文件部署
    let request = DeployRequest {
        source: PluginSource::Local {
            path: PathBuf::from("/tmp/my-plugin.zip"),
        },
        db_id: None,
        force_reinstall: false,
        build_type: Some("release".to_string()),
        publish_to_marketplace: false,
        app_id: Some("my-app".to_string()),
        send_event: true,
        marketplace_source_id: None,
        marketplace_publish_info: None,
    };

    let response = manager.deploy(request).await?;
    println!("部署结果: action={:?}, version={}", response.action, response.new_version);

    Ok(())
}
```

#### 2.2 从远程 URL 部署

```rust
use cmx_plugin::{GlobalPluginManager, DeployRequest, PluginSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let request = DeployRequest {
        source: PluginSource::Remote {
            url: "https://plugins.example.com/my-plugin-1.0.0.zip".to_string(),
            checksum: Some("sha256:abc123...".to_string()),
        },
        db_id: None,
        force_reinstall: false,
        build_type: None,
        publish_to_marketplace: false,
        app_id: None,
        send_event: true,
        marketplace_source_id: None,
        marketplace_publish_info: None,
    };

    let response = manager.deploy(request).await?;
    println!("部署完成: plugin_id={}, action={:?}", response.plugin_id, response.action);

    Ok(())
}
```

### 三、插件生命周期操作

#### 3.1 安装插件

```rust
use cmx_plugin::{GlobalPluginManager, InstallRequest, PluginSource};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let request = InstallRequest {
        source: PluginSource::Local {
            path: PathBuf::from("/tmp/my-plugin.zip"),
        },
        db_id: None,
        auto_activate: false,
        version_constraint: None,
        build_type: Some("release".to_string()),
        marketplace_source_id: None,
        app_id: Some("my-app".to_string()),
        send_event: true,
    };

    let result = manager.install(request).await?;
    println!("安装成功: plugin_id={}, version={}", result.plugin_id, result.version);

    Ok(())
}
```

#### 3.2 升级插件

```rust
use cmx_plugin::{GlobalPluginManager, UpgradeRequest, PluginSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let request = UpgradeRequest {
        plugin_id: "my-plugin".to_string(),
        source: Some(PluginSource::Remote {
            url: "https://plugins.example.com/my-plugin-2.0.0.zip".to_string(),
            checksum: None,
        }),
        version_constraint: None,
        force: false,
        operator: Some("admin".to_string()),
        build_type: None,
        marketplace_source_id: None,
        app_id: Some("my-app".to_string()),
        send_event: true,
    };

    let result = manager.upgrade(request).await?;
    println!("升级成功: {} -> {}", result.old_version, result.new_version);

    Ok(())
}
```

#### 3.3 降级插件

```rust
use cmx_plugin::{GlobalPluginManager, DowngradeRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let request = DowngradeRequest {
        plugin_id: "my-plugin".to_string(),
        target_version: "1.0.0".to_string(),
        source: None,
        operator: None,
        app_id: Some("my-app".to_string()),
        send_event: true,
    };

    let result = manager.downgrade(request).await?;
    println!("降级成功: {} -> {}", result.old_version, result.new_version);

    Ok(())
}
```

#### 3.4 卸载插件

```rust
use cmx_plugin::{GlobalPluginManager, UninstallRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    let request = UninstallRequest {
        plugin_id: "my-plugin".to_string(),
        force: false,
        operator: "admin".to_string(),
        app_id: Some("my-app".to_string()),
        send_event: true,
    };

    manager.uninstall(request).await?;
    println!("卸载成功");

    Ok(())
}
```

### 四、分布式架构：插件同步与 DDL 执行

#### 4.1 架构概览

cmx-plugin 采用**单一写入原则**的分布式架构：

```
┌─────────────────────────────────────────────────────────────┐
│                    Node A (API 接收节点)                      │
│                                                              │
│  API 请求 → DeployService → Executor                         │
│                              ├─ 1. Persistence (DDL + DML)   │
│                              ├─ 2. RuntimeOps (内存注册)      │
│                              ├─ 3. AuditLogger (审计日志)     │
│                              └─ 4. EventPublisher            │
│                                   ├─ GlobalEventBus (进程内)  │
│                                   └─ Redis: Installed 等     │
└──────────────────────────┬──────────────────────────────────┘
                           │ Redis Pub/Sub
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    Node B (其他节点)                          │
│                                                              │
│  PluginChangeHandler                                         │
│  ├─ 收到 Installed/Upgraded/Removed 通知                     │
│  ├─ RuntimeOps.sync_and_register() (同步文件 + 内存注册)     │
│  └─ EventPublisher.publish_local_event() (进程内事件)        │
│                                                              │
│  ❌ 不操作数据库                                             │
│  ❌ 不执行 DDL                                               │
└─────────────────────────────────────────────────────────────┘
```

#### 4.2 通知分类

Redis 通知分为**持久化变更**和**运行时变更**两类：

**持久化变更**（文件/DB 发生变更，其他实例需全量同步）：
- `Installed`：插件首次安装
- `Upgraded`：插件升级
- `Downgraded`：插件降级
- `Reinstalled`：插件覆盖安装
- `Removed`：插件卸载

**运行时变更**（仅内存状态变更，其他实例只需加载/卸载运行时）：
- `RuntimeLoad`：插件运行时加载
- `RuntimeUnload`：插件运行时卸载

#### 4.3 跨实例同步流程

```rust
// Node A：安装插件
// 1. Executor 执行持久化（DDL + DML）
// 2. Executor 执行本地运行时注册
// 3. EventPublisher 发布 Installed 通知到 Redis

// Node B：收到 Installed 通知
// 1. PluginChangeHandler 跳过自身发出的通知（instance_id 过滤）
// 2. 过滤非本应用的通知（app_id 过滤）
// 3. RuntimeOps.sync_and_register()：从数据库查询插件记录，同步文件并注册到内存
// 4. EventPublisher.publish_local_event()：发布进程内 INSTALLED 事件
```

#### 4.4 对账任务

定时对账任务对比 **DB vs Registry vs 本地文件** 三层状态，自动补偿差异：

```rust
// 对账逻辑：
// 1. 查询 DB 中当前 app_id 下所有已安装插件
// 2. 获取 Registry 中已注册的插件列表
// 3. 补偿 Registry 中缺失的插件 → register_from_db()
// 4. 补偿本地文件缺失的插件 → 先 unregister 再 sync_and_register()
// 5. 清理 Registry 中存在但 DB 中不存在的孤立插件 → unregister_and_cleanup()
```

对账任务仅做运行时同步（下载文件 + 内存注册/卸载），**不操作数据库**。

#### 4.5 启动时同步

程序启动时，`PluginInitializer` 执行以下流程：

1. 从 `cmx_plugin` 表获取期望安装的插件列表
2. 扫描本地文件系统获取已安装的插件版本
3. 对比得出需要执行的操作（安装/版本同步/卸载）
4. 通过 `RuntimeOps` 执行运行时同步（**单写原则**：启动仅做运行时同步，不操作数据库）
5. 加载 contexts 到内存

### 五、插件查询

#### 5.1 查询插件信息

```rust
use cmx_plugin::GlobalPluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalPluginManager::get();

    // 获取单个插件信息（先查 Registry，再查数据库）
    if let Some(info) = manager.get_plugin("my-plugin").await? {
        println!("插件: {} v{}", info.id, info.version);
        println!("状态: {:?}", info.status);
        println!("安装路径: {:?}", info.install_path);
        println!("应用ID: {}", info.app_id);
    }

    // 检查插件是否已安装
    let installed = manager.is_plugin_installed("my-plugin").await?;

    // 列出所有插件
    let all_plugins = manager.list_plugins(&Default::default()).await?;

    Ok(())
}
```

### 六、错误处理

```rust
use cmx_plugin::PluginError;

match result {
    Err(PluginError::NotFound(msg)) => {
        eprintln!("未找到: {}", msg);
    }
    Err(PluginError::InvalidState { plugin_id, current, operation }) => {
        eprintln!("插件 {} 当前状态为 {}，无法执行 {} 操作", plugin_id, current, operation);
    }
    Err(PluginError::MissingDependency { plugin_id, dependency }) => {
        eprintln!("插件 {} 缺少依赖: {}", plugin_id, dependency);
    }
    Err(PluginError::VersionIncompatible { plugin_id, installed, required }) => {
        eprintln!("插件 {} 版本不兼容: 已安装 {}, 要求 {}", plugin_id, installed, required);
    }
    Err(PluginError::SignatureVerification(msg)) => {
        eprintln!("签名验证失败: {}", msg);
    }
    Err(e) => {
        // 检查是否可重试
        if e.is_retryable() {
            eprintln!("可重试错误: {}", e);
        }
        // 检查是否致命
        if e.is_fatal() {
            eprintln!("致命错误: {}", e);
        }
        // 获取错误代码
        eprintln!("错误代码: {}", e.error_code());
    }
}
```

### 七、完整示例

```rust
use cmx_plugin::{
    GlobalPluginManager, PluginManagerSettings,
    DeployRequest, PluginSource,
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化插件管理器
    let settings = PluginManagerSettings {
        plugin_root: PathBuf::from("./plugins"),
        reconciliation_interval_secs: 60,
        ..Default::default()
    };
    GlobalPluginManager::initialize(settings).await?;

    let manager = GlobalPluginManager::get();

    // 2. 部署插件（自动判断安装/升级/覆盖安装）
    let deploy_request = DeployRequest {
        source: PluginSource::Local {
            path: PathBuf::from("/tmp/user-service.zip"),
        },
        db_id: None,
        force_reinstall: false,
        build_type: Some("release".to_string()),
        publish_to_marketplace: false,
        app_id: Some("my-app".to_string()),
        send_event: true,
        marketplace_source_id: None,
        marketplace_publish_info: None,
    };

    let response = manager.deploy(deploy_request).await?;
    println!("部署结果: plugin_id={}, action={:?}", response.plugin_id, response.action);

    // 3. 查询插件状态
    if let Some(info) = manager.get_plugin(&response.plugin_id).await? {
        println!("插件 {} v{} 已安装", info.id, info.version);
    }

    // 4. 关闭插件管理器
    GlobalPluginManager::shutdown().await?;

    Ok(())
}
```

## 常见问题

### Q: 覆盖安装（Reinstall）是原子操作吗？

**A**: 不是。覆盖安装是先卸载再安装的非原子操作，卸载和安装各自有独立事务。若安装失败，插件将处于"已卸载但未安装"的中间状态，由定时对账任务补偿。

### Q: 对账任务的间隔是多少？

**A**: 通过 `PluginManagerSettings.reconciliation_interval_secs` 配置，默认 60 秒，最小 10 秒。设为 0 则禁用对账任务。

### Q: 如何确保多实例间插件状态一致？

**A**: 通过三层机制保证：1) Redis Pub/Sub 实时通知（毫秒级）；2) 定时对账任务补偿差异（秒级）；3) 启动时全量同步（分钟级，仅一次）。所有操作天然幂等，无需分布式锁或请求去重。

### Q: DDL 执行如何保证安全？

**A**: DDL 操作通过 `execute_ddl_with_lock` 使用分布式锁保护，确保同一插件的 DDL 不会并发执行。DDL 在事务内执行，与 DML 操作原子提交。
