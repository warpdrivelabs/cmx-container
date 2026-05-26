# 分布式插件操作架构重构方案

## 一、摘要

重新规划分布式架构下插件安装、升级、卸载、降级、覆盖安装等操作的架构，遵循**单一写入原则**：只有接收请求的当前节点可以操作数据库和执行 DDL，其他节点通过 Redis 消息仅做**同步更新、下载安装本地插件包、内存注册/卸载**工作，不再次操作数据库。同时通过分层抽象消除代码重复，提升复用性。

***

## 二、现状分析

### 2.1 当前架构问题

#### 问题1：操作服务职责过重，DB操作与内存操作耦合

当前 `InstallService`、`UpgradeService`、`DowngradeService`、`UninstallService` 每个服务内部同时包含：

* 数据库操作（DML/DDL）

* 文件操作（下载、解压、复制、删除）

* 内存注册（Registry、Contexts）

* 缓存更新

* 事件发布（GlobalEventBus + Redis Notifier）

这导致其他节点收到 Redis 通知后，需要通过 `DeployService` 重新执行完整的安装/升级流程（包含 DB 操作），只是通过 `send_event=false` 避免二次通知。**其他节点实际上也在操作数据库**（upsert 是幂等的所以没出错，但违反了"只有当前节点操作数据库"的原则）。

#### 问题2：代码大量重复

| 重复代码                    | 出现位置                                               | 说明                                                                                    |
| ----------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `extract_source_info()` | install.rs, upgrade.rs                             | 完全相同的函数                                                                               |
| `default_true()`        | install.rs, upgrade.rs, uninstall.rs, downgrade.rs | 完全相同的函数                                                                               |
| `XxxServiceDeps` 结构体    | 4个服务文件                                             | 字段高度重叠（repository, cache, registry, contexts, audit\_logger, plugin\_notifier 等）      |
| DDL 分布式锁逻辑              | install.rs, upgrade.rs                             | 几乎相同的 try\_lock 模式                                                                    |
| 事件发布模式                  | 4个服务 + control.rs + deploy.rs                      | 每处都是 `if send_event { notifier + GlobalEventBus }`                                    |
| Registry/Contexts 更新    | 4个服务 + plugin\_sync.rs + runtime\_loader.rs        | `registry.write().await.register/unregister` + `contexts.write().await.insert/remove` |
| `scan_local_plugins()`  | initializer.rs, plugin\_sync.rs                    | 完全相同的逻辑                                                                               |
| `build_plugin_source()` | initializer.rs（pub）, plugin\_sync.rs 引用            | 只定义一次但分散引用                                                                            |

#### 问题3：两条调用路径导致 ControlService 需要重复编排

`ControlService` 对每个操作（install/upgrade/downgrade/uninstall）都重复了相同的模式：

1. 调用底层服务（send\_event=false）
2. 发布 GlobalEventBus 事件
3. 发布 Redis RuntimeLoad/Unload 通知

这本质上是一个"事件发布后置"的编排逻辑，但散布在 5 个方法中。

#### 问题4：PluginChangeHandler 通过 DeployService 间接操作数据库

`handle_plugin_changed()` 调用 `DeployService.deploy()`，而 DeployService 内部调用 `InstallService.install()` 或 `UpgradeService.upgrade()`，这些服务都会执行数据库写入。虽然 upsert 幂等不会出错，但违反了"其他节点不操作数据库"的原则。

### 2.2 当前调用链路

```
API 请求 → PluginManager → [Install|Upgrade|Downgrade|Uninstall]Service
                                    ↓
                        DB操作 + 文件操作 + 内存注册 + 事件发布

Redis 通知 → PluginChangeHandler → DeployService → [Install|Upgrade]Service
                                                    ↓
                                        DB操作(重复!) + 文件操作 + 内存注册

管控路径 → ControlService → [Install|Upgrade|Downgrade|Uninstall]Service(send_event=false)
                                    ↓
                        DB操作 + 文件操作 + 内存注册 + ControlService补发事件
```

***

## 三、新架构设计

### 3.1 核心原则

1. **单一写入原则**：只有接收 API 请求的节点操作数据库和执行 DDL
2. **消息驱动同步**：其他节点仅通过 Redis 消息做本地同步（下载文件 + 内存注册/卸载）
3. **分层解耦**：将每个操作拆分为"持久化层"（DB+DDL+文件）和"运行时层"（内存注册+缓存）
4. **代码复用**：通过共享的 Deps 结构、工具函数和事件发布器消除重复

### 3.2 分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    API 层 (cmx-api)                          │
│  /api/plugin/install | /api/plugin/control/install | ...    │
└───────────────┬─────────────────────┬───────────────────────┘
                │                     │
                ▼                     ▼
┌───────────────────────┐  ┌──────────────────────────────────┐
│   本地操作入口         │  │   管控操作入口                    │
│   PluginManager       │  │   ControlService                 │
│   (直接调用)          │  │   (编排调用)                      │
└───────────┬───────────┘  └──────────────┬───────────────────┘
            │                             │
            ▼                             ▼
┌───────────────────────────────────────────────────────────────┐
│                    操作编排层 (新)                              │
│                                                               │
│  PluginOperationExecutor                                      │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ 1. 执行持久化操作 (DB + DDL + 文件)                      │  │
│  │ 2. 执行运行时操作 (内存注册 + 缓存)                      │  │
│  │ 3. 发布进程内事件 (GlobalEventBus)                       │  │
│  │ 4. 发布跨实例通知 (Redis Notifier)                       │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
            │                             │
            ▼                             ▼
┌──────────────────────┐    ┌──────────────────────────────────┐
│  持久化层 (新)        │    │  运行时层 (新)                    │
│                      │    │                                  │
│  PluginPersistence   │    │  PluginRuntime                   │
│  ┌────────────────┐  │    │  ┌────────────────────────────┐  │
│  │ install_persist│  │    │  │ register_plugin            │  │
│  │ upgrade_persist│  │    │  │ unregister_plugin          │  │
│  │ downgrade_p    │  │    │  │ update_plugin              │  │
│  │ uninstall_p    │  │    │  │ sync_plugin_files          │  │
│  │ reinstall_p    │  │    │  │ cleanup_plugin_files       │  │
│  └────────────────┘  │    │  └────────────────────────────┘  │
│                      │    │                                  │
│  只操作: DB + DDL    │    │  只操作: 内存 + 文件系统          │
│  + 文件下载/解压     │    │  + 缓存                          │
└──────────────────────┘    └──────────────────────────────────┘
            │                             │
            │                             │
            ▼                             ▼
┌───────────────────────────────────────────────────────────────┐
│                    Redis 通知接收方 (改造)                      │
│                                                               │
│  PluginChangeHandler                                          │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ Installed/Upgraded/Downgraded/Reinstalled:              │  │
│  │   → PluginRuntime.sync_plugin_files() + register()     │  │
│  │   → GlobalEventBus.publish()                           │  │
│  │   ❌ 不再调用 DeployService / InstallService            │  │
│  │                                                         │  │
│  │ Removed:                                                │  │
│  │   → PluginRuntime.unregister() + cleanup_files()       │  │
│  │   → GlobalEventBus.publish(UNINSTALLED)                │  │
│  │                                                         │  │
│  │ RuntimeLoad:                                            │  │
│  │   → PluginRuntime.register() (从DB查+下载+注册)        │  │
│  │   → GlobalEventBus.publish(LOADED)                     │  │
│  │                                                         │  │
│  │ RuntimeUnload:                                          │  │
│  │   → PluginRuntime.unregister()                         │  │
│  │   → GlobalEventBus.publish(UNLOADED)                   │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

### 3.3 新架构调用链路

#### 3.3.1 本地操作（API 直接调用）

```
API 请求 → PluginManager → PluginOperationExecutor
                                │
                                ├─ 1. PluginPersistence.install_persist()
                                │     → DB操作 + DDL + 文件下载解压
                                │
                                ├─ 2. PluginRuntime.register_plugin()
                                │     → Registry + Contexts + Cache
                                │
                                ├─ 3. GlobalEventBus.publish(INSTALLED)
                                │
                                └─ 4. Notifier.notify_installed()
                                      → Redis "cmx:plugin:changed"
```

#### 3.3.2 管控操作

```
API 请求 → ControlService → PluginOperationExecutor
                                │
                                ├─ 1. PluginPersistence.xxx_persist()
                                │     → DB操作 + DDL + 文件下载解压
                                │
                                ├─ 2. PluginRuntime.register_plugin()
                                │     → Registry + Contexts + Cache
                                │
                                ├─ 3. GlobalEventBus.publish(XXX)
                                │
                                └─ 4. Notifier.notify_runtime_load()
                                      → Redis "cmx:plugin:changed"
```

#### 3.3.3 其他节点收到 Redis 通知

```
Redis 通知 → PluginChangeHandler
                │
                ├─ Installed/Upgraded/Downgraded:
                │   → PluginRuntime.sync_and_register()
                │     → 从DB查最新状态 + 下载文件 + 注册内存
                │   → GlobalEventBus.publish(XXX)
                │   ❌ 不操作数据库
                │
                ├─ Reinstalled:
                │   → PluginRuntime.force_resync_and_register()
                │     → 强制重新下载 + 注册内存
                │   → GlobalEventBus.publish(REINSTALLED)
                │
                ├─ Removed:
                │   → PluginRuntime.unregister_and_cleanup()
                │     → 清理内存 + 删除本地文件
                │   → GlobalEventBus.publish(UNINSTALLED)
                │
                ├─ RuntimeLoad:
                │   → PluginRuntime.register_from_db()
                │     → 从DB查 + 下载(如需) + 注册内存
                │   → GlobalEventBus.publish(LOADED)
                │
                └─ RuntimeUnload:
                    → PluginRuntime.unregister_only()
                      → 仅清理内存
                    → GlobalEventBus.publish(UNLOADED)
```

***

## 四、具体实现方案

### 4.1 新增共享组件

#### 4.1.1 `EventPublisher` — 统一事件发布

**文件**: `crates/libs/cmx-plugin/src/service/event_publisher.rs`（新建）

```rust
/// 持久化操作的统一结果（承载不同操作的差异化信息）
pub struct PersistResult {
    pub plugin_id: String,
    pub app_id: String,
    pub version: String,
    pub old_version: Option<String>,
    pub install_path: PathBuf,
    pub wasm_path: String,
    pub db_record: Option<crate::infrastructure::database::repository::PluginRecord>,
    pub plugin_def: Option<crate::domain::definition::PluginDefinition>,
}

/// 统一的事件发布器，封装 GlobalEventBus + Redis Notifier 的发布逻辑
pub struct EventPublisher {
    notifier: Option<Arc<PluginNotifier>>,
}

impl EventPublisher {
    /// 发布安装完成事件（进程内 + 跨实例）
    /// 接收 PersistResult 而非多个独立参数，避免参数膨胀
    pub async fn publish_installed(&self, result: &PersistResult) {
        let payload = PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
            .with_install_path(result.install_path.clone())
            .with_wasm_path(PathBuf::from(&result.wasm_path));
        GlobalEventBus::get().publish(plugin_events::INSTALLED, serde_json::to_value(&payload).unwrap()).await;

        if let Some(notifier) = &self.notifier {
            notifier.notify_installed(&result.plugin_id, &result.version, &result.app_id).await;
        }
    }

    /// 发布升级完成事件
    pub async fn publish_upgraded(&self, result: &PersistResult) {
        let payload = PluginLifecyclePayload::new(&result.app_id, &result.plugin_id, &result.version)
            .with_old_version(result.old_version.as_deref().unwrap_or("unknown"))
            .with_install_path(result.install_path.clone())
            .with_wasm_path(PathBuf::from(&result.wasm_path));
        GlobalEventBus::get().publish(plugin_events::UPGRADED, serde_json::to_value(&payload).unwrap()).await;

        if let Some(notifier) = &self.notifier {
            notifier.notify_upgraded(&result.plugin_id, &result.version, &result.app_id).await;
        }
    }

    /// 发布降级完成事件
    pub async fn publish_downgraded(&self, result: &PersistResult) { /* 同理 */ }

    /// 发布卸载完成事件
    pub async fn publish_uninstalled(&self, result: &PersistResult) { /* 同理 */ }

    /// 发布覆盖安装完成事件
    pub async fn publish_reinstalled(&self, result: &PersistResult) { /* 同理 */ }

    /// 仅发布进程内事件（不发送 Redis 通知）
    /// 用于其他节点收到 Redis 通知后，在本地发布 GlobalEventBus 事件
    pub async fn publish_local_event(&self, event: &str, payload: PluginLifecyclePayload) {
        GlobalEventBus::get().publish(event, serde_json::to_value(&payload).unwrap()).await;
    }

    /// 仅发布 Redis 运行时加载通知（管控模式使用）
    pub async fn notify_runtime_load(&self, plugin_id: &str, version: &str, app_id: &str) {
        if let Some(notifier) = &self.notifier {
            notifier.notify_runtime_load(plugin_id, version, app_id).await;
        }
    }

    /// 仅发布 Redis 运行时卸载通知（管控模式使用）
    pub async fn notify_runtime_unload(&self, plugin_id: &str, version: &str, app_id: &str) {
        if let Some(notifier) = &self.notifier {
            notifier.notify_runtime_unload(plugin_id, version, app_id).await;
        }
    }
}
```

消除所有服务中重复的 `if send_event { notifier + GlobalEventBus }` 模式。

#### 4.1.2 `PluginPersistence` — 持久化操作层

**文件**: `crates/libs/cmx-plugin/src/service/persistence.rs`（新建）

```rust
/// 插件持久化操作层
///
/// 只负责数据库操作（DML + DDL）和源文件处理（安装包解压+元数据提取），
/// 不涉及内存注册、缓存更新、事件发布。
///
/// 文件操作边界说明：
/// - PluginPersistence 的文件操作 = "安装包解压 + 元数据提取 + 复制到安装目录"
///   （处理的是源文件 → 安装目录的转换）
/// - PluginRuntime 的文件操作 = "从来源同步 wasm/资源到本地运行目录"
///   （处理的是运行时文件的下载和同步，保留 RuntimeLoader 的原子性下载策略）
pub struct PluginPersistence {
    deps: InstallServiceDeps,  // 复用现有 Deps，不引入新的 SharedPluginDeps
    package_utils: PackageUtils,
    dependency_utils: DependencyUtils,
}

impl PluginPersistence {
    /// 安装持久化：DB写入 + DDL + 源文件解压复制
    ///
    /// 事务边界：DDL 在事务外执行（分布式锁保护），DML 在事务内执行。
    /// 这与当前 InstallService 的行为一致。
    pub async fn install_persist(&self, request: InstallRequest) -> PluginResult<PersistResult> {
        // 1. 获取插件包 + 解压 + 安全验证 + 解析元数据
        // 2. 检查已安装状态（DB查询）
        // 3. 检查依赖
        // 4. 创建安装目录 + 复制文件（源文件处理）
        // 5. DDL（分布式锁保护，事务外执行）
        // 6. 开启事务：upsert_plugin + upsert_version + set_current_version
        // 7. 解析并存储服务定义
        // 8. 提交事务
        // 返回 PersistResult（plugin_id, version, install_path, wasm_path, db_record 等）
    }

    /// 升级持久化
    pub async fn upgrade_persist(&self, request: UpgradeRequest) -> PluginResult<PersistResult> { ... }

    /// 降级持久化
    pub async fn downgrade_persist(&self, request: DowngradeRequest) -> PluginResult<PersistResult> { ... }

    /// 卸载持久化
    pub async fn uninstall_persist(&self, request: UninstallRequest) -> PluginResult<PersistResult> { ... }

    /// 覆盖安装持久化（先卸载持久化再安装持久化）
    ///
    /// ⚠️ 已知风险：此操作不是原子的。如果卸载成功但安装失败，
    /// 数据库状态会处于不一致状态（插件记录已删除但未重新创建）。
    /// 依赖 ReconciliationTask 定时对账进行最终补偿。
    /// 这与当前 deploy.rs execute_reinstall 的行为一致。
    pub async fn reinstall_persist(&self, request: DeployRequest) -> PluginResult<PersistResult> { ... }
}
```

**关键变化**：持久化方法只返回 `PersistResult`，不操作内存、不发布事件。调用方自行决定后续操作。

#### 4.1.3 `PluginRuntime` — 运行时操作层

**文件**: `crates/libs/cmx-plugin/src/service/runtime_ops.rs`（新建）

```rust
/// 插件运行时操作层
///
/// 只负责内存注册/卸载、缓存更新、运行时文件同步，
/// 不涉及数据库操作。
///
/// 文件操作边界说明：
/// - PluginRuntime 的文件操作 = "从来源同步 wasm/资源到本地运行目录"
///   保留 RuntimeLoader 的原子性下载策略：
///   先下载到 .downloading 临时目录，完成后 rename 到正式目录。
///
/// 并发安全：
/// - Registry 和 Contexts 使用 Arc<RwLock> 保护，多通知并发到达时
///   通过写锁互斥保证安全。
/// - register 操作天然幂等（HashMap insert 覆盖旧值）。
/// - unregister 对不存在的 key 无副作用。
///
/// 幂等性保证：
/// - sync_and_register：如果插件已注册且版本一致，跳过重复操作。
/// - register_from_db：同上。
/// - force_resync_and_register：不检查本地路径，强制重新下载（Reinstalled 场景需要）。
pub struct PluginRuntime {
    deps: RuntimeOpsDeps,  // 独立的 Deps，只包含运行时层需要的依赖
    package_utils: PackageUtils,
}

/// 运行时操作层依赖（精简版，不包含安全验证器、备份管理器等持久化层依赖）
#[derive(Clone)]
pub struct RuntimeOpsDeps {
    pub repository: Arc<PluginRepository>,
    pub registry: Arc<RwLock<PluginRegistry>>,
    pub contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
    pub cache: Arc<LayeredCacheManager>,
    pub plugin_root: PathBuf,
    pub temp_root: PathBuf,
    pub app_id: String,
}

impl PluginRuntime {
    // ===== 注册操作 =====

    /// 注册插件到内存（Registry + Contexts + Cache）
    /// 用于当前节点完成持久化后的本地注册
    pub async fn register_plugin(&self, result: &PersistResult) -> PluginResult<()> {
        let plugin_info = PluginInfo { ... };
        self.deps.registry.write().await.register(plugin_info);
        if let Some(record) = &result.db_record {
            let context = PluginContext::from_db_record(record);
            self.deps.contexts.write().await.insert(result.plugin_id.clone(), context);
        }
        self.deps.cache.set(&format!("plugin:{}", result.plugin_id), ...).await;
        Ok(())
    }

    /// 更新插件内存信息（升级/降级后版本变更）
    pub async fn update_plugin(&self, result: &PersistResult) -> PluginResult<()> { ... }

    /// 从数据库查询并注册插件（其他节点收到 RuntimeLoad 通知时使用）
    ///
    /// 幂等性：如果插件已注册且版本一致，跳过。
    pub async fn register_from_db(&self, plugin_id: &str, version: &str) -> PluginResult<()> {
        // 0. 幂等检查：如果已注册且版本一致，跳过
        {
            let registry = self.deps.registry.read().await;
            if let Some(info) = registry.get(plugin_id) {
                if info.version == version {
                    tracing::debug!("插件 {} v{} 已注册，跳过", plugin_id, version);
                    return Ok(());
                }
            }
        }
        // 1. 从DB查询插件记录
        // 2. 检查本地文件是否存在，不存在则 sync_plugin_files（原子性下载策略）
        // 3. 注册到 Registry + Contexts
    }

    /// 同步文件并注册（其他节点收到 Installed/Upgraded/Downgraded 通知时使用）
    ///
    /// 幂等性：如果本地文件已存在且插件已注册，跳过。
    pub async fn sync_and_register(&self, plugin_id: &str, version: &str) -> PluginResult<()> {
        // 1. 从DB查询插件记录
        // 2. 检查本地文件，不存在则下载（原子性下载策略：.downloading + rename）
        // 3. 注册到 Registry + Contexts
    }

    /// 强制重新同步并注册（其他节点收到 Reinstalled 通知时使用）
    ///
    /// 不检查本地路径是否已存在，强制重新下载。
    /// 保留原子性下载策略：先下载到 .downloading 临时目录，完成后 rename。
    pub async fn force_resync_and_register(&self, plugin_id: &str, version: &str) -> PluginResult<()> { ... }

    // ===== 卸载操作 =====

    /// 从内存注销插件（Registry + Contexts + Cache）
    /// 幂等性：对不存在的 key 无副作用。
    pub async fn unregister_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        self.deps.registry.write().await.unregister(plugin_id);
        self.deps.contexts.write().await.remove(plugin_id);
        self.deps.cache.delete(&format!("plugin:{}", plugin_id)).await;
        Ok(())
    }

    /// 注销并清理本地文件（其他节点收到 Removed 通知时使用）
    pub async fn unregister_and_cleanup(&self, plugin_id: &str) -> PluginResult<()> {
        self.unregister_plugin(plugin_id).await?;
        let plugin_dir = self.deps.plugin_root.join(&self.deps.app_id).join(plugin_id);
        if plugin_dir.exists() {
            tokio::fs::remove_dir_all(&plugin_dir).await?;
        }
        Ok(())
    }

    // ===== 文件同步 =====

    /// 从来源同步插件文件到本地（复用 RuntimeLoader 的 sync_plugin_files 逻辑）
    /// 保留原子性下载策略：先下载到 .downloading 临时目录，完成后 rename 到正式目录。
    pub async fn sync_plugin_files(&self, plugin_id: &str, version: &str, source: &PluginSource) -> PluginResult<PathBuf> { ... }
}
```

#### 4.1.4 `PluginOperationExecutor` — 操作编排器

**文件**: `crates/libs/cmx-plugin/src/service/executor.rs`（新建）

```rust
/// 插件操作编排器
///
/// 统一编排持久化 → 运行时 → 事件发布的完整流程。
/// 替代当前散布在各服务中的重复编排逻辑。
///
/// 设计决策：保持独立方法而非策略模式。
/// 原因：不同操作的 Request/Response 类型不同，策略模式会引入
/// 运行时分支和参数膨胀，降低类型安全性。独立方法虽然数量多（9个），
/// 但每个方法签名清晰、类型安全，调用方不需要理解策略参数。
pub struct PluginOperationExecutor {
    persistence: PluginPersistence,
    runtime: PluginRuntime,
    event_publisher: EventPublisher,
    audit_logger: Arc<AuditLogger>,
}

impl PluginOperationExecutor {
    // ===== 本地操作（API直接调用） =====

    /// 安装插件（完整流程：持久化 + 运行时 + 事件）
    pub async fn execute_install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        // 1. 持久化
        let persist_result = self.persistence.install_persist(request).await?;
        // 2. 运行时注册
        self.runtime.register_plugin(&persist_result).await?;
        // 3. 审计日志
        self.audit_logger.log(...).await?;
        // 4. 事件发布（GlobalEventBus + Redis Notifier）
        self.event_publisher.publish_installed(&persist_result).await;
        Ok(InstallResponse { ... })
    }

    /// 升级插件
    pub async fn execute_upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> { ... }

    /// 降级插件
    pub async fn execute_downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> { ... }

    /// 卸载插件
    pub async fn execute_uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> { ... }

    /// 覆盖安装
    pub async fn execute_reinstall(&self, request: DeployRequest) -> PluginResult<DeployResponse> { ... }

    // ===== 管控操作 =====

    /// 管控安装（持久化 + 本地运行时 + 进程内事件 + RuntimeLoad通知）
    pub async fn execute_control_install(&self, request: ControlInstallRequest) -> PluginResult<ControlDeployResponse> {
        // 1. 持久化
        let persist_result = self.persistence.install_persist(request.into()).await?;
        // 2. 本地运行时注册
        self.runtime.register_plugin(&persist_result).await?;
        // 3. 进程内事件
        self.event_publisher.publish_local_event(INSTALLED, payload).await;
        // 4. 跨实例运行时加载通知
        self.event_publisher.notify_runtime_load(&persist_result).await;
        Ok(...)
    }

    /// 管控升级/降级/卸载 同理
    pub async fn execute_control_upgrade(&self, ...) -> PluginResult<...> { ... }
    pub async fn execute_control_downgrade(&self, ...) -> PluginResult<...> { ... }
    pub async fn execute_control_uninstall(&self, ...) -> PluginResult<...> { ... }
}
```

### 4.2 改造现有组件

#### 4.2.1 `PluginChangeHandler` 改造

**文件**: `crates/libs/cmx-plugin/src/service/plugin_sync.rs`

**核心变化**：不再调用 `DeployService`，改为调用 `PluginRuntime` 的方法。

```rust
pub struct PluginChangeHandler {
    repository: Arc<PluginRepository>,
    runtime: Arc<PluginRuntime>,          // 替换 deploy_service
    event_publisher: EventPublisher,       // 新增
    plugin_root: PathBuf,
    app_id: String,
    instance_id: String,
}

impl PluginChangeHandler {
    /// 处理安装/升级/降级通知
    async fn handle_plugin_changed(&self, notification: &PluginChangeNotification) {
        // 1. 从DB查询最新状态
        let db_plugin = self.repository.find_plugin(...)?;
        // 2. 同步文件 + 注册内存（不操作数据库）
        self.runtime.sync_and_register(plugin_id, &db_plugin.version).await?;
        // 3. 发布进程内事件
        let event_name = match notification.action {
            PluginChangeAction::Installed => plugin_events::INSTALLED,
            PluginChangeAction::Upgraded => plugin_events::UPGRADED,
            PluginChangeAction::Downgraded => plugin_events::DOWNGRADED,
            _ => return,
        };
        self.event_publisher.publish_local_event(event_name, payload).await;
    }

    /// 处理覆盖安装通知
    async fn handle_plugin_reinstalled(&self, notification: &PluginChangeNotification) {
        let db_plugin = self.repository.find_plugin(...)?;
        self.runtime.force_resync_and_register(plugin_id, &db_plugin.version).await?;
        self.event_publisher.publish_local_event(plugin_events::REINSTALLED, payload).await;
    }

    /// 处理移除通知
    async fn handle_plugin_removed(&self, notification: &PluginChangeNotification) {
        self.runtime.unregister_and_cleanup(plugin_id).await?;
        self.event_publisher.publish_local_event(plugin_events::UNINSTALLED, payload).await;
    }

    /// 处理运行时加载通知
    async fn handle_runtime_load(&self, notification: &PluginChangeNotification) {
        self.runtime.register_from_db(plugin_id, version).await?;
        self.event_publisher.publish_local_event(plugin_events::LOADED, payload).await;
    }

    /// 处理运行时卸载通知
    async fn handle_runtime_unload(&self, notification: &PluginChangeNotification) {
        self.runtime.unregister_plugin(plugin_id).await?;
        self.event_publisher.publish_local_event(plugin_events::UNLOADED, payload).await;
    }
}
```

#### 4.2.2 `ControlService` 简化

**文件**: `crates/libs/cmx-plugin/src/service/control.rs`

ControlService 不再需要手动编排事件发布，改为调用 `PluginOperationExecutor` 的管控方法：

```rust
pub struct ControlService {
    executor: Arc<PluginOperationExecutor>,
    repository: Arc<PluginRepository>,
    app_id: String,
}

impl ControlService {
    pub async fn install(&self, req: ControlInstallRequest) -> PluginResult<ControlDeployResponse> {
        // 版本一致性校验...
        self.executor.execute_control_install(req).await
    }

    pub async fn upgrade(&self, req: ControlUpgradeRequest) -> PluginResult<ControlDeployResponse> {
        self.check_version_consistency(...)?;
        self.executor.execute_control_upgrade(req).await
    }
    // downgrade, uninstall 同理
}
```

#### 4.2.3 `DeployService` 简化

**文件**: `crates/libs/cmx-plugin/src/service/deploy.rs`

DeployService 不再直接调用 InstallService/UpgradeService/UninstallService，改为调用 `PluginOperationExecutor`：

```rust
pub struct DeployService {
    executor: Arc<PluginOperationExecutor>,
    repository: Arc<PluginRepository>,
    package_utils: PackageUtils,
    security_validator: Arc<SecurityValidator>,
}

impl DeployService {
    pub async fn deploy(&self, request: DeployRequest) -> PluginResult<DeployResponse> {
        // 1. 获取包 + 解析元数据 + 安全验证
        // 2. 查询当前安装状态
        // 3. 根据版本比较分发：
        match version_cmp {
            Greater => self.executor.execute_upgrade(...).await,
            Equal if force_reinstall => self.executor.execute_reinstall(...).await,
            Equal => Ok(AlreadyInstalled),
            Less => Err(...),
            None => self.executor.execute_install(...).await,
        }
    }
}
```

#### 4.2.4 `ReconciliationTask` 改造

**文件**: `crates/libs/cmx-plugin/src/service/reconciliation.rs`

对账任务改为调用 `PluginRuntime` 而非 `RuntimeLoader`：

```rust
pub struct ReconciliationTask {
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    runtime: Arc<PluginRuntime>,       // 替换 runtime_loader
    event_publisher: EventPublisher,    // 新增
    app_id: String,
    interval: Duration,
    plugin_root: PathBuf,
}
```

#### 4.2.5 `PluginInitializer` 改造

**文件**: `crates/libs/cmx-plugin/src/service/initializer.rs`

启动同步改为调用 `PluginRuntime`：

```rust
pub struct PluginInitializer {
    runtime: Arc<PluginRuntime>,
    event_publisher: EventPublisher,
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
    plugin_root: PathBuf,
    app_id: String,
}

impl PluginInitializer {
    /// 启动时同步：只做运行时加载，不操作数据库
    pub async fn sync_plugins(&self) -> PluginResult<PluginSyncResult> {
        // 1. 从DB查询期望插件
        // 2. 扫描本地文件
        // 3. 对比差异
        // 4. 缺失的 → runtime.register_from_db() (下载文件 + 注册内存)
        // 5. 多余的 → runtime.unregister_and_cleanup()
        // ❌ 不再调用 InstallService/UpgradeService/DowngradeService
    }
}
```

### 4.3 共享工具函数

#### 4.3.1 `common/mod.rs` 扩展

将以下重复代码统一到 `common/` 模块：

| 函数                      | 当前位置                            | 迁移到                      |
| ----------------------- | ------------------------------- | ------------------------ |
| `extract_source_info()` | install.rs, upgrade.rs          | `common/source_utils.rs` |
| `default_true()`        | 4个文件                            | `common/mod.rs`          |
| `build_plugin_source()` | initializer.rs                  | `common/source_utils.rs` |
| `scan_local_plugins()`  | initializer.rs, plugin\_sync.rs | `common/scanner.rs`      |

#### 4.3.2 DDL 分布式锁逻辑提取

**文件**: `crates/libs/cmx-plugin/src/service/utils.rs`（已有，扩展）

```rust
/// 执行 DDL 操作（带分布式锁保护）
pub async fn execute_ddl_with_lock(
    lock_manager: &Option<Arc<LockManager>>,
    plugin_id: &str,
    target_db_id: &str,
    app_id: &str,
    version: &str,
    install_path: &Path,
    plugin_def: &PluginDefinition,
    txn_id: Option<&str>,
) -> PluginResult<()> {
    let lock_key = format!("plugin:ddl:{}", plugin_id);
    match lock_manager {
        Some(lm) => match lm.try_lock_with_value(&lock_key).await {
            Ok((true, Some(lock_value))) => {
                tracing::info!("获取DDL锁成功，本实例负责创建表: {}", plugin_id);
                create_plugin_tables(...).await?;
                if let Err(e) = lm.unlock_with_value(&lock_key, &lock_value).await {
                    tracing::debug!("释放DDL锁失败（将等待TTL过期）: {}", e);
                }
            }
            Ok(_) => tracing::info!("其他实例正在创建表，跳过DDL: {}", plugin_id),
            Err(e) => {
                tracing::warn!("锁服务异常: {}，继续创建表", e);
                create_plugin_tables(...).await?;
            }
        },
        None => create_plugin_tables(...).await?,
    }
    Ok(())
}
```

### 4.4 废弃/合并的组件

| 组件                  | 处理方式                                            | 原因        |
| ------------------- | ----------------------------------------------- | --------- |
| `InstallService`    | 保留但瘦身为仅调用 `PluginPersistence` + `PluginRuntime` | 向后兼容，内部委托 |
| `UpgradeService`    | 同上                                              | 同上        |
| `DowngradeService`  | 同上                                              | 同上        |
| `UninstallService`  | 同上                                              | 同上        |
| `RuntimeLoader`     | 合并到 `PluginRuntime`                             | 功能完全重叠    |
| 4个 `XxxServiceDeps` | 替换为 `SharedPluginDeps`                          | 消除字段重复    |

***

## 五、Redis 通知语义调整

### 5.1 当前问题

当前 Redis 通知有 7 种 Action，但语义不够清晰：

| Action          | 当前接收方行为                 | 问题         |
| --------------- | ----------------------- | ---------- |
| `Installed`     | 调用 DeployService（含DB操作） | 其他节点不应操作DB |
| `Upgraded`      | 调用 DeployService（含DB操作） | 同上         |
| `Downgraded`    | 调用 DeployService（含DB操作） | 同上         |
| `Reinstalled`   | 调用 DeployService（含DB操作） | 同上         |
| `Removed`       | 清理内存+文件                 | 正确，无需改     |
| `RuntimeLoad`   | 下载+注册内存                 | 正确，无需改     |
| `RuntimeUnload` | 卸载内存                    | 正确，无需改     |

### 5.2 调整方案

**保持 7 种 Action 不变**，但接收方行为统一改为"只做运行时同步"：

| Action          | 新的接收方行为                                          | 说明               |
| --------------- | ------------------------------------------------ | ---------------- |
| `Installed`     | `runtime.sync_and_register()` + EventBus         | 从DB查+下载+注册，不操作DB |
| `Upgraded`      | `runtime.sync_and_register()` + EventBus         | 同上               |
| `Downgraded`    | `runtime.sync_and_register()` + EventBus         | 同上               |
| `Reinstalled`   | `runtime.force_resync_and_register()` + EventBus | 强制重新下载+注册        |
| `Removed`       | `runtime.unregister_and_cleanup()` + EventBus    | 不变               |
| `RuntimeLoad`   | `runtime.register_from_db()` + EventBus          | 不变               |
| `RuntimeUnload` | `runtime.unregister_plugin()` + EventBus         | 不变               |

***

## 六、文件变更清单

### 6.1 新建文件

| 文件                               | 说明                       |
| -------------------------------- | ------------------------ |
| `src/service/shared_deps.rs`     | 统一依赖结构                   |
| `src/service/event_publisher.rs` | 统一事件发布器                  |
| `src/service/persistence.rs`     | 持久化操作层                   |
| `src/service/runtime_ops.rs`     | 运行时操作层（合并 RuntimeLoader） |
| `src/service/executor.rs`        | 操作编排器                    |
| `src/common/source_utils.rs`     | 来源信息工具函数                 |
| `src/common/scanner.rs`          | 本地插件扫描工具                 |

### 6.2 修改文件

| 文件                              | 变更内容                                                                             |
| ------------------------------- | -------------------------------------------------------------------------------- |
| `src/service/install.rs`        | 瘦身：内部委托 PluginPersistence + PluginRuntime，删除 extract\_source\_info/default\_true |
| `src/service/upgrade.rs`        | 同上                                                                               |
| `src/service/downgrade.rs`      | 同上                                                                               |
| `src/service/uninstall.rs`      | 同上                                                                               |
| `src/service/deploy.rs`         | 改为调用 PluginOperationExecutor                                                     |
| `src/service/control.rs`        | 简化：改为调用 executor 的管控方法                                                           |
| `src/service/plugin_sync.rs`    | 核心改造：不再调用 DeployService，改为调用 PluginRuntime                                       |
| `src/service/reconciliation.rs` | 改为调用 PluginRuntime                                                               |
| `src/service/initializer.rs`    | 改为调用 PluginRuntime，删除 scan\_local\_plugins 重复代码                                  |
| `src/service/runtime_loader.rs` | 合并到 runtime\_ops.rs，本文件废弃                                                        |
| `src/core/manager.rs`           | 适配新组件，更新初始化逻辑                                                                    |
| `src/service/mod.rs`            | 添加新模块声明                                                                          |
| `src/common/mod.rs`             | 添加新模块声明                                                                          |

### 6.3 废弃文件

| 文件                              | 处理方式                          |
| ------------------------------- | ----------------------------- |
| `src/service/runtime_loader.rs` | 逻辑合并到 `runtime_ops.rs`，文件注释保留 |

***

## 七、实施步骤

### 阶段1：基础设施（无破坏性变更）

1. 创建 `shared_deps.rs` — 统一依赖结构
2. 创建 `common/source_utils.rs` — 提取 `extract_source_info` + `build_plugin_source`
3. 创建 `common/scanner.rs` — 提取 `scan_local_plugins`
4. 扩展 `service/utils.rs` — 提取 `execute_ddl_with_lock`
5. 创建 `event_publisher.rs` — 统一事件发布

### 阶段2：核心分层（新组件，不影响现有代码）

1. 创建 `persistence.rs` — 从现有服务提取持久化逻辑
2. 创建 `runtime_ops.rs` — 合并 RuntimeLoader + 新增运行时操作方法
3. 创建 `executor.rs` — 编排持久化+运行时+事件

### 阶段3：切换调用（逐步替换）

1. 改造 `plugin_sync.rs` — 使用 PluginRuntime 替换 DeployService
2. 改造 `control.rs` — 使用 executor 替换手动编排
3. 改造 `deploy.rs` — 使用 executor 替换直接调用
4. 改造 `reconciliation.rs` — 使用 PluginRuntime
5. 改造 `initializer.rs` — 使用 PluginRuntime

### 阶段4：瘦身旧服务（可选，渐进式）

1. 瘦身 `install.rs` — 内部委托 persistence + runtime
2. 瘦身 `upgrade.rs` — 同上
3. 瘦身 `downgrade.rs` — 同上
4. 瘦身 `uninstall.rs` — 同上
5. 废弃 `runtime_loader.rs`

***

## 八、验证方案

1. **编译验证**：每个阶段完成后 `rtk cargo check` 确保编译通过
2. **单元测试**：为 PluginPersistence、PluginRuntime、EventPublisher 编写单元测试
3. **集成测试**：

   * 单节点：安装→升级→降级→覆盖安装→卸载 完整流程

   * 多节点：节点A安装→节点B收到通知→验证B的内存注册和文件同步

   * 管控路径：ControlService 安装→验证其他节点只做运行时加载
4. **Clippy 检查**：`rtk cargo clippy` 确保无警告

***

## 九、架构审查维度评估

基于 rust-arch-review 技能的五个维度：

| 维度          | 当前评分 | 重构后预期 | 改善点                                            |
| ----------- | ---- | ----- | ---------------------------------------------- |
| Crate 与模块划分 | 7/10 | 8/10  | 新增 persistence/runtime\_ops 分层，职责更清晰           |
| Trait 解耦设计  | 5/10 | 7/10  | EventPublisher 和 PluginRuntime 可提取为 trait，便于测试 |
| 依赖管理        | 8/10 | 8/10  | 无变化，已遵循 workspace 规范                           |
| 错误处理与状态管理   | 6/10 | 8/10  | 持久化层和运行时层错误隔离，SharedDeps 减少 Arc<RwLock> 散布     |
| 异步编程模式      | 7/10 | 8/10  | 消除 DeployService 中的嵌套异步调用链                     |

