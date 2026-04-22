# cmx-service — 企业级通用服务层

插件编排的执行引擎，协调 `PluginQuery` 和 `RuntimeInvoker` 完成请求处理。

## 目录

- [模块概述](#模块概述)
- [设计思想](#设计思想)
- [代码结构](#代码结构)
- [核心类型](#核心类型)
- [编排执行](#编排执行)
- [全局单例](#全局单例)
- [使用指南](#使用指南)
- [依赖约束](#依赖约束)

---

## 模块概述

`cmx-service` 是 CMX 插件系统的服务编排层，提供：

- **服务调用** — 单次 WASM 函数调用
- **编排执行** — 基于 Flow JSON 的 DAG 编排，支持事务框、多分支路由
- **生命周期监听** — 响应插件激活/停用/升级/降级事件，自动同步服务缓存
- **服务注册中心** — 服务信息的内存缓存
- **服务仓储层** — 服务定义的数据库访问（CRUD + 分页）
- **HTTP Handler** — 提供 HTTP 接口封装供 cmx-api 调用

---

## 设计思想

### 1. 依赖倒置原则

`cmx-service` 不依赖 `cmx-plugin`，通过 trait 对象交互：

```
cmx-service ──► cmx-traits ◄── cmx-plugin (PluginQuery)
                    ◄── cmx-runtime (RuntimeInvoker)
```

### 2. 服务编排模式

基于 Flow JSON 的 DAG 编排执行：

```
┌─────────────────────────────────────────────────────────────────┐
│                         Orchestrator                             │
├─────────────────────────────────────────────────────────────────┤
│  execute_service()                                               │
│       ↓                                                          │
│  ┌─────────────┐  ┌──────────────────┐  ┌─────────────────┐    │
│  │ FlowNavigator│  │TransactionManager│  │  NodeHandler    │    │
│  │ (流程导航)   │  │  (事务管理)      │  │  (节点执行)     │    │
│  └─────────────┘  └──────────────────┘  └─────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### 3. 节点类型

| 节点类型 | 说明 |
|----------|------|
| `skylake-start` | 开始节点，流程入口 |
| `skylake-end` | 结束节点，流程出口 |
| `skylake-func` | 函数节点，执行 WASM 函数 |
| `skylake-switch` | 分支节点，根据返回值选择执行路径 |
| `skylake-transaction` | 事务框节点，内部子节点在同一事务中执行 |

---

## 代码结构

```
crates/libs/cmx-service/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # 模块入口，全局单例
│   ├── service.rs                # CmxService 核心服务
│   ├── handler.rs                # ServiceHandler HTTP 处理器
│   ├── request.rs                # 请求/响应类型 (InvokeRequest / InvokeResponse)
│   ├── error.rs                  # ServiceError 错误类型
│   ├── registry.rs               # ServiceRegistry 服务注册中心（内存缓存）
│   ├── repository.rs             # ServiceRepository 服务仓储层（数据库访问）
│   ├── service_query_impl.rs     # ServiceQuery trait 实现（缓存优先）
│   ├── service_storage_impl.rs   # ServiceStorage trait 实现
│   ├── lifecycle_listener.rs     # 生命周期监听器（插件安装/升级/卸载/降级）
│   └── orchestrator/             # 编排器模块
│       ├── mod.rs                # 模块入口，统一导出
│       ├── types.rs              # 类型定义 (OrchestrationResult, ExecutionStep 等)
│       ├── executor.rs           # Orchestrator 主执行器
│       ├── node_handler.rs       # 节点执行器（统一 func/switch 调用逻辑）
│       ├── flow_navigator.rs     # 流程导航器（节点和边查找）
│       └── transaction_manager.rs # 事务管理器（事务生命周期管理）
```

---

## 核心类型

### Orchestrator

编排执行器，支持基于 Flow JSON 的 DAG 编排：

```rust
pub struct Orchestrator {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_query: Arc<dyn ServiceQuery>,
    default_db_id: String,
}
```

### OrchestrationResult

编排执行结果：

```rust
pub struct OrchestrationResult {
    /// 是否执行成功
    pub success: bool,
    /// 最终输出结果
    pub output: Option<serde_json::Value>,
    /// 各步骤执行记录
    pub steps: Vec<ExecutionStep>,
    /// 总执行耗时（微秒）
    pub total_elapsed_us: u64,
    /// 结构化错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OrchestrationError>,
}
```

### ExecutionStep

执行步骤记录，自包含成功/失败状态和排错信息：

```rust
pub struct ExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub status: StepStatus,                // Success / Failed / Skipped
    pub output: Option<serde_json::Value>,
    pub elapsed_us: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,             // 失败时的错误描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,  // 失败时的上一步输出
}
```

### StepStatus

步骤状态枚举：

```rust
pub enum StepStatus {
    Success,   // 执行成功
    Failed,    // 执行失败
    Skipped,   // 跳过未执行
}
```

### OrchestrationError

结构化错误信息：

```rust
pub struct OrchestrationError {
    /// 错误摘要信息
    pub message: String,
}
```

> **设计说明**：失败步骤的详细信息（node_id、error、previous_output）统一记录在 `steps` 数组中对应步骤的 `ExecutionStep` 里，不再单独维护 `failed_step` 字段。失败时 `steps` 数组中最后一个 `status=Failed` 的步骤即为失败步骤。

### ExecutionContext

执行上下文，在编排执行过程中传递：

```rust
pub struct ExecutionContext {
    /// 当前步骤输出（传递给下一个步骤的输入）
    pub current_output: serde_json::Value,
    /// 服务调用上下文（包含初始入参、请求头、各步骤输出、事务ID）
    pub svr_context: SVRContext,
}
```

### ExecuteOptions

执行选项：

```rust
pub struct ExecuteOptions {
    /// 是否返回 steps 数据
    /// - false: 仅返回最终结果（生产环境推荐）
    /// - true: 返回所有步骤数据（调试时使用）
    /// - 失败时始终返回步骤数据
    pub include_steps: bool,
}
```

### CmxService

核心服务结构，持有 trait 对象引用：

```rust
pub struct CmxService {
    plugin_query: Arc<dyn PluginQuery>,
    runtime: Arc<dyn RuntimeInvoker>,
    config: ServiceConfig,
}
```

### 请求/响应类型

```rust
// 单次调用请求
pub struct InvokeRequest {
    pub plugin_id: String,
    pub function_name: String,
    pub input: serde_json::Value,
    pub db_id: Option<String>,
    pub request_id: Option<String>,
    pub tenant_id: Option<String>,
}

// 单次调用响应
pub struct InvokeResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub elapsed_us: u64,
    pub fuel_consumed: u64,
    pub error: Option<String>,
}
```

---

## 编排执行

### 执行流程

```text
1. 查询服务编排定义（ServiceOrchestration）
2. 初始化执行上下文（ExecutionContext）
3. 查找开始节点（skylake-start）
4. 循环执行节点：
   a. 查找当前节点
   b. 管理事务状态（TransactionManager）
   c. 根据节点类型执行：
      - skylake-start: 跳转到下一个节点
      - skylake-end: 提交事务，结束循环
      - skylake-func: 执行函数，跳转到下一个节点
      - skylake-switch: 执行函数，根据返回值选择分支
      - skylake-transaction: 执行事务框内的所有子节点
5. 循环退出后处理（提交/回滚事务）
6. 构建返回结果（OrchestrationResult）
```

### 节点执行流程

```text
解析节点元信息 → 确保 WASM 模块已加载 → 构建 FunctionInput → 调用 WASM 函数 → 解析 FunctionOutput → 更新 ExecutionContext → 记录 ExecutionStep
```

- `func` 和 `switch` 节点的执行逻辑统一由 `NodeHandler::execute_node()` 处理
- 输入序列化使用 MessagePack（`rmp_serde`），而非 JSON
- `switch` 节点根据函数返回值构建端口ID（如返回 `"1"` → 端口 `"out_1"`）

### 事务管理

事务状态机：

```text
[无事务] --节点进入事务框--> [有事务] --节点离开事务框--> [提交事务] --> [无事务]
    |                              |
    +--节点不在事务框--> 正常执行   +--执行失败--> [回滚事务] --> [无事务]
```

状态转换规则：

| 当前状态 | 节点 parent | 操作 |
|----------|-------------|------|
| 无活跃事务 | None | 无操作，正常执行 |
| 无活跃事务 | Some(id) | 开启新事务 |
| 有活跃事务 | Some(id) 且 id 相同 | 继续在当前事务中执行 |
| 有活跃事务 | None 或 id 不同 | 提交当前事务，必要时开启新事务 |

### 失败处理

当节点执行失败时：
1. 在 `steps` 数组中追加一个 `status=Failed` 的 `ExecutionStep`，包含 `error` 和 `previous_output`
2. 构建 `OrchestrationError { message }` 摘要信息
3. 跳出执行循环
4. 如果有活跃事务则回滚
5. 返回 `OrchestrationResult { success: false, error: Some(...), steps: [...] }`

### 调用示例

```rust
use cmx_service::{Orchestrator, ExecuteOptions};

// 创建编排执行器
let orchestrator = Orchestrator::new(
    runtime.clone(),
    plugin_query.clone(),
    service_query.clone(),
    default_db_id,
);

// 执行服务编排
let options = ExecuteOptions::new(true);  // 返回步骤数据
let result = orchestrator.execute_service(
    "user-service",
    svr_context,
    options,
).await?;

if result.success {
    println!("输出: {:?}", result.output);
    println!("总耗时: {} μs", result.total_elapsed_us);
} else {
    // 获取错误摘要
    if let Some(err) = &result.error {
        println!("失败: {}", err.message);
    }
    // 从 steps 中找到失败步骤，获取详细排错信息
    for step in &result.steps {
        if matches!(step.status, StepStatus::Failed) {
            println!("失败节点: {} ({})", step.node_name, step.node_id);
            println!("错误信息: {:?}", step.error);
            println!("上一步输出: {:?}", step.previous_output);
        }
    }
}
```

### 响应 JSON 示例

成功时：

```json
{
  "success": true,
  "output": {"result": "ok"},
  "steps": [],
  "total_elapsed_us": 1500,
  "error": null
}
```

失败时：

```json
{
  "success": false,
  "output": null,
  "steps": [
    {
      "node_id": "node-1",
      "node_name": "查询用户",
      "node_type": "skylake-func",
      "status": "Success",
      "output": {"user_id": "123"},
      "elapsed_us": 500,
      "error": null,
      "previous_output": null
    },
    {
      "node_id": "node-2",
      "node_name": "更新余额",
      "node_type": "skylake-func",
      "status": "Failed",
      "output": null,
      "elapsed_us": 0,
      "error": "运行时调用失败: 余额不足",
      "previous_output": {"user_id": "123"}
    }
  ],
  "total_elapsed_us": 800,
  "error": {
    "message": "步骤 [更新余额(node-2)] 执行失败: 运行时调用失败: 余额不足"
  }
}
```

---

## 服务查询与缓存

### 缓存优先策略

`ServiceQueryImpl` 实现 `ServiceQuery` trait，采用缓存优先策略：

```text
查询请求 → 检查内存缓存 (ServiceRegistry)
             ├── 命中 → 直接返回
             └── 未命中 → 查询数据库 (ServiceRepository) → 回写缓存 → 返回
```

### 生命周期同步

`ServiceLifecycleListener` 监听插件生命周期事件，自动同步服务缓存：

| 事件 | 处理逻辑 |
|------|----------|
| 插件安装 | 从数据库加载服务定义到缓存 |
| 插件升级 | 清空旧缓存 → 强制从数据库加载最新数据 |
| 插件卸载 | 清理内存缓存 |
| 插件降级 | 清空旧缓存 → 强制从数据库加载最新数据 |

---

## 全局单例

提供全局访问器，避免在应用层传递引用：

```rust
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage, GlobalServiceRegistry};

// 初始化
GlobalServiceQuery::set(service_query)?;
GlobalServiceStorage::set(service_storage)?;
GlobalServiceRegistry::set(registry)?;

// 使用
let query = GlobalServiceQuery::get();
let storage = GlobalServiceStorage::get();
let registry = GlobalServiceRegistry::get();

// 检查是否已初始化
if GlobalServiceQuery::is_initialized() { /* ... */ }
```

---

## 使用指南

### 1. 创建服务实例

```rust
use cmx_service::{CmxService, ServiceConfig};
use cmx_traits::{PluginQuery, RuntimeInvoker};
use std::sync::Arc;

// 使用默认配置
let service = CmxService::with_defaults(
    plugin_query,  // Arc<dyn PluginQuery>
    runtime,       // Arc<dyn RuntimeInvoker>
);

// 或使用自定义配置
let config = ServiceConfig {
    invoke_timeout_ms: 60000,
    max_retries: 5,
    enable_orchestration_cache: true,
};
let service = CmxService::new(plugin_query, runtime, config);
```

### 2. 单次调用

```rust
use cmx_service::InvokeRequest;

let request = InvokeRequest {
    plugin_id: "my-plugin".to_string(),
    function_name: "handle_request".to_string(),
    input: json!({"data": "value"}),
    db_id: Some("main-db".to_string()),
    request_id: Some("req-001".to_string()),
    tenant_id: None,
};

let response = service.invoke(&request).await?;

if response.success {
    println!("输出: {:?}", response.output);
    println!("耗时: {} μs", response.elapsed_us);
    println!("Fuel: {}", response.fuel_consumed);
}
```

### 3. 通过 HTTP Handler 使用

```rust
use cmx_service::ServiceHandler;

// 创建 Handler（从 CmxService 实例）
let handler = ServiceHandler::new(service);

// 处理调用请求
let response = handler.handle_invoke(request).await;
```

---

## 依赖约束

### 允许的依赖

- `cmx-core` — 基础类型
- `cmx-traits` — trait 定义
- `cmx-database` — 直接 SQL 执行和事务管理

### 禁止的依赖

- ❌ `cmx-plugin` — 通过 `PluginQuery` trait 交互
- ❌ `cmx-runtime` — 通过 `RuntimeInvoker` trait 交互

### 依赖图

```
cmx-service
├── cmx-core
├── cmx-traits
│   └── cmx-core
├── cmx-database
│   ├── cmx-core
│   └── cmx-traits
└── tokio (异步运行时)
```

---

## 错误处理

```rust
pub enum ServiceError {
    PluginNotFound(String),
    PluginNotActive(String),
    WasmNotLoaded(String),
    InvokeFailed(String),
    OrchestrationFailed { step_id: String, message: String },
    InputParseError(String),
    OutputSerializeError(String),
    DatabaseError(String),
    TraitError(TraitError),
    NodeExecutionFailed { node_id: String, node_name: String, node_type: String, detail: String },
    TransactionRolledBack { txn_id: String, reason: String },
    InternalError(String),
}
```

---

## 配置选项

```rust
pub struct ServiceConfig {
    /// 默认调用超时（毫秒）
    pub invoke_timeout_ms: u64,  // 默认: 30000

    /// 最大重试次数
    pub max_retries: u32,        // 默认: 3

    /// 是否启用编排缓存
    pub enable_orchestration_cache: bool,  // 默认: true
}
```

---

*文档版本: 3.0.0*
*最后更新: 2026-04-22*
