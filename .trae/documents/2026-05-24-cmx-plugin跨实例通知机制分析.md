# 2026-05-24 cmx-plugin跨实例通知机制分析文档

## 计划概述

阅读 cmx-plugin 模块所有源码，梳理 Notifier（Redis 跨实例通知）和 GlobalEventBus（进程内事件总线）两种通知方式的完整机制，生成详细分析文档。

## 当前状态分析

已阅读以下核心源码文件：

| 文件                                                      | 职责                                                                 |
| ------------------------------------------------------- | ------------------------------------------------------------------ |
| `crates/libs/cmx-traits/src/event_bus/bus.rs`           | EventBus 核心实现（发布-订阅）                                               |
| `crates/libs/cmx-traits/src/event_bus/global.rs`        | GlobalEventBus 单例管理（OnceLock）                                      |
| `crates/libs/cmx-traits/src/event_bus/types.rs`         | 类型定义：EventTopic、EventPayload、EventHandler                          |
| `crates/libs/cmx-traits/src/lifecycle.rs`               | 事件主题常量、PluginLifecyclePayload、PluginLifecycleListener trait        |
| `crates/libs/cmx-plugin/src/cluster/notification.rs`    | PluginNotifier 实现（RuntimeLoad/RuntimeUnload 已注释）                   |
| `crates/libs/cmx-plugin/src/service/event_publisher.rs` | EventPublisher 统一封装（notify\_runtime\_load/unload 已注释）              |
| `crates/libs/cmx-plugin/src/service/plugin_sync.rs`     | PluginChangeHandler — Redis 通知处理器（RuntimeLoad/RuntimeUnload 处理已注释） |
| `crates/libs/cmx-plugin/src/service/executor.rs`        | PluginOperationExecutor — 统一编排（管控操作已注释）                            |
| `crates/libs/cmx-plugin/src/service/runtime_ops.rs`     | RuntimeOps — 运行时操作层                                                |
| `crates/libs/cmx-plugin/src/service/reconciliation.rs`  | ReconciliationTask — 定时对账                                          |
| `crates/libs/cmx-plugin/src/service/initializer.rs`     | PluginInitializer — 启动初始化                                          |
| `crates/libs/cmx-plugin/src/core/manager.rs`            | PluginManager — 核心协调器                                              |
| `crates/libs/cmx-runtime/src/lifecycle_listener.rs`     | RuntimeLifecycleListener                                           |
| `crates/libs/cmx-service/src/lifecycle_listener.rs`     | ServiceLifecycleListener                                           |

## 输出产物

在 `/media/yqs/工作/rustspace/cmx/cmx-container/.trae/documents/` 下创建文档：

**文件名**: `2026-05-24-cmx-plugin跨实例通知机制分析.md`

## 文档内容规划

### 1. 架构概览

* 两套通知机制的整体关系图

* EventPublisher 统一封装层

### 2. GlobalEventBus 详解

* 实现原理（OnceLock 单例、HashMap\<topic, handlers>、tokio::spawn 异步分发）

* 事件主题常量（7 个：INSTALLED/UPGRADED/UNINSTALLED/DOWNGRADED/REINSTALLED/LOADED/UNLOADED）

* 事件载荷 PluginLifecyclePayload 结构

* 两个订阅者及其处理逻辑：

  * RuntimeLifecycleListener：订阅 5 个事件，调用 unload\_module 清除 WASM 缓存

  * ServiceLifecycleListener：订阅 7 个事件，同步服务定义缓存

### 3. PluginNotifier 详解

* 实现原理（Redis Pub/Sub、频道 cmx:plugin:changed）

* 通知动作 PluginChangeAction（5 个：Installed/Upgraded/Downgraded/Reinstalled/Removed）

* RuntimeLoad/RuntimeUnload 已随管控模式一起禁用（注释状态）

* 通知消息 PluginChangeNotification 结构

* instance\_id 隔离机制

### 4. 两种通知方式的核心区别

* 通信范围：进程内 vs 跨实例

* 载荷丰富度：完整业务数据 vs 最小通知

* 数据来源：直接使用载荷 vs 从数据库查询

* 操作范围：仅内存缓存 vs 文件同步+内存注册

* 触发链路：直接触发 vs 间接触发（Redis 通知 → 本地处理 → 再发 GlobalEventBus）

### 5. 事件接收后的后续操作详解

* GlobalEventBus 两个订阅者的每个事件处理逻辑

* PluginChangeHandler 的每个动作处理逻辑

* RuntimeOps 各方法详解

### 6. 事件发布策略

* 本地操作（API 请求）：GlobalEventBus 进程内事件 + Redis 跨实例变更通知

* 管控操作（ControlService）已禁用，相关代码已注释

### 7. 完整数据流图

#### 7.1 整体架构关系图

```mermaid
graph TB
    subgraph "当前节点（接收API请求）"
        API[API 请求] --> Executor[PluginOperationExecutor]
        Executor --> |1.持久化| Persistence[PluginPersistence<br/>DB + 文件系统]
        Executor --> |2.运行时| RuntimeOps[RuntimeOps<br/>Registry + Contexts + Cache]
        Executor --> |3.审计| Audit[AuditLogger]
        Executor --> |4.事件| EP[EventPublisher]
        EP --> |进程内| GEB[GlobalEventBus]
        EP --> |跨实例| PN[PluginNotifier<br/>Redis Pub/Sub]
    end

    subgraph "当前进程内订阅者"
        GEB --> |事件分发| RLL[RuntimeLifecycleListener<br/>清除 WASM 缓存]
        GEB --> |事件分发| SLL[ServiceLifecycleListener<br/>同步服务定义缓存]
    end

    subgraph "其他节点"
        PN --> |Redis Pub/Sub| PCH[PluginChangeHandler]
        PCH --> |运行时同步| RuntimeOps2[RuntimeOps<br/>下载文件 + 内存注册]
        PCH --> |发布本地事件| EP2[EventPublisher.publish_local_event]
        EP2 --> GEB2[GlobalEventBus]
        GEB2 --> RLL2[RuntimeLifecycleListener]
        GEB2 --> SLL2[ServiceLifecycleListener]
    end
```

#### 7.2 本地操作数据流（安装为例）

```mermaid
sequenceDiagram
    participant API as API请求
    participant Executor as PluginOperationExecutor
    participant Persistence as PluginPersistence
    participant RuntimeOps as RuntimeOps
    participant Audit as AuditLogger
    participant EP as EventPublisher
    participant GEB as GlobalEventBus
    participant Redis as Redis Pub/Sub
    participant OtherNode as 其他节点 PluginChangeHandler
    participant RLL as RuntimeLifecycleListener
    participant SLL as ServiceLifecycleListener

    API->>Executor: execute_install(request)
    Executor->>Persistence: install_persist(request)
    Persistence-->>Executor: PersistResult
    Executor->>RuntimeOps: register_plugin(result)
    RuntimeOps-->>Executor: Ok
    Executor->>Audit: log(audit_record)
    Executor->>EP: publish_installed(result)

    par 进程内事件
        EP->>GEB: publish("plugin.installed", payload)
        GEB->>RLL: handle_installed → 无操作（不订阅此事件）
        GEB->>SLL: handle_installed → 从DB加载服务定义到缓存
    and 跨实例通知
        EP->>Redis: notify_installed(plugin_id, version, app_id)
        Redis-->>OtherNode: PluginChangeNotification{Installed}
        OtherNode->>OtherNode: 跳过自身 + 过滤app_id
        OtherNode->>OtherNode: 查询DB获取最新版本
        OtherNode->>OtherNode: sync_and_register → 下载文件+注册内存
        OtherNode->>OtherNode: publish_local_event("plugin.installed")
        OtherNode->>RLL: handle_installed
        OtherNode->>SLL: handle_installed → 从DB加载服务定义到缓存
    end
```

#### 7.3 本地操作数据流（升级为例）

```mermaid
sequenceDiagram
    participant API as API请求
    participant Executor as PluginOperationExecutor
    participant Persistence as PluginPersistence
    participant RuntimeOps as RuntimeOps
    participant EP as EventPublisher
    participant GEB as GlobalEventBus
    participant Redis as Redis Pub/Sub
    participant OtherNode as 其他节点 PluginChangeHandler
    participant RLL as RuntimeLifecycleListener
    participant SLL as ServiceLifecycleListener

    API->>Executor: execute_upgrade(request)
    Executor->>Persistence: upgrade_persist(request)
    Persistence-->>Executor: PersistResult
    Executor->>RuntimeOps: update_plugin(result)
    Executor->>EP: publish_upgraded(result)

    par 进程内事件
        EP->>GEB: publish("plugin.upgraded", payload)
        GEB->>RLL: handle_upgraded → unload_module() 清除WASM缓存
        GEB->>SLL: handle_upgraded → 清空旧缓存 + 从DB强制加载最新服务定义
    and 跨实例通知
        EP->>Redis: notify_upgraded(plugin_id, version, app_id)
        Redis-->>OtherNode: PluginChangeNotification{Upgraded}
        OtherNode->>OtherNode: 查询DB → sync_and_register
        OtherNode->>OtherNode: publish_local_event("plugin.upgraded")
        OtherNode->>RLL: 清除WASM缓存
        OtherNode->>SLL: 清空旧缓存 + 从DB强制加载
    end
```

#### 7.4 本地操作数据流（卸载为例）

```mermaid
sequenceDiagram
    participant API as API请求
    participant Executor as PluginOperationExecutor
    participant Persistence as PluginPersistence
    participant RuntimeOps as RuntimeOps
    participant EP as EventPublisher
    participant GEB as GlobalEventBus
    participant Redis as Redis Pub/Sub
    participant OtherNode as 其他节点 PluginChangeHandler
    participant RLL as RuntimeLifecycleListener
    participant SLL as ServiceLifecycleListener

    API->>Executor: execute_uninstall(request)
    Executor->>Persistence: uninstall_persist(request)
    Persistence-->>Executor: PersistResult
    Executor->>RuntimeOps: unregister_plugin(plugin_id)
    Executor->>EP: publish_uninstalled(result)

    par 进程内事件
        EP->>GEB: publish("plugin.uninstalled", payload)
        GEB->>RLL: handle_uninstalled → unload_module() 清除WASM缓存
        GEB->>SLL: handle_uninstalled → 清理服务定义缓存
    and 跨实例通知
        EP->>Redis: notify_removed(plugin_id, version, app_id)
        Redis-->>OtherNode: PluginChangeNotification{Removed}
        OtherNode->>OtherNode: unregister_and_cleanup → 注销内存+删除本地文件
        OtherNode->>OtherNode: publish_local_event("plugin.uninstalled")
        OtherNode->>RLL: 清除WASM缓存
        OtherNode->>SLL: 清理服务定义缓存
    end
```

#### 7.5 Redis 通知接收处理流程

```mermaid
flowchart TD
    A[Redis Pub/Sub 收到消息] --> B[反序列化 PluginChangeNotification]
    B --> C{instance_id == 自身?}
    C --> |是| D[跳过处理]
    C --> |否| E{app_id 匹配?}
    E --> |否| F[忽略通知]
    E --> |是| G{action 类型?}

    G --> |Installed/Upgraded/Downgraded| H[handle_plugin_changed]
    G --> |Reinstalled| I[handle_plugin_reinstalled]
    G --> |Removed| J[handle_plugin_removed]

    H --> H1[查询DB获取最新版本]
    H1 --> H2[sync_and_register<br/>下载文件+注册内存]
    H2 --> H3[publish_local_event<br/>INSTALLED/UPGRADED/DOWNGRADED]

    I --> I1[查询DB获取最新版本]
    I1 --> I2[force_resync_and_register<br/>强制重新同步文件+注册]
    I2 --> I3[publish_local_event REINSTALLED]

    J --> J1[unregister_and_cleanup<br/>注销内存+删除本地文件]
    J1 --> J2[publish_local_event UNINSTALLED]
```

#### 7.6 GlobalEventBus 事件分发流程

```mermaid
flowchart TD
    A[EventPublisher 发布事件] --> B[GlobalEventBus.publish<br/>topic + payload]
    B --> C[读取 handlers HashMap]
    C --> D{有订阅者?}
    D --> |否| E[跳过]
    D --> |是| F[遍历每个 handler]
    F --> G[tokio::spawn 异步执行]

    G --> H[RuntimeLifecycleListener]
    G --> I[ServiceLifecycleListener]

    H --> H1{事件类型?}
    H1 --> |UPGRADED/DOWNGRADED/REINSTALLED| H2[unload_module<br/>清除 WASM 实例缓存]
    H1 --> |UNINSTALLED| H2
    H1 --> |UNLOADED ⚠️| H2
    H1 --> |INSTALLED| H3[无操作]
    H1 --> |LOADED ⚠️| H3

    I --> I1{事件类型?}
    I1 --> |INSTALLED| I2[从DB加载服务定义到缓存<br/>service_query.get_services_by_plugin]
    I1 --> |LOADED ⚠️| I2
    I1 --> |UPGRADED/DOWNGRADED/REINSTALLED| I3[清空旧缓存 + 从DB强制加载<br/>repository.get_services_by_plugin]
    I1 --> |UNINSTALLED| I4[清理服务定义缓存<br/>registry.unregister]
    I1 --> |UNLOADED ⚠️| I4
```

> ⚠️ 标注说明：LOADED/UNLOADED 事件主题常量和订阅者代码仍保留，但因管控模式已禁用，当前无实际触发源。这些事件仅在管控模式下的 RuntimeLoad/RuntimeUnload Redis 通知被处理时通过 `publish_local_event` 触发。

#### 7.7 对账任务流程

```mermaid
flowchart TD
    A[定时触发 reconciliation] --> B[查询DB中所有插件<br/>按app_id过滤]
    B --> C[获取Registry中已注册插件]
    C --> D{遍历DB插件}

    D --> E{Registry中存在?}
    E --> |否| F[register_from_db<br/>从DB查询+注册内存]
    E --> |是| G{本地文件存在?}
    G --> |否| H[先unregister_plugin<br/>再sync_and_register<br/>重新下载文件+注册]
    G --> |是| I[无需操作]

    D --> J{Registry中有DB无的插件?}
    J --> |是| K[unregister_and_cleanup<br/>注销内存+删除本地文件]
    J --> |否| L[对账完成]
    F --> L
    H --> L
    I --> L
    K --> L
```

#### 7.8 启动初始化流程

```mermaid
flowchart TD
    A[PluginManager.initialize] --> B[清理临时目录]
    B --> C[PluginInitializer.load_contexts]
    C --> C1[查询DB中所有插件<br/>按app_id过滤]
    C1 --> C2[遍历每个插件]
    C2 --> C3[register_from_db<br/>注册到内存]

    C --> D{Redis Pub/Sub 可用?}
    D --> |是| E[创建 PluginChangeHandler]
    E --> F[注册频道回调<br/>cmx:plugin:changed]
    F --> G[收到消息 → 反序列化 → handler.handle]
    D --> |否| H[跳过Redis订阅]

    G --> I{reconciliation_interval > 0?}
    H --> I
    I --> |是| J[启动 ReconciliationTask<br/>定时对账]
    I --> |否| K[初始化完成]
    J --> K
```

### 8. 辅助机制

* 对账任务（ReconciliationTask）

* 启动初始化（PluginInitializer）

## 验证步骤

1. 文档内容与源码逐一对照确认准确性
2. 事件主题、动作枚举值与源码一致
3. 数据流图覆盖所有关键路径

