# CMX Container Crate 架构分析与 cmx-service 模块解耦方案

## 一、现有 Crate 结构与依赖关系分析

### 1.1 Crate 清单（仅内部 crate）

| Crate | 路径 | 职责定位 |
|-------|------|----------|
| **cmx-utils** | `crates/libs/cmx-utils` | 基础工具库：配置管理、UUID/雪花ID、ZIP 压缩解压、Base64 |
| **cmx-core** | `crates/libs/cmx-core` | 核心领域模型：数据模型（DataValue、Context、Request/Response）、域管理、元数据结构（TableDefine、PluginDefinition）、分页参数 |
| **cmx-database** | `crates/libs/cmx-infra/cmx-database` | 数据库基础设施：连接池管理、事务、CRUD 宏、SQL 执行器 |
| **cmx-buffer** | `crates/libs/cmx-infra/cmx-buffer` | 缓存基础设施：Redis 客户端、分布式锁、发布/订阅 |
| **cmx-metadata** | `crates/libs/cmx-metadata` | 表元数据管理：JSON 配置加载、DDL 生成/解析/执行、i18n |
| **cmx-plugin** | `crates/libs/cmx-plugin` | 插件生命周期管理：ZIP 安装/卸载/升级/降级/激活、签名验证、集群部署、运行时实例管理 |
| **cmx-api** | `crates/libs/cmx-api` | Web API 层：Axum 路由/Handler、中间件、CRUD 框架、OpenAPI |
| **web-server** | `crates/web/web-server` | 应用入口：服务器启动、组件初始化、路由挂载 |

### 1.2 内部依赖关系图

```
                         ┌──────────────┐
                         │  web-server  │
                         └──────┬───────┘
                                │
                    ┌───────────┼───────────┐
                    ▼           ▼           ▼
              ┌──────────┐ ┌──────────┐ ┌──────────┐
              │ cmx-api  │ │cmx-plugin│ │cmx-buffer│
              └────┬─────┘ └────┬─────┘ └──────────┘
                   │            │
          ┌────────┼────┐       │
          ▼        ▼    ▼       ▼
    ┌──────────┐┌──────────┐┌──────────┐
    │cmx-utils││ cmx-core ││cmx-meta- │
    └──────────┘└──────────┘│  data   │
                           └────┬─────┘
                                │
                    ┌───────────┼───────────┐
                    ▼           ▼           ▼
              ┌──────────┐ ┌──────────┐ ┌──────────┐
              │cmx-utils │ │cmx-core  │ │cmx-databa│
              │          │ │          │ │   se     │
              └──────────┘ └────┬─────┘ └────┬─────┘
                                │           │
                                ▼           ▼
                          ┌──────────┐ ┌──────────┐
                          │cmx-core  │ │cmx-utils │
                          └──────────┘ └──────────┘
```

**依赖详情（仅内部依赖）：**

| Crate | 依赖的内部 Crate |
|-------|-----------------|
| **cmx-utils** | （无，基础层） |
| **cmx-core** | （无，基础层） |
| **cmx-database** | cmx-core, cmx-utils |
| **cmx-buffer** | cmx-utils |
| **cmx-metadata** | cmx-core, cmx-database |
| **cmx-plugin** | cmx-core, cmx-metadata, cmx-buffer, cmx-database, cmx-utils |
| **cmx-api** | cmx-utils, cmx-core, cmx-database, cmx-metadata, cmx-plugin |
| **web-server** | cmx-utils, cmx-database, cmx-api, cmx-buffer, cmx-plugin |

### 1.3 依赖层级分析

```
Layer 0 (基础层):   cmx-utils, cmx-core
Layer 1 (基础设施): cmx-database, cmx-buffer
Layer 2 (元数据):   cmx-metadata
Layer 3 (业务层):   cmx-plugin
Layer 4 (API层):    cmx-api
Layer 5 (应用层):   web-server
```

## 二、当前依赖关系评估

### 2.1 优点

1. **清晰的层级结构**：整体遵循自底向上的依赖方向，基础层无外部依赖，层级分明
2. **无循环依赖**：所有依赖关系都是单向的，不存在 A→B→A 的循环
3. **基础设施分离**：cmx-database 和 cmx-buffer 作为独立基础设施层，职责清晰
4. **核心模型与实现分离**：cmx-core 定义模型，cmx-plugin/cmx-metadata 等负责实现

### 2.2 潜在问题

#### 问题1：cmx-plugin 依赖过重（过度依赖）
cmx-plugin 同时依赖了 **5个内部 crate**（cmx-core, cmx-metadata, cmx-buffer, cmx-database, cmx-utils），几乎覆盖了所有底层模块。

- **影响**：cmx-plugin 的编译会触发大量 crate 重新编译；任何底层模块的变更都可能影响 cmx-plugin
- **根因**：cmx-plugin 内部实现了完整的基础设施层（数据库、缓存、存储、消息），导致需要直接依赖所有基础设施

#### 问题2：cmx-api 直接依赖 cmx-plugin（耦合偏紧）
cmx-api 直接依赖 cmx-plugin，导致 API 层与插件管理紧密耦合。

- **影响**：插件模块的任何变更都会触发 cmx-api 重编译；未来新增 cmx-service 时，cmx-api 可能需要同时依赖 cmx-plugin 和 cmx-service，加剧耦合
- **根因**：当前 cmx-api 的 `handlers/plugin/` 模块直接调用 `cmx_plugin::GlobalPluginManager` 全局单例

#### 问题3：cmx-plugin 缺少 cmx-runtime 独立 crate（依赖缺失）
当前 cmx-plugin 内部的 `runtime/` 模块仅管理运行时实例的元数据（激活状态、内存、调用计数），**尚未实现真正的 WASM 执行引擎**。

- **影响**：cmx-plugin 承担了过多职责（生命周期管理 + 运行时实例管理），未来 WASM 执行引擎实现后，耦合会更加严重
- **建议**：应该有一个独立的 cmx-runtime crate 专门负责 WASM 执行

#### 问题4：全局单例模式存在隐式耦合
cmx-plugin 使用 `GlobalPluginManager`（`OnceLock` + `Arc<RwLock<>>`）全局单例，cmx-api 的 handler 直接通过 `GlobalPluginManager::get().await` 获取实例。

- **影响**：模块间通过全局状态隐式通信，不利于测试和模块独立演进
- **根因**：缺少统一的依赖注入/服务定位机制

## 三、cmx-plugin 与 cmx-service 解耦架构方案

### 3.1 需求理解

| 模块 | 职责 | 关键交互 |
|------|------|----------|
| **cmx-plugin** | 插件生命周期管理（安装/卸载/升级/降级/激活）、ZIP 包处理、签名验证 | 需要在插件激活时通知 cmx-service 加载 WASM 实例 |
| **cmx-service**（新增） | 企业级通用服务：处理 cmx-api 通用请求、解析插件编排、调用 cmx-runtime 执行 WASM | 需要查询插件状态、获取 WASM 文件路径 |
| **cmx-runtime**（建议新增） | WASM 运行时引擎：加载/执行/卸载 WASM 模块 | 被cmx-service调用 |

### 3.2 解耦核心原则

1. **cmx-plugin 和 cmx-service 之间不直接依赖**
2. **通过共享接口 crate 或事件总线进行通信**
3. **基础设施（cmx-database, cmx-buffer）由各模块按需依赖**

---

## 四、方案一：Trait 抽象层 + 依赖注入（接口隔离方案）

### 4.1 架构设计

引入一个 **cmx-traits** crate 作为接口抽象层，定义所有跨模块交互的 trait，各模块通过 trait 进行解耦。

```
新增 crate: cmx-traits (接口抽象层)
新增 crate: cmx-runtime (WASM 运行时)
新增 crate: cmx-service (企业服务层)
```

**依赖关系：**

```
cmx-traits  ←── cmx-core, cmx-utils (仅类型)
     ▲              ▲
     │              │
cmx-plugin    cmx-service ──→ cmx-runtime
     │              │
     └──────────────┘
        通过 trait 交互
```

### 4.2 cmx-traits 核心接口定义

```rust
// cmx-traits/src/lib.rs

/// 插件状态查询 trait（cmx-service 用来查询插件信息）
#[async_trait]
pub trait PluginQuery: Send + Sync {
    /// 获取插件信息
    async fn get_plugin(&self, plugin_id: &str) -> Result<PluginInfo>;
    /// 检查插件是否已激活
    async fn is_active(&self, plugin_id: &str) -> bool;
    /// 获取插件的 WASM 文件路径
    async fn get_wasm_path(&self, plugin_id: &str) -> Result<PathBuf>;
    /// 列出所有激活的插件
    async fn list_active_plugins(&self) -> Vec<PluginInfo>;
}

/// 运行时调用 trait（cmx-service 用来调用 WASM 执行）
#[async_trait]
pub trait RuntimeInvoker: Send + Sync {
    /// 调用 WASM 函数
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
    ) -> Result<Vec<u8>>;
    /// 加载 WASM 模块
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<()>;
    /// 卸载 WASM 模块
    async fn unload_module(&self, plugin_id: &str) -> Result<()>;
}

/// 插件生命周期事件通知 trait（cmx-plugin 用来通知 cmx-service）
#[async_trait]
pub trait PluginLifecycleListener: Send + Sync {
    /// 插件已激活
    async fn on_plugin_activated(&self, plugin_id: &str, wasm_path: &Path);
    /// 插件已停用
    async fn on_plugin_deactivated(&self, plugin_id: &str);
    /// 插件已卸载
    async fn on_plugin_uninstalled(&self, plugin_id: &str);
}
```

### 4.3 模块内部依赖关系

```
cmx-traits  (Layer 0.5, 仅依赖 cmx-core, cmx-utils)
    ▲
    │
cmx-plugin  (Layer 3, 依赖 cmx-core, cmx-traits, cmx-database, cmx-buffer, cmx-utils)
    │            不再依赖 cmx-metadata（可选优化）
    │
cmx-runtime  (Layer 3, 依赖 cmx-core, cmx-traits, cmx-utils)
    │            专门负责 WASM 执行引擎
    ▲
    │
cmx-service  (Layer 4, 依赖 cmx-core, cmx-traits, cmx-runtime)
    │            不直接依赖 cmx-plugin
    ▲
    │
cmx-api      (Layer 5, 依赖 cmx-traits, cmx-service, cmx-plugin)
                通过 cmx-traits 接口访问各模块
```

### 4.4 通信机制

**依赖注入方式**：在 `web-server` 的初始化阶段组装所有模块：

```rust
// web-server/src/main.rs 初始化逻辑（伪代码）
async fn init() {
    // 1. 初始化基础设施
    let db_manager = init_db();
    let cache_manager = init_cache();

    // 2. 初始化 cmx-plugin（注入生命周期监听器）
    let runtime = Arc::new(CmxRuntime::new());
    let service = Arc::new(CmxService::new(runtime.clone()));

    let plugin_manager = PluginManagerBuilder::new(settings)
        .with_database(db_manager)
        .with_cache(cache_manager)
        .with_lifecycle_listener(service.clone())  // 注入监听器
        .build().await?;

    // 3. 注入到 AppState 或全局
    let app_state = CmxAppState::new()
        .with_plugin_query(plugin_manager.clone())  // 实现 PluginQuery trait
        .with_runtime_invoker(runtime);              // 实现 RuntimeInvoker trait
}
```

### 4.5 优点

| 优点 | 说明 |
|------|------|
| **完全解耦** | cmx-plugin 和 cmx-service 无直接依赖，仅依赖 cmx-traits 接口 |
| **编译隔离** | cmx-plugin 或 cmx-service 的变更不会互相触发重编译 |
| **可测试性强** | 可以轻松 mock trait 实现进行单元测试 |
| **类型安全** | 编译期检查接口匹配，运行时零成本抽象 |
| **扩展性好** | 新模块只需实现 cmx-traits 中的 trait 即可接入 |

### 4.6 缺点

| 缺点 | 说明 |
|------|------|
| **新增 crate** | 需要新建 cmx-traits 和 cmx-runtime 两个 crate |
| **接口设计成本** | 需要提前设计好 trait 接口，接口变更影响面较大 |
| **样板代码较多** | 每个模块需要实现 trait，组装逻辑在 web-server 中较为繁琐 |
| **间接调用开销** | 通过 trait 对象（dyn trait）调用有轻微的动态分发开销（可通过泛型约束消除） |
| **学习曲线** | 团队需要理解 trait 抽象层的设计模式 |

### 4.7 适用场景

- 团队规模较大，模块由不同开发者维护
- 模块间通信接口相对稳定
- 对编译速度有较高要求
- 需要独立测试各模块

### 4.8 实施复杂度：⭐⭐⭐（中等偏高）

---

## 五、方案二：事件驱动 + 服务注册表（松耦合事件方案）

### 5.1 架构设计

利用现有的 cmx-buffer（Redis Pub/Sub）和 cmx-plugin 内部的 EventBus，通过**事件总线**实现模块间异步通信，各模块通过**服务注册表**（扩展 cmx-plugin 内的 ServiceRegistry 为全局独立模块）进行能力发现。

```
新增 crate: cmx-runtime (WASM 运行时)
新增 crate: cmx-service (企业服务层)
扩展:       cmx-plugin 的 EventBus 提升为跨模块事件总线
```

**依赖关系：**

```
cmx-plugin ──→ cmx-buffer (EventBus)
                  ▲
                  │  (事件订阅)
cmx-service ──────┘
                  │
cmx-service ──→ cmx-runtime (直接依赖，同团队维护)
```

### 5.2 事件定义

在 cmx-core 或新建 cmx-events crate 中定义标准事件：

```rust
/// 插件生命周期事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginLifecycleEvent {
    /// 插件已安装 { plugin_id, version }
    Installed { plugin_id: String, version: String },
    /// 插件已激活 { plugin_id, wasm_path }
    Activated { plugin_id: String, wasm_path: String },
    /// 插件已停用 { plugin_id }
    Deactivated { plugin_id: String },
    /// 插件已卸载 { plugin_id }
    Uninstalled { plugin_id: String },
    /// 插件已升级 { plugin_id, old_version, new_version }
    Upgraded { plugin_id: String, old_version: String, new_version: String },
}

/// 服务调用请求事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInvokeRequest {
    pub request_id: String,
    pub service_id: String,
    pub plugin_id: String,
    pub function_name: String,
    pub payload: Vec<u8>,
    pub reply_to: String,  // 回复通道
}
```

### 5.3 服务注册表（全局化）

将 cmx-plugin 内的 ServiceRegistry 提升为独立的全局服务发现机制：

```rust
/// 全局服务注册表
pub struct GlobalServiceRegistry;

impl GlobalServiceRegistry {
    /// 注册服务能力
    pub fn register(capability: ServiceCapability) -> Result<()>;

    /// 查找服务提供者
    pub fn find_provider(service_id: &str) -> Option<ServiceHandle>;

    /// 调用服务
    pub async fn invoke(
        service_id: &str,
        function_name: &str,
        input: &[u8],
    ) -> Result<Vec<u8>>;
}
```

### 5.4 通信流程

```
1. 插件激活流程：
   cmx-plugin ──(发布事件)──→ EventBus(Redis) ──(订阅)──→ cmx-service
                                                         │
                                                    加载 WASM 模块
                                                    到 cmx-runtime

2. 服务调用流程：
   cmx-api ──(HTTP请求)──→ cmx-service ──(直接调用)──→ cmx-runtime
                                │
                           (查询插件信息)
                                │
                           GlobalServiceRegistry
                           或 EventBus 查询
```

### 5.5 优点

| 优点 | 说明 |
|------|------|
| **极低耦合** | 模块间仅通过事件消息通信，完全不需要知道对方的存在 |
| **天然支持分布式** | 基于 Redis Pub/Sub，未来可扩展为多节点部署 |
| **异步解耦** | 事件发布后立即返回，不阻塞调用方 |
| **可观测性好** | 所有事件可被多个消费者订阅，方便监控和日志 |
| **复用现有基础设施** | cmx-buffer 的 Pub/Sub 和 cmx-plugin 的 EventBus 已有实现 |

### 5.6 缺点

| 缺点 | 说明 |
|------|------|
| **最终一致性** | 事件传递有延迟，不适合需要强一致性的场景 |
| **调试困难** | 异步事件链路追踪复杂，问题定位较难 |
| **类型安全弱** | 事件消息需要序列化/反序列化，丧失编译期类型检查 |
| **错误处理复杂** | 事件消费失败需要重试/死信队列机制 |
| **cmx-service 仍需查询插件状态** | 某些场景下 cmx-service 需要同步查询插件信息，纯事件驱动不够 |
| **性能开销** | 序列化 + Redis 网络开销，对高频调用不友好 |

### 5.7 适用场景

- 需要支持分布式部署（多节点）
- 模块间交互以异步通知为主
- 对延迟不敏感的后台处理场景
- 需要多个消费者响应同一事件

### 5.8 实施复杂度：⭐⭐⭐⭐（较高）

---

## 六、方案对比与推荐

### 6.1 综合对比

| 维度 | 方案一（Trait + DI） | 方案二（事件驱动） |
|------|---------------------|-------------------|
| **耦合程度** | 低（编译期接口耦合） | 极低（运行时消息耦合） |
| **类型安全** | ✅ 编译期检查 | ❌ 运行时反序列化 |
| **编译隔离** | ✅ crate 级别隔离 | ✅ crate 级别隔离 |
| **可测试性** | ✅ Mock trait 即可 | ⚠️ 需要模拟事件总线 |
| **同步调用支持** | ✅ 天然支持 | ❌ 需要额外实现 |
| **分布式支持** | ❌ 进程内 only | ✅ 天然支持 |
| **调试难度** | 低 | 高 |
| **新增代码量** | 中等 | 较多 |
| **学习成本** | 中等（Rust trait 熟悉即可） | 较高（事件驱动架构） |
| **实施复杂度** | ⭐⭐⭐ | ⭐⭐⭐⭐ |
| **适合当前项目** | ✅ 当前单进程架构 | ⚠️ 需要分布式时才适用 |

### 6.2 推荐方案

**推荐采用方案一（Trait 抽象层 + 依赖注入）**，理由：

1. **当前项目为单进程架构**：web-server 是唯一的进程入口，不需要分布式事件总线
2. **同步调用是刚需**：cmx-service 需要同步查询插件状态、同步调用 WASM 执行，事件驱动的异步模型不适合
3. **Rust 生态主流做法**：Trait 抽象是 Rust 社区的标准解耦方式，团队更容易接受
4. **编译期安全**：对 Rust 项目而言，编译期类型检查是核心优势，不应牺牲
5. **渐进式演进**：未来如需分布式，可在 Trait 实现层引入事件总线，不影响上层代码

### 6.3 推荐的最终依赖结构

```
Layer 0 (基础层):     cmx-utils, cmx-core
Layer 1 (基础设施):   cmx-database, cmx-buffer
Layer 1.5 (接口层):   cmx-traits ← 新增
Layer 2 (元数据):     cmx-metadata
Layer 3 (业务层):     cmx-plugin, cmx-runtime ← 新增
Layer 4 (服务层):     cmx-service ← 新增
Layer 5 (API层):      cmx-api
Layer 6 (应用层):     web-server (组装层)
```

### 6.4 新增 Crate 职责定义

| Crate | 职责 | 内部依赖 |
|-------|------|----------|
| **cmx-traits** | 跨模块 trait 接口定义、共享事件/消息类型 | cmx-core, cmx-utils |
| **cmx-runtime** | WASM 运行时引擎：加载/编译/执行/卸载 WASM 模块、内存管理、沙箱隔离 | cmx-core, cmx-traits, cmx-utils |
| **cmx-service** | 企业服务层：通用请求处理、插件编排解析、调用 cmx-runtime 执行 WASM、结果聚合 | cmx-core, cmx-traits, cmx-runtime, cmx-database |

## 七、WASM 宿主函数注册解耦方案

### 7.1 问题分析

cmx-runtime（WASM 运行时引擎）基于 wasmtime，需要通过 `Linker` 向 WASM 模块注入宿主函数（host functions）。这些宿主函数的实现分布在多个模块中：

| 宿主函数类别 | 来源模块 | 典型函数 |
|-------------|---------|---------|
| **数据库操作** | cmx-database | `execute_sql`, `query_sql`, `begin_txn`, `commit_txn`, `rollback_txn` |
| **元数据操作** | cmx-metadata | `execute_ddl`, `get_table_meta`, `parse_table_config` |
| **插件查询** | cmx-plugin | `get_plugin_info`, `call_plugin_service`, `list_plugins` |
| **缓存操作** | cmx-buffer | `cache_get`, `cache_set`, `cache_delete` |
| **日志输出** | cmx-utils | `log_info`, `log_warn`, `log_error` |
| **上下文访问** | cmx-core | `get_context`, `get_request_param`, `set_response` |

**核心矛盾**：cmx-runtime 必须调用 wasmtime 的 `Linker::define()` 来注册宿主函数，而宿主函数的闭包内部需要捕获各模块的 `Arc<DatabaseManager>`、`Arc<CacheManager>` 等依赖。如果 cmx-runtime 直接依赖 cmx-database、cmx-metadata 等模块来获取这些依赖，就会产生严重的反向耦合。

### 7.2 解决思路

cmx-runtime 不应该知道宿主函数的**具体实现**，它只需要知道：
1. 有哪些宿主函数需要注册（函数签名）
2. 这些函数的具体实现在哪里（由谁提供）

关键洞察：**将「宿主函数注册」本身抽象为一个 trait，各模块自行实现注册逻辑，cmx-runtime 只负责管理 Linker 和调用注册方法。**

### 7.3 方案：HostFunctionProvider 注册模式

#### 核心设计

在 cmx-traits 中定义 `HostFunctionProvider` trait，各提供宿主函数的模块实现该 trait，在 web-server 初始化时统一注册到 cmx-runtime。

```
cmx-database  ──实现──→ HostFunctionProvider
cmx-metadata  ──实现──→ HostFunctionProvider
cmx-plugin    ──实现──→ HostFunctionProvider
cmx-buffer    ──实现──→ HostFunctionProvider
cmx-utils     ──实现──→ HostFunctionProvider

                            ▼
                    cmx-traits (定义 trait)
                            ▲
                            │
                    cmx-runtime (消费 trait)
```

#### cmx-traits 中的 trait 定义

```rust
/// 宿主函数注册器 trait
///
/// 各模块通过实现此 trait，将自身提供的宿主函数注册到 WASM Linker。
/// cmx-runtime 在创建 WASM 实例时，遍历所有注册器完成 Linker 配置。
pub trait HostFunctionProvider: Send + Sync {
    /// 命名空间标识（用于日志和调试）
    ///
    /// 如 "cmx.database", "cmx.metadata", "cmx.plugin" 等
    fn namespace(&self) -> &str;

    /// 向 Linker 注册宿主函数
    ///
    /// 实现方在此方法中调用 linker.define_*() 注册宿主函数。
    /// cmx-runtime 会在创建新实例前调用此方法。
    fn register_functions(&self, linker: &mut WasmLinker) -> Result<(), HostFuncError>;
}

/// WASM Linker 的抽象包装（避免 cmx-traits 直接依赖 wasmtime）
///
/// cmx-runtime 提供具体实现，cmx-traits 中仅定义接口。
pub trait WasmLinker {
    /// 注册一个无返回值的宿主函数
    fn define_func(
        &mut self,
        module: &str,
        name: &str,
        func: HostFunctionWrapper,
    ) -> Result<(), HostFuncError>;

    /// 注册一个返回值的宿主函数
    fn define_func_with_result(
        &mut self,
        module: &str,
        name: &str,
        func: HostFunctionWithResultWrapper,
    ) -> Result<(), HostFuncError>;
}

/// 宿主函数包装器（类型擦除后的闭包）
///
/// 使用 Box<dyn Fn> 进行类型擦除，避免 trait 暴露 wasmtime 类型参数
pub type HostFunctionWrapper = Box<dyn Fn(&mut WasmCaller<'_>) + Send + Sync>;
pub type HostFunctionWithResultWrapper =
    Box<dyn Fn(&mut WasmCaller<'_>) -> HostFuncResult + Send + Sync>;

/// WASM 调用者上下文抽象（提供 memory/read/write 访问）
pub trait WasmCaller<'a> {
    /// 从 WASM 内存读取数据
    fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>>;
    /// 向 WASM 内存写入数据
    fn write_memory(&mut self, offset: u32, data: &[u8]) -> Result<()>;
    /// 获取调用者数据
    fn caller_data(&self) -> &CallerData;
}

/// 调用者数据（传递给宿主函数的运行时上下文）
#[derive(Debug, Clone)]
pub struct CallerData {
    /// 当前插件 ID
    pub plugin_id: String,
    /// 请求数据集 ID
    pub dataset_id: String,
    /// 数据库 ID
    pub db_id: String,
    /// 事务 ID（可选）
    pub txn_id: Option<String>,
}

/// 宿主函数错误类型
#[derive(Debug, thiserror::Error)]
pub enum HostFuncError {
    #[error("函数注册失败: {0}")]
    RegistrationFailed(String),
    #[error("函数执行失败: {0}")]
    ExecutionFailed(String),
    #[error("内存访问越界")]
    MemoryOutOfBounds,
    #[error("无效参数: {0}")]
    InvalidParam(String),
}
```

#### cmx-database 的实现示例

```rust
// cmx-database/src/host_functions.rs

use cmx_traits::{HostFunctionProvider, WasmLinker, CallerData, HostFuncError};
use std::sync::Arc;

/// 数据库宿主函数注册器
pub struct DatabaseHostFunctions {
    /// 数据库管理器引用
    db_manager: Arc<DatabaseManager>,
}

impl DatabaseHostFunctions {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }
}

impl HostFunctionProvider for DatabaseHostFunctions {
    fn namespace(&self) -> &str {
        "cmx.database"
    }

    fn register_functions(&self, linker: &mut WasmLinker) -> Result<(), HostFuncError> {
        let db = self.db_manager.clone();

        // 注册 execute_sql 宿主函数
        linker.define_func_with_result("cmx:database", "execute_sql", Box::new(
            move |caller: &mut WasmCaller<'_>| -> HostFuncResult {
                // 1. 从 WASM 内存读取参数
                let data = caller.caller_data();
                let sql_ptr = /* 从内存读取 */;
                let sql = String::from_utf8_lossy(&sql_bytes);

                // 2. 调用 cmx-database 的 API
                let result = cmx_database::execute_sql(
                    &data.db_id,
                    data.txn_id.as_deref(),
                    &sql,
                );

                // 3. 将结果写回 WASM 内存
                Ok(result_bytes)
            }
        ))?;

        // 类似地注册 query_sql, begin_txn, commit_txn 等...
        Ok(())
    }
}
```

#### cmx-runtime 的消费方式

```rust
// cmx-runtime/src/engine.rs

use cmx_traits::HostFunctionProvider;

/// WASM 运行时引擎
pub struct WasmEngine {
    /// 宿主函数注册器列表
    host_providers: Vec<Box<dyn HostFunctionProvider>>,
    /// wasmtime Engine
    engine: wasmtime::Engine,
}

impl WasmEngine {
    /// 注册宿主函数提供者
    pub fn register_provider(&mut self, provider: Box<dyn HostFunctionProvider>) {
        tracing::info!("注册宿主函数提供者: {}", provider.namespace());
        self.host_providers.push(provider);
    }

    /// 创建 Linker 并注册所有宿主函数
    fn build_linker(&self) -> Result<wasmtime::Linker<WasmInstanceContext>> {
        let mut linker = wasmtime::Linker::new(&self.engine);

        // 遍历所有注册器，注册宿主函数
        for provider in &self.host_providers {
            let mut adapter = RuntimeLinkerAdapter::new(&mut linker);
            provider.register_functions(&mut adapter)?;
        }

        Ok(linker)
    }

    /// 加载并实例化 WASM 模块
    pub async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<()> {
        let linker = self.build_linker()?;
        let module = wasmtime::Module::from_file(&self.engine, wasm_path)?;
        let instance = linker.instantiate_async(&module).await?;
        // ...
    }
}

/// Linker 适配器（将 cmx-traits 的 WasmLinker 适配为 wasmtime::Linker）
struct RuntimeLinkerAdapter<'a> {
    inner: &'a mut wasmtime::Linker<WasmInstanceContext>,
}
```

#### web-server 初始化时的组装

```rust
// web-server/src/config.rs 初始化逻辑

async fn init_runtime() {
    let mut engine = WasmEngine::new()?;

    // 注册来自各模块的宿主函数（按命名空间隔离）
    engine.register_provider(Box::new(
        DatabaseHostFunctions::new(get_default_db_manager())
    ));

    engine.register_provider(Box::new(
        MetadataHostFunctions::new(/* ... */)
    ));

    engine.register_provider(Box::new(
        PluginHostFunctions::new(/* ... */)
    ));

    engine.register_provider(Box::new(
        BufferHostFunctions::new(GlobalCacheManager::get_arc())
    ));

    engine.register_provider(Box::new(
        LoggingHostFunctions::new()
    ));

    // 注册到全局或 AppState
    GlobalWasmEngine::initialize(engine).await;
}
```

### 7.4 WASM 函数命名约定

采用 `namespace:function_name` 的命名规范，避免函数名冲突：

| 命名空间 | 模块 | 函数示例 |
|---------|------|---------|
| `cmx:database` | cmx-database | `cmx:database/execute_sql`, `cmx:database/query_sql` |
| `cmx:database/txn` | cmx-database | `cmx:database/txn/begin`, `cmx:database/txn/commit` |
| `cmx:metadata` | cmx-metadata | `cmx:metadata/execute_ddl`, `cmx:metadata/get_table_def` |
| `cmx:plugin` | cmx-plugin | `cmx:plugin/get_info`, `cmx:plugin/call_service` |
| `cmx:buffer` | cmx-buffer | `cmx:buffer/cache_get`, `cmx:buffer/cache_set` |
| `cmx:log` | cmx-utils | `cmx:log/info`, `cmx:log/warn`, `cmx:log/error` |
| `cmx:context` | cmx-core | `cmx:context/get_param`, `cmx:context/set_response` |

### 7.5 依赖关系变化

解耦后的依赖关系：

```
cmx-traits  (定义 HostFunctionProvider trait, WasmLinker trait, CallerData 等)
     ▲
     │                cmx-runtime  (依赖 cmx-traits, wasmtime)
     │                     │
     │    ┌────────────────┼────────────────┐
     │    ▼                ▼                ▼
cmx-database      cmx-metadata     cmx-buffer
(实现 trait)      (实现 trait)     (实现 trait)
     │                │                │
     │    ┌───────────┘                │
     │    ▼                            │
cmx-plugin (实现 trait)               │
     │                                │
     └────────────────────────────────┘
                    ▲
                    │
              web-server (组装层：创建 engine 并注册所有 provider)
```

**关键点**：
- cmx-runtime **不依赖** cmx-database、cmx-metadata、cmx-plugin、cmx-buffer 中的任何一个
- 各模块**不依赖** cmx-runtime（它们只依赖 cmx-traits 的 trait 定义）
- 所有组装逻辑集中在 web-server
- cmx-runtime 仅依赖 cmx-traits + wasmtime

### 7.6 优化：支持按插件动态过滤宿主函数

某些插件可能不需要所有宿主函数（如只读插件不需要写操作）。可以通过扩展 trait 支持动态过滤：

```rust
/// 宿主函数注册器 trait（扩展版）
pub trait HostFunctionProvider: Send + Sync {
    fn namespace(&self) -> &str;

    fn register_functions(&self, linker: &mut WasmLinker) -> Result<(), HostFuncError>;

    /// 返回该提供者支持的所有函数名（用于元数据查询）
    fn provided_functions(&self) -> Vec<&str> {
        vec![]  // 默认实现
    }

    /// 根据插件上下文动态决定是否注册某些函数
    ///
    /// 例如：只读插件不注册 execute_sql 等写操作函数
    fn should_register(&self, _func_name: &str, _context: &CallerData) -> bool {
        true  // 默认全部注册
    }
}
```

### 7.7 cmx-database 现有 API 的适配

当前 cmx-database 的 `transaction/api.rs` 已经提供了 `execute_sql`、`query_sql`、`execute_sql_with_params`、`query_sql_with_params` 等函数，它们通过 `db_id` + `txn_id` 参数定位数据库和事务，天然适合作为宿主函数：

```rust
// cmx-database/src/host_functions.rs

impl HostFunctionProvider for DatabaseHostFunctions {
    fn register_functions(&self, linker: &mut WasmLinker) -> Result<(), HostFuncError> {
        // execute_sql: 从 CallerData 获取 db_id/txn_id，从内存读取 sql
        // 直接调用已有的 execute_sql() API
        // query_sql:   类似，调用已有的 query_sql() API
        // begin_txn:   调用 begin_transaction_by_id()
        // commit_txn:  调用 commit_txn_by_id()
        // rollback_txn: 调用 rollback_txn_by_id()
        Ok(())
    }
}
```

这些 API 无需修改，只需在 `HostFunctionProvider` 实现中进行参数提取和结果转换的适配工作。

### 7.8 方案优势总结

| 优势 | 说明 |
|------|------|
| **零耦合** | cmx-runtime 不依赖任何宿主函数的提供模块，只依赖 cmx-traits 接口 |
| **开放封闭** | 新增宿主函数只需实现 `HostFunctionProvider`，无需修改 cmx-runtime |
| **命名空间隔离** | 按模块命名空间组织，避免函数名冲突 |
| **可插拔** | 运行时可以动态增减 HostFunctionProvider |
| **可测试** | 可以用 MockProvider 替换真实实现进行单元测试 |
| **渐进式迁移** | 现有 cmx-database API 无需改动，仅做薄适配层 |

## 八、更新后的完整依赖关系图

```
Layer 0   (基础层):     cmx-utils, cmx-core
Layer 1   (基础设施):   cmx-database, cmx-buffer
Layer 1.5 (接口层):     cmx-traits ← 新增（定义所有跨模块 trait）
Layer 2   (元数据):     cmx-metadata
Layer 3   (业务层):     cmx-plugin, cmx-runtime ← 新增
Layer 3.5 (服务层):     cmx-service ← 新增
Layer 4   (API层):      cmx-api
Layer 5   (应用层):     web-server (组装层)
```

**各模块内部依赖：**

| Crate | 依赖的内部 Crate |
|-------|-----------------|
| **cmx-utils** | （无） |
| **cmx-core** | （无） |
| **cmx-database** | cmx-core, cmx-utils, cmx-traits（实现 HostFunctionProvider） |
| **cmx-buffer** | cmx-utils, cmx-traits（实现 HostFunctionProvider） |
| **cmx-metadata** | cmx-core, cmx-database, cmx-traits（实现 HostFunctionProvider） |
| **cmx-traits** | cmx-core, cmx-utils（仅类型引用） |
| **cmx-plugin** | cmx-core, cmx-metadata, cmx-buffer, cmx-database, cmx-utils, cmx-traits（实现 HostFunctionProvider） |
| **cmx-runtime** | cmx-core, cmx-traits, cmx-utils |
| **cmx-service** | cmx-core, cmx-traits, cmx-runtime |
| **cmx-api** | cmx-traits, cmx-service, cmx-plugin |
| **web-server** | cmx-utils, cmx-database, cmx-api, cmx-buffer, cmx-plugin, cmx-runtime |

## 九、更新后的实施步骤（方案一）

### 步骤1：创建 cmx-traits crate
- 定义 `PluginQuery`、`RuntimeInvoker`、`PluginLifecycleListener` trait
- 定义 `HostFunctionProvider`、`WasmLinker`、`WasmCaller`、`CallerData` trait 和类型
- 定义跨模块共享的错误类型和事件结构体
- 添加到 workspace members

### 步骤2：创建 cmx-runtime crate
- 实现 WASM 运行时引擎（基于 wasmtime）
- 实现 `RuntimeInvoker` trait
- 实现 `RuntimeLinkerAdapter`（适配 cmx-traits 的 WasmLinker 到 wasmtime::Linker）
- 支持 HostFunctionProvider 的注册和遍历
- 支持模块加载/卸载/函数调用/内存管理

### 步骤3：为各模块实现 HostFunctionProvider
- cmx-database: 实现 `DatabaseHostFunctions`（封装现有 transaction API）
- cmx-metadata: 实现 `MetadataHostFunctions`
- cmx-plugin: 实现 `PluginHostFunctions`
- cmx-buffer: 实现 `BufferHostFunctions`
- cmx-utils: 实现 `LoggingHostFunctions`

### 步骤4：创建 cmx-service crate
- 实现通用请求处理逻辑
- 实现 `PluginLifecycleListener` trait（监听插件激活/停用事件）
- 通过 `PluginQuery` trait 查询插件信息
- 通过 `RuntimeInvoker` trait 调用 WASM 执行

### 步骤5：重构 cmx-plugin
- 让 PluginManager 实现 `PluginQuery` trait
- 在 PluginManager 中注入 `PluginLifecycleListener`，在激活/停用时通知 cmx-service
- 实现 `PluginHostFunctions` 宿主函数注册

### 步骤6：重构 cmx-api
- 将 `handlers/plugin/` 中的直接 `GlobalPluginManager` 调用改为通过 trait 接口
- 新增 cmx-service 相关的 handler
- 从 AppState 中获取 trait 实例而非全局单例

### 步骤7：重构 web-server 初始化
- 创建 WasmEngine 并注册所有 HostFunctionProvider
- 统一组装所有模块的依赖关系
- 将 trait 实例注入到 AppState
- 逐步替换全局单例为显式依赖注入
