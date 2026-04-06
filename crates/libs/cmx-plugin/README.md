# cmx-plugin — 插件管理系统

插件全生命周期管理，包括安装、激活、升级、回滚等操作。

## 目录

- [模块概述](#模块概述)
- [设计思想](#设计思想)
- [代码结构](#代码结构)
- [核心类型](#核心类型)
- [使用指南](#使用指南)
- [生命周期服务](#生命周期服务)
- [宿主函数](#宿主函数)
- [依赖约束](#依赖约束)

---

## 模块概述

`cmx-plugin` 是 CMX 插件系统的核心管理模块，提供：

- **生命周期管理** — 安装、卸载、激活、停用、升级、降级、回滚
- **插件注册表** — 管理已安装插件的状态和上下文
- **集群支持** — 多节点部署协调
- **安全验证** — 插件签名验证
- **服务注册** — 插件提供的服务管理
- **宿主函数** — 插件间调用能力

---

## 设计思想

### 1. 分层架构

```
┌─────────────────────────────────────────────────────────┐
│                    PluginManager                          │
│                    (核心协调器)                            │
├─────────────────────────────────────────────────────────┤
│  service/    │  core/      │  infrastructure/  │ cluster/ │
│  (生命周期服务) │ (注册表/状态机) │ (数据库/缓存/存储)  │ (集群管理) │
├─────────────────────────────────────────────────────────┤
│                    domain/                               │
│                    (领域模型)                             │
└─────────────────────────────────────────────────────────┘
```

### 2. Trait 实现

`PluginManager` 实现了 `cmx-traits::PluginQuery` trait：

```rust
#[async_trait]
impl PluginQuery for PluginManager {
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError>;
    async fn is_active(&self, plugin_id: &str) -> Result<bool, TraitError>;
    async fn get_wasm_path(&self, plugin_id: &str) -> Result<PathBuf, TraitError>;
    async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, TraitError>;
    async fn list_plugins(&self, filter: &PluginFilter) -> Result<Vec<PluginSnapshot>, TraitError>;
}
```

### 3. 全局单例模式

```rust
pub struct GlobalPluginManager;

impl GlobalPluginManager {
    pub async fn initialize(settings: PluginManagerSettings) -> PluginResult<()>;
    pub async fn get() -> RwLockReadGuard<'static, PluginManager>;
    pub async fn get_mut() -> RwLockWriteGuard<'static, PluginManager>;
    pub fn get_arc() -> Arc<RwLock<PluginManager>>;
    pub fn is_initialized() -> bool;
}
```

---

## 代码结构

```
crates/libs/cmx-plugin/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口 + GlobalPluginManager
│   ├── core/
│   │   ├── manager.rs      # PluginManager 核心管理器
│   │   ├── registry.rs     # PluginRegistry 插件注册表
│   │   ├── context.rs      # PluginContext 插件上下文
│   │   └── lifecycle.rs     # LifecycleStateMachine 状态机
│   ├── domain/
│   │   ├── plugin.rs       # PluginInfo, PluginStatus, PluginFilter
│   │   └── version.rs      # SemanticVersion 版本管理
│   ├── service/
│   │   ├── install.rs      # 安装服务
│   │   ├── uninstall.rs    # 卸载服务
│   │   ├── activate.rs     # 激活/停用服务
│   │   ├── upgrade.rs      # 升级服务
│   │   ├── downgrade.rs    # 降级服务
│   │   ├── rollback.rs     # 回滚服务
│   │   └── deploy.rs       # 部署服务
│   ├── infrastructure/
│   │   ├── database/       # 数据库仓储
│   │   ├── cache/          # 缓存管理
│   │   ├── storage/        # 文件存储
│   │   └── messaging/      # 事件总线
│   ├── cluster/
│   │   ├── node.rs         # 节点管理
│   │   └── deployment.rs   # 部署协调
│   ├── security/
│   │   └── validator.rs    # 安全验证
│   ├── runtime/
│   │   ├── activation.rs   # 运行时激活管理
│   │   └── service_registry.rs  # 服务注册
│   ├── host_functions.rs   # PluginHostFunctions
│   └── traits_impl.rs     # PluginQuery trait 实现
└── tests/
    └── plugin_test.rs
```

---

## 核心类型

### PluginManager

核心管理器，协调所有子模块：

```rust
pub struct PluginManager {
    settings: PluginManagerSettings,
    registry: Arc<PluginRegistry>,
    repository: Arc<PluginRepository>,
    cache: Arc<LayeredCacheManager>,
    event_bus: Arc<EventBus>,
    activation_manager: Arc<ActivationManager>,
    service_registry: Arc<ServiceRegistry>,
    // ... 更多组件
}
```

### PluginInfo

插件信息：

```rust
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: PluginStatus,
    pub install_path: PathBuf,
    pub plugin_type: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}
```

### PluginStatus

插件状态枚举：

```rust
pub enum PluginStatus {
    Installed,    // 已安装
    Activated,    // 已激活
    Deactivated,  // 已停用
    Upgrading,    // 升级中
    Downgrading,  // 降级中
    RollingBack,  // 回滚中
}
```

---

## 使用指南

### 1. 初始化

```rust
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
use std::path::PathBuf;

let settings = PluginManagerSettings {
    plugin_root: PathBuf::from("./plugins/root"),
    backup_root: PathBuf::from("./plugins/backup"),
    temp_root: PathBuf::from("./plugins/temp"),
    default_database_id: "default".to_string(),
    node_id: "node-001".to_string(),
    ..Default::default()
};

GlobalPluginManager::initialize(settings).await?;
```

### 2. 安装插件

```rust
use cmx_plugin::{InstallRequest, PluginSource};

let request = InstallRequest {
    source: PluginSource::Local {
        path: PathBuf::from("./my-plugin.zip"),
    },
    db_id: None,
    force: false,
    auto_activate: false,
};

let response = GlobalPluginManager::get_mut().await.install(request).await?;
println!("安装成功: {}", response.plugin_id);
```

### 3. 激活插件

```rust
use cmx_plugin::ActivateRequest;

let request = ActivateRequest {
    plugin_id: "my-plugin".to_string(),
    db_id: None,
};

let response = GlobalPluginManager::get_mut().await.activate(request).await?;
```

### 4. 查询插件

```rust
// 通过 GlobalPluginManager
let manager = GlobalPluginManager::get().await;
let plugin = manager.get_plugin("my-plugin").await?;

// 通过 PluginQuery trait
use cmx_traits::PluginQuery;
let snapshot = manager.get_plugin("my-plugin").await?;
```

### 5. 使用构建器

```rust
use cmx_plugin::core::manager::PluginManagerBuilder;

let manager = PluginManagerBuilder::new(settings)
    .with_database(db_manager)
    .with_cache(cache_manager)
    .with_lock_manager(lock_manager)
    .with_pubsub(pubsub)
    .build()
    .await?;
```

---

## 生命周期服务

| 服务 | 功能 |
|------|------|
| `InstallService` | 安装插件 ZIP 包 |
| `UninstallService` | 卸载插件 |
| `ActivateService` | 激活/停用插件 |
| `UpgradeService` | 升级插件版本 |
| `DowngradeService` | 降级插件版本 |
| `RollbackService` | 回滚到之前版本 |
| `DeployService` | 集群部署协调 |

---

## 宿主函数

`PluginHostFunctions` 提供 WASM 插件间调用能力：

```rust
use cmx_plugin::host_functions::PluginHostFunctions;
use cmx_traits::HostFunctionProvider;

// 注册到 WASM 引擎
engine.register_provider(Box::new(PluginHostFunctions::new(runtime)));
```

### 提供的函数

| 函数名 | 说明 |
|--------|------|
| `cmx:plugin/call_service` | 调用另一个插件的服务 |
| `cmx:plugin/get_info` | 获取当前插件信息 |

### 调用示例

```json
{
    "target_plugin_id": "order-plugin",
    "function_name": "calculate_total",
    "input": {"items": [...]}
}
```

---

## 依赖约束

### 允许的依赖

- `cmx-core` — 基础类型
- `cmx-traits` — trait 定义
- `cmx-database` — 数据库操作
- `cmx-buffer` — 缓存和锁
- `cmx-metadata` — 元数据

### 依赖图

```
cmx-plugin
├── cmx-core
├── cmx-traits
│   └── cmx-core
├── cmx-database
│   ├── cmx-core
│   └── cmx-traits
├── cmx-buffer
│   └── cmx-traits
└── cmx-metadata
    └── cmx-core
```

---

## 错误处理

```rust
pub enum PluginError {
    NotFound(String),
    AlreadyExists(String),
    InvalidState(String),
    InstallFailed(String),
    UninstallFailed(String),
    ActivateFailed(String),
    UpgradeFailed(String),
    ValidationFailed(String),
    DatabaseError(String),
    // ...
}
```

---

## 配置选项

```rust
pub struct PluginManagerSettings {
    /// 插件根目录
    pub plugin_root: PathBuf,
    
    /// 备份目录
    pub backup_root: PathBuf,
    
    /// 临时目录
    pub temp_root: PathBuf,
    
    /// 默认数据库 ID
    pub default_database_id: String,
    
    /// 节点 ID（集群模式）
    pub node_id: String,
    
    /// 是否启用集群模式
    pub enable_cluster: bool,
    
    /// 安全验证配置
    pub security: SecuritySettings,
}
```

---

*文档版本: 1.0.0*
*最后更新: 2026-04-02*
