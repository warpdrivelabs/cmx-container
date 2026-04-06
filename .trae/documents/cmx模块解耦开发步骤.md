# CMX Container 解耦重构 — 详细开发步骤计划

> 本文档基于 [cmx-crate-architecture-analysis-and-decoupling-plan.md](cmx模块解耦方案.md) 架构方案，
> 为 AI 开发者提供逐步可执行的开发指导。

---

## 全局约定

### 代码风格与规范

1. **注释语言**：所有函数、结构体、字段、模块必须添加中文注释（遵循用户规则）
2. **错误处理**：统一使用 `thiserror` 定义错误类型，各 crate 定义自己的 Error enum，通过 `#[from]` 关联下游错误
3. **异步 trait**：使用 `async-trait` crate 的 `#[async_trait]` 宏（workspace 已有依赖）
4. **全局单例**：新模块统一使用 `std::sync::OnceLock` + `Arc<RwLock<T>>` 模式（与现有 `GlobalPluginManager` 一致）
5. **日志**：统一使用 `tracing` crate（`info!`, `debug!`, `warn!`, `error!`）
6. **命名空间**：WASM 宿主函数采用 `cmx:模块名/函数名` 格式

### 依赖添加规范

- 新增 crate 必须在根 `Cargo.toml` 的 `[workspace.members]` 和 `[workspace.dependencies]` 中注册
- 各 crate 的 `Cargo.toml` 中使用 `cmx-xxx = { workspace = true }` 引用内部依赖
- 第三方依赖优先从 `[workspace.dependencies]` 引用，必要时才在 crate 级别声明

### 文件路径约定

```
新增 crate 统一放置在:
  crates/libs/cmx-traits/
  crates/libs/cmx-runtime/
  crates/libs/cmx-service/
```

---

## 阶段一：基础设施层 — 创建 cmx-traits 接口抽象 crate

### 目标

建立所有跨模块交互的 trait 定义，作为解耦的核心枢纽。本阶段不修改任何现有 crate 的代码。

### 任务 1.1：创建 crate 骨架

**操作步骤：**

1. 在 `crates/libs/cmx-traits/` 下创建目录结构：
   ```
   crates/libs/cmx-traits/
   ├── Cargo.toml
   └── src/
       ├── lib.rs
       ├── plugin_query.rs      # 插件查询 trait
       ├── runtime_invoker.rs   # 运行时调用 trait
       ├── lifecycle.rs         # 生命周期监听 trait
       ├── host_func.rs         # 宿主函数注册 trait
       ├── error.rs             # 统一错误类型
       └── caller_data.rs       # WASM 调用者上下文
   ```

2. 编写 `Cargo.toml`：
   ```toml
   [package]
   name = "cmx-traits"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   cmx-core = { workspace = true }
   cmx-utils = { workspace = true }
   async-trait = { workspace = true }
   thiserror = { workspace = true }
   serde = { workspace = true }
   tokio = { workspace = true }
   ```

3. 在根 `Cargo.toml` 的 `[workspace.members]` 添加 `"crates/libs/cmx-traits"`
4. 在根 `Cargo.toml` 的 `[workspace.dependencies]` 添加 `cmx-traits = { path = "crates/libs/cmx-traits" }`

**AI 注意事项：**
- cmx-traits 仅依赖 cmx-core（类型引用）和 cmx-utils，不得依赖 cmx-database、cmx-plugin、cmx-buffer 等业务 crate
- cmx-traits 中不得依赖 wasmtime（wasmtime 类型仅在 cmx-runtime 内部使用）

### 任务 1.2：定义 PluginQuery trait

**文件**：`src/plugin_query.rs`

**设计要点：**
- 此 trait 供 cmx-service 使用，用于查询插件信息
- cmx-plugin 的 `PluginManager` 将实现此 trait
- 返回类型使用 `cmx_plugin::domain::plugin::PluginInfo`（通过泛型或重新定义轻量结构体避免依赖 cmx-plugin）

**关键决策：** 由于 cmx-traits 不能依赖 cmx-plugin，需要定义一个轻量级的 `PluginSnapshot` 结构体（在 cmx-traits 中），cmx-plugin 的 `PluginInfo` 实现转换到此类型。

```rust
/// 插件快照信息（跨模块传递的插件元数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSnapshot {
    /// 插件唯一标识
    pub plugin_id: String,
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件状态
    pub status: String,
    /// 安装路径
    pub install_path: String,
    /// WASM 文件路径（相对于安装路径）
    pub wasm_path: Option<String>,
    /// 插件类型
    pub plugin_type: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
}

#[async_trait]
pub trait PluginQuery: Send + Sync {
    /// 根据插件ID查询插件快照
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError>;

    /// 检查插件是否已激活
    async fn is_active(&self, plugin_id: &str) -> Result<bool, TraitError>;

    /// 获取插件的 WASM 文件绝对路径
    async fn get_wasm_path(&self, plugin_id: &str) -> Result<std::path::PathBuf, TraitError>;

    /// 列出所有已激活的插件快照
    async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, TraitError>;

    /// 根据筛选条件查询插件列表
    async fn list_plugins(&self, filter: &PluginFilter) -> Result<Vec<PluginSnapshot>, TraitError>;
}
```

**AI 注意事项：**
- `PluginSnapshot` 是 `PluginInfo` 的轻量子集，仅包含跨模块需要的字段
- 使用 `TraitError`（cmx-traits 自定义错误）而非 `anyhow`，保持类型安全
- `get_wasm_path` 需要返回绝对路径，由实现方拼接 `install_path` + `wasm_path`

### 任务 1.3：定义 RuntimeInvoker trait

**文件**：`src/runtime_invoker.rs`

**设计要点：**
- 此 trait 供 cmx-service 使用，用于调用 WASM 执行
- cmx-runtime 的 `WasmEngine` 将实现此 trait

```rust
/// WASM 调用结果
#[derive(Debug, Clone)]
pub struct WasmInvokeResult {
    /// 返回数据（字节）
    pub output: Vec<u8>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 消耗的燃料（可选）
    pub fuel_consumed: Option<u64>,
}

#[async_trait]
pub trait RuntimeInvoker: Send + Sync {
    /// 调用 WASM 模块的指定导出函数
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError>;

    /// 加载 WASM 模块到运行时
    async fn load_module(
        &self,
        plugin_id: &str,
        wasm_path: &std::path::Path,
    ) -> Result<(), TraitError>;

    /// 从运行时卸载 WASM 模块
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError>;

    /// 检查模块是否已加载
    async fn is_loaded(&self, plugin_id: &str) -> bool;
}
```

### 任务 1.4：定义 PluginLifecycleListener trait

**文件**：`src/lifecycle.rs`

**设计要点：**
- 此 trait 供 cmx-plugin 使用，在插件激活/停用/卸载时通知 cmx-service
- cmx-service 的 `CmxService` 将实现此 trait

```rust
/// 生命周期事件载荷
#[derive(Debug, Clone)]
pub struct LifecycleEvent {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// WASM 文件绝对路径
    pub wasm_path: Option<std::path::PathBuf>,
    /// 事件发生时间
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait PluginLifecycleListener: Send + Sync {
    /// 插件已激活 — 通知监听者加载 WASM 模块
    async fn on_plugin_activated(&self, event: LifecycleEvent);

    /// 插件已停用 — 通知监听者卸载 WASM 模块
    async fn on_plugin_deactivated(&self, event: LifecycleEvent);

    /// 插件已卸载 — 通知监听者清理资源
    async fn on_plugin_uninstalled(&self, event: LifecycleEvent);
}
```

**AI 注意事项：**
- trait 方法不返回 `Result`，监听者内部自行处理错误并记录日志，不应阻塞插件生命周期流程
- 如果监听者处理失败，应 log warn 并继续，不应影响插件操作本身

### 任务 1.5：定义 HostFunctionProvider trait（宿主函数注册）

**文件**：`src/host_func.rs`

**设计要点：**
- 此 trait 供各模块实现，将宿主函数注册到 WASM Linker
- cmx-runtime 提供具体的 `WasmLinker` 实现，cmx-traits 仅定义接口

```rust
/// WASM Linker 抽象接口
///
/// cmx-runtime 提供具体实现，将 cmx-traits 的调用适配到 wasmtime::Linker
pub trait WasmLinker: Send + Sync {
    /// 注册一个宿主函数（带返回值）
    fn define(
        &mut self,
        module: &str,
        name: &str,
        func: HostFuncWrapper,
    ) -> Result<(), HostFuncError>;

    /// 注册一个宿主函数（无返回值，仅副作用）
    fn define_void(
        &mut self,
        module: &str,
        name: &str,
        func: HostVoidFuncWrapper,
    ) -> Result<(), HostFuncError>;
}

/// 带返回值的宿主函数包装器
pub type HostFuncWrapper = Box<dyn Fn(&dyn WasmCallerAccess) -> Result<Vec<u8>, HostFuncError> + Send + Sync>;

/// 无返回值的宿主函数包装器
pub type HostVoidFuncWrapper = Box<dyn Fn(&dyn WasmCallerAccess) -> Result<(), HostFuncError> + Send + Sync>;

/// WASM 调用者访问接口
///
/// 提供宿主函数访问 WASM 内存和调用上下文的能力
pub trait WasmCallerAccess {
    /// 从 WASM 线性内存读取字节
    fn read_memory(&self, offset: u32, len: u32) -> Result<Vec<u8>, HostFuncError>;

    /// 向 WASM 线性内存写入字节
    fn write_memory(&mut self, offset: u32, data: &[u8]) -> Result<(), HostFuncError>;

    /// 分配 WASM 内存并写入数据，返回指针和长度
    fn alloc_and_write(&mut self, data: &[u8]) -> Result<(u32, u32), HostFuncError>;

    /// 获取当前调用上下文
    fn caller_data(&self) -> &CallerData;
}

/// 宿主函数注册器
///
/// 各模块通过实现此 trait，将自身提供的宿主函数注册到 WASM Linker
pub trait HostFunctionProvider: Send + Sync {
    /// 命名空间标识（如 "cmx.database", "cmx.buffer"）
    fn namespace(&self) -> &str;

    /// 向 Linker 注册所有宿主函数
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError>;

    /// 列出该提供者注册的所有函数名（用于调试和元数据查询）
    fn provided_functions(&self) -> Vec<&str> {
        Vec::new()
    }
}
```

### 任务 1.6：定义 CallerData 和错误类型

**文件**：`src/caller_data.rs`

```rust
/// WASM 调用者上下文数据
///
/// 每次从 HTTP 请求触发 WASM 调用时创建，传递给宿主函数使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerData {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID（从插件配置或请求上下文获取）
    pub db_id: String,
    /// 当前事务ID（可选，由宿主函数的事务管理创建）
    pub txn_id: Option<String>,
    /// 请求ID（用于链路追踪）
    pub request_id: String,
    /// 租户ID（多租户隔离，预留）
    pub tenant_id: Option<String>,
    /// 自定义扩展数据
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}
```

**文件**：`src/error.rs`

```rust
/// cmx-traits 统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum TraitError {
    #[error("插件未找到: {0}")]
    PluginNotFound(String),

    #[error("插件未激活: {0}")]
    PluginNotActive(String),

    #[error("WASM 模块加载失败: {0}")]
    WasmLoadFailed(String),

    #[error("WASM 函数调用失败: {0}")]
    WasmInvokeFailed(String),

    #[error("WASM 模块未加载: {0}")]
    WasmNotLoaded(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

/// 宿主函数错误类型
#[derive(Debug, thiserror::Error)]
pub enum HostFuncError {
    #[error("函数注册失败 [{namespace}/{name}]: {reason}")]
    RegistrationFailed {
        namespace: String,
        name: String,
        reason: String,
    },

    #[error("函数执行失败 [{namespace}/{name}]: {reason}")]
    ExecutionFailed {
        namespace: String,
        name: String,
        reason: String,
    },

    #[error("WASM 内存越界访问 (offset={offset}, len={len})")]
    MemoryOutOfBounds { offset: u32, len: u32 },

    #[error("无效参数: {0}")]
    InvalidParam(String),
}
```

### 任务 1.7：组装 lib.rs

**文件**：`src/lib.rs`

```rust
pub mod plugin_query;
pub mod runtime_invoker;
pub mod lifecycle;
pub mod host_func;
pub mod error;
pub mod caller_data;

// 统一导出
pub use plugin_query::{PluginQuery, PluginSnapshot, PluginFilter};
pub use runtime_invoker::{RuntimeInvoker, WasmInvokeResult};
pub use lifecycle::{PluginLifecycleListener, LifecycleEvent};
pub use host_func::{HostFunctionProvider, WasmLinker, WasmCallerAccess, HostFuncWrapper, HostVoidFuncWrapper};
pub use caller_data::CallerData;
pub use error::{TraitError, HostFuncError};
```

### 任务 1.8：验证编译

```bash
cargo check -p cmx-traits
```

### 阶段一检查点 ✅

**验证清单：**
- [ ] `cargo check -p cmx-traits` 编译通过
- [ ] cmx-traits 仅依赖 cmx-core、cmx-utils 及基础第三方库
- [ ] 所有 trait 方法有完整的中文文档注释
- [ ] 无循环依赖

---

## 阶段二：WASM 运行时引擎 — 创建 cmx-runtime crate

### 目标

基于 wasmtime 实现 WASM 运行时引擎，实现 `RuntimeInvoker` 和 `WasmLinker` trait，支持 `HostFunctionProvider` 的注册。

### 任务 2.1：创建 crate 骨架

**操作步骤：**

1. 创建目录结构：
   ```
   crates/libs/cmx-runtime/
   ├── Cargo.toml
   └── src/
       ├── lib.rs
       ├── engine.rs           # WasmEngine 主结构体
       ├── linker_adapter.rs   # RuntimeLinkerAdapter（实现 cmx-traits::WasmLinker）
       ├── caller_adapter.rs   # RuntimeCallerAdapter（实现 cmx-traits::WasmCallerAccess）
       ├── instance.rs         # WasmInstance 包装
       └── error.rs            # 运行时错误类型
   ```

2. 编写 `Cargo.toml`：
   ```toml
   [package]
   name = "cmx-runtime"
   version.workspace = true
   edition.workspace = true

   [dependencies]
   cmx-core = { workspace = true }
   cmx-traits = { workspace = true }
   cmx-utils = { workspace = true }
   wasmtime = { workspace = true }
   tokio = { workspace = true }
   tracing = { workspace = true }
   thiserror = { workspace = true }
   serde = { workspace = true }
   serde_json = { workspace = true }
   ```

3. 在根 `Cargo.toml` 中注册 workspace member 和 dependency

**AI 注意事项：**
- cmx-runtime **不得**依赖 cmx-database、cmx-metadata、cmx-plugin、cmx-buffer、cmx-service
- cmx-runtime 的 wasmtime 依赖使用 workspace 统一版本（当前为 39.0.1）

### 任务 2.2：实现 WasmEngine 核心结构

**文件**：`src/engine.rs`

**设计要点：**
- `WasmEngine` 持有 `wasmtime::Engine`、`wasmtime::Store`、以及 `Vec<Box<dyn HostFunctionProvider>>`
- 提供 `register_provider()` 方法注册宿主函数提供者
- 提供 `build_linker()` 方法创建 wasmtime Linker 并注册所有宿主函数
- 实现 `RuntimeInvoker` trait

**关键实现细节：**

```rust
pub struct WasmEngine {
    /// wasmtime 引擎配置
    engine: wasmtime::Engine,
    /// 已加载的 WASM 实例映射 (plugin_id -> WasmInstance)
    instances: Arc<RwLock<HashMap<String, WasmInstance>>>,
    /// 宿主函数注册器列表
    host_providers: Vec<Box<dyn HostFunctionProvider>>,
    /// 引擎配置
    config: WasmEngineConfig,
}

/// 引擎配置
pub struct WasmEngineConfig {
    /// 默认内存上限（字节）
    pub max_memory_bytes: u64,
    /// 是否启用燃料计量
    pub enable_fuel: bool,
    /// 最大燃料量
    pub max_fuel: u64,
    /// WASI 是否启用（预留）
    pub enable_wasi: bool,
}
```

**AI 注意事项：**
- `wasmtime::Engine` 配置应开启 `cranelift` 编译器（默认）和 `epoch_interruption`（支持实例终止）
- 每个 WASM 实例应使用独立的 `wasmtime::Store`，避免不同插件之间的状态干扰
- `CallerData` 需要通过 `wasmtime::Store` 的 `data` 字段传递给宿主函数

### 任务 2.3：实现 RuntimeLinkerAdapter

**文件**：`src/linker_adapter.rs`

**设计要点：**
- 实现 `cmx_traits::WasmLinker` trait
- 内部持有 `&mut wasmtime::Linker<WasmStoreData>`
- 将 `HostFuncWrapper`（闭包）适配为 `wasmtime::Func` 的参数签名

**核心挑战：** wasmtime 的 `Linker::define_func()` 需要指定具体的参数和返回值类型签名。由于 cmx-traits 使用类型擦除的闭包，adapter 需要统一使用 `(i32, i32, i32) -> i32` 等通用签名，在闭包内部进行参数解析。

```rust
/// WASM 通用函数签名约定
///
/// 所有宿主函数统一使用以下签名与 WASM 交互：
/// - 参数通过指针+长度传递（input_ptr: i32, input_len: i32）
/// - 返回值为输出缓冲区指针（通过 alloc_and_write 写入）
/// - 错误通过返回负值表示
const HOST_FUNC_SIG: (wasmtime::ValType, wasmtime::ValType) = (
    wasmtime::ValType::I32,  // 参数指针
    wasmtime::ValType::I32,  // 参数长度
);
```

**AI 注意事项：**
- wasmtime `Func` 的创建需要在 `Store` 存在的上下文中进行
- 考虑使用 `wasmtime::FuncType::new()` 动态构建函数类型
- 宿主函数闭包需要通过 `Caller` 访问 `Store` data 中的 `CallerData`

### 任务 2.4：实现 RuntimeCallerAdapter

**文件**：`src/caller_adapter.rs`

**设计要点：**
- 实现 `cmx_traits::WasmCallerAccess` trait
- 内部持有 `wasmtime::Caller<'_, WasmStoreData>` 的引用
- `read_memory`/`write_memory` 通过 `caller.get_export("memory")` 获取线性内存

```rust
/// Store 的数据类型
pub struct WasmStoreData {
    /// 当前调用上下文
    pub caller_data: CallerData,
}
```

### 任务 2.5：实现 WasmInstance 包装

**文件**：`src/instance.rs`

```rust
/// WASM 实例包装
pub struct WasmInstance {
    /// 插件ID
    pub plugin_id: String,
    /// wasmtime Instance
    instance: wasmtime::Instance,
    /// 关联的 Store（需要保持生命周期）
    store: wasmtime::Store<WasmStoreData>,
    /// 模块信息
    module_info: WasmModuleInfo,
}

/// 模块元信息
pub struct WasmModuleInfo {
    /// 导出函数列表
    pub exports: Vec<String>,
    /// 模块哈希（用于缓存）
    pub hash: Option<String>,
    /// 加载时间
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}
```

### 任务 2.6：实现 RuntimeInvoker trait

在 `engine.rs` 中为 `WasmEngine` 实现 `cmx_traits::RuntimeInvoker`：

```rust
#[async_trait]
impl RuntimeInvoker for WasmEngine {
    async fn invoke(...) -> Result<WasmInvokeResult, TraitError> {
        // 1. 从 instances 中获取目标插件实例
        // 2. 获取导出函数
        // 3. 将 input 写入 WASM 内存
        // 4. 设置 CallerData 到 Store data
        // 5. 调用函数
        // 6. 读取返回数据
        // 7. 返回 WasmInvokeResult
    }

    async fn load_module(...) -> Result<(), TraitError> {
        // 1. 创建新 Store
        // 2. build_linker() 创建 Linker
        // 3. 编译 Module
        // 4. 实例化 Instance
        // 5. 保存到 instances HashMap
    }

    async fn unload_module(...) -> Result<(), TraitError> {
        // 1. 从 instances 中移除
        // 2. Store 会自动 drop 释放资源
    }
}
```

### 任务 2.7：实现 GlobalWasmEngine 单例

**文件**：`src/lib.rs`

```rust
/// 全局 WASM 引擎单例
pub struct GlobalWasmEngine;

static GLOBAL_WASM_ENGINE: OnceLock<Arc<RwLock<WasmEngine>>> = OnceLock::new();

impl GlobalWasmEngine {
    pub async fn initialize(config: WasmEngineConfig) -> Result<(), RuntimeError> { ... }
    pub async fn get() -> RwLockReadGuard<'static, WasmEngine> { ... }
    pub async fn get_mut() -> RwLockWriteGuard<'static, WasmEngine> { ... }
    pub fn get_arc() -> Arc<RwLock<WasmEngine>> { ... }
}
```

### 任务 2.8：验证编译

```bash
cargo check -p cmx-runtime
```

### 阶段二检查点 ✅

**验证清单：**
- [ ] `cargo check -p cmx-runtime` 编译通过
- [ ] cmx-runtime 仅依赖 cmx-core、cmx-traits、cmx-utils、wasmtime
- [ ] `RuntimeLinkerAdapter` 实现了 `cmx_traits::WasmLinker`
- [ ] `WasmEngine` 实现了 `cmx_traits::RuntimeInvoker`
- [ ] `GlobalWasmEngine` 单例模式可用

---

## 阶段三：宿主函数适配层 — 为各模块实现 HostFunctionProvider

### 目标

在 cmx-database、cmx-buffer、cmx-plugin、cmx-utils 中各新增一个 `host_functions.rs` 模块，实现 `HostFunctionProvider` trait。

### 任务 3.1：cmx-database — DatabaseHostFunctions

**文件**：`crates/libs/cmx-infra/cmx-database/src/host_functions.rs`

**依赖变更**：cmx-database 的 `Cargo.toml` 添加 `cmx-traits = { workspace = true }`

**要封装的现有 API（来自 `transaction/api.rs`）：**
- `execute_sql(db_id, txn_id, sql) -> Result<...>`
- `query_sql(db_id, txn_id, sql) -> Result<Vec<...>>`
- `execute_sql_with_params(db_id, txn_id, sql, params) -> Result<...>`
- `query_sql_with_params(db_id, txn_id, sql, params) -> Result<Vec<...>>`
- `begin_transaction_by_id(db_id) -> Result<String>` (返回 txn_id)
- `commit_txn_by_id(txn_id) -> Result<()>`
- `rollback_txn_by_id(txn_id) -> Result<()>`

**注册的宿主函数：**

| 宿主函数名 | 封装的 API | 说明 |
|-----------|-----------|------|
| `cmx:database/execute_sql` | `execute_sql` | 执行写操作 SQL |
| `cmx:database/query_sql` | `query_sql` | 执行查询 SQL |
| `cmx:database/execute_sql_with_params` | `execute_sql_with_params` | 参数化写操作 |
| `cmx:database/query_sql_with_params` | `query_sql_with_params` | 参数化查询 |
| `cmx:database/txn/begin` | `begin_transaction_by_id` | 开启事务 |
| `cmx:database/txn/commit` | `commit_txn_by_id` | 提交事务 |
| `cmx:database/txn/rollback` | `rollback_txn_by_id` | 回滚事务 |

**AI 注意事项：**
- 宿主函数闭包内部需要从 `CallerData` 获取 `db_id` 和 `txn_id`
- SQL 字符串通过 WASM 内存指针+长度传递
- 查询结果需要序列化为 JSON 后写入 WASM 内存
- 事务操作的结果（如 txn_id）需要通过 WASM 内存返回
- **不要修改**现有的 `transaction/api.rs` 中的函数签名

### 任务 3.2：cmx-buffer — BufferHostFunctions

**文件**：`crates/libs/cmx-infra/cmx-buffer/src/host_functions.rs`

**依赖变更**：cmx-buffer 的 `Cargo.toml` 添加 `cmx-traits = { workspace = true }`

**要封装的现有 API（来自 Redis 客户端）：**
- `cache_get(key) -> Option<Vec<u8>>`
- `cache_set(key, value, ttl) -> Result<()>`
- `cache_delete(key) -> Result<()>`
- 分布式锁操作（可选）

**注册的宿主函数：**

| 宿主函数名 | 说明 |
|-----------|------|
| `cmx:buffer/cache_get` | 读取缓存 |
| `cmx:buffer/cache_set` | 写入缓存 |
| `cmx:buffer/cache_delete` | 删除缓存 |

**AI 注意事项：**
- 需要 `Arc<CacheManager>` 引用，通过构造函数注入
- key 使用 `CallerData.plugin_id` 作为前缀，实现插件间缓存隔离

### 任务 3.3：cmx-plugin — PluginHostFunctions

**文件**：`crates/libs/cmx-plugin/src/host_functions.rs`

**依赖变更**：cmx-plugin 的 `Cargo.toml` 添加 `cmx-traits = { workspace = true }`

**要封装的现有 API（来自 PluginManager / ServiceRegistry）：**
- 插件间调用：通过 `ServiceRegistry` 查找目标插件并调用
- 获取插件信息

**注册的宿主函数：**

| 宿主函数名 | 说明 |
|-----------|------|
| `cmx:plugin/call_service` | 调用另一个插件的服务 |
| `cmx:plugin/get_info` | 获取当前插件信息 |

**AI 注意事项：**
- 插件间调用需要通过 cmx-runtime 的 `invoke` 方法中转，宿主函数内部调用 `RuntimeInvoker`
- 这意味着 `PluginHostFunctions` 需要持有 `Arc<dyn RuntimeInvoker>` 引用

### 任务 3.4：cmx-utils — LoggingHostFunctions

**文件**：`crates/libs/cmx-utils/src/host_functions.rs`

**依赖变更**：cmx-utils 的 `Cargo.toml` 添加 `cmx-traits = { workspace = true }`

**注册的宿主函数：**

| 宿主函数名 | 说明 |
|-----------|------|
| `cmx:log/info` | 记录 info 级别日志 |
| `cmx:log/warn` | 记录 warn 级别日志 |
| `cmx:log/error` | 记录 error 级别日志 |

**AI 注意事项：**
- 日志消息从 WASM 内存读取
- 自动附加 `plugin_id` 前缀到日志消息中

### 任务 3.5：各 crate 导出并在 lib.rs 中注册模块

每个 crate 的 `lib.rs` 中添加 `pub mod host_functions;` 并导出关键类型。

### 任务 3.6：验证编译

```bash
cargo check -p cmx-database -p cmx-buffer -p cmx-plugin -p cmx-utils
```

### 阶段三检查点 ✅

**验证清单：**
- [ ] 所有实现了 `HostFunctionProvider` 的 crate 编译通过
- [ ] 每个 Provider 的 `namespace()` 返回值符合命名约定
- [ ] 每个 Provider 的 `provided_functions()` 返回完整的函数名列表
- [ ] 宿主函数闭包正确捕获了所需的 `Arc<>` 引用

---

## 阶段四：企业服务层 — 创建 cmx-service crate

### 目标

实现企业级通用服务层，处理 cmx-api 请求、解析插件编排、调用 cmx-runtime 执行 WASM。

### 任务 4.1：创建 crate 骨架

**目录结构：**
```
crates/libs/cmx-service/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── service.rs           # CmxService 主结构体
    ├── orchestrator.rs      # 插件编排解析器
    ├── handler.rs           # HTTP Handler（供 cmx-api 调用）
    ├── request.rs           # 请求/响应类型定义
    └── error.rs             # 错误类型
```

**Cargo.toml：**
```toml
[package]
name = "cmx-service"
version.workspace = true
edition.workspace = true

[dependencies]
cmx-core = { workspace = true }
cmx-traits = { workspace = true }
cmx-runtime = { workspace = true }
cmx-database = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }
```

**AI 注意事项：**
- cmx-service **不得**依赖 cmx-plugin（通过 trait 交互）
- cmx-service 依赖 cmx-database（可能需要直接执行编排中的 SQL）
- cmx-service 依赖 cmx-runtime（调用 WASM 执行）

### 任务 4.2：实现 CmxService 核心结构

**文件**：`src/service.rs`

```rust
/// 企业级通用服务
///
/// 作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker 完成请求处理
pub struct CmxService {
    /// 插件查询器（trait 对象，由 web-server 注入）
    plugin_query: Arc<dyn PluginQuery>,
    /// WASM 运行时调用器（trait 对象，由 web-server 注入）
    runtime: Arc<dyn RuntimeInvoker>,
    /// 服务配置
    config: ServiceConfig,
}

/// 服务配置
pub struct ServiceConfig {
    /// 默认调用超时（毫秒）
    pub invoke_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否启用编排缓存
    pub enable_orchestration_cache: bool,
}
```

### 任务 4.3：实现 PluginLifecycleListener

```rust
#[async_trait]
impl PluginLifecycleListener for CmxService {
    async fn on_plugin_activated(&self, event: LifecycleEvent) {
        // 插件激活时，加载 WASM 模块到运行时
        if let Some(wasm_path) = &event.wasm_path {
            match self.runtime.load_module(&event.plugin_id, wasm_path).await {
                Ok(_) => info!("插件 {} WASM 模块加载成功", event.plugin_id),
                Err(e) => warn!("插件 {} WASM 模块加载失败: {}", event.plugin_id, e),
            }
        }
    }

    async fn on_plugin_deactivated(&self, event: LifecycleEvent) {
        // 插件停用时，卸载 WASM 模块
        match self.runtime.unload_module(&event.plugin_id).await {
            Ok(_) => info!("插件 {} WASM 模块卸载成功", event.plugin_id),
            Err(e) => warn!("插件 {} WASM 模块卸载失败: {}", event.plugin_id, e),
        }
    }

    async fn on_plugin_uninstalled(&self, event: LifecycleEvent) {
        // 插件卸载时，先卸载 WASM 模块再清理
        let _ = self.runtime.unload_module(&event.plugin_id).await;
        info!("插件 {} 资源已清理", event.plugin_id);
    }
}
```

### 任务 4.4：实现编排解析器

**文件**：`src/orchestrator.rs`

```rust
/// 编排步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationStep {
    /// 步骤ID
    pub step_id: String,
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON 或引用前序步骤输出）
    pub input: StepInput,
    /// 是否并行执行
    pub parallel: bool,
}

/// 编排定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Orchestration {
    /// 编排ID
    pub id: String,
    /// 编排名称
    pub name: String,
    /// 编排步骤列表（有序）
    pub steps: Vec<OrchestrationStep>,
}

/// 编排执行器
pub struct Orchestrator {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
}

impl Orchestrator {
    /// 执行编排
    pub async fn execute(
        &self,
        orchestration: &Orchestration,
        caller_data: &CallerData,
    ) -> Result<OrchestrationResult, ServiceError>;
}
```

### 任务 4.5：定义请求/响应类型

**文件**：`src/request.rs`

```rust
/// 通用服务调用请求
#[derive(Debug, Deserialize)]
pub struct ServiceCallRequest {
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON）
    pub input: serde_json::Value,
    /// 调用选项
    pub options: Option<CallOptions>,
}

/// 调用选项
#[derive(Debug, Deserialize)]
pub struct CallOptions {
    /// 超时（毫秒）
    pub timeout_ms: Option<u64>,
    /// 数据库ID（覆盖默认值）
    pub db_id: Option<String>,
}

/// 服务调用响应
#[derive(Debug, Serialize)]
pub struct ServiceCallResponse {
    /// 输出数据（JSON）
    pub output: serde_json::Value,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 是否成功
    pub success: bool,
    /// 错误信息（失败时）
    pub error: Option<String>,
}
```

### 任务 4.6：实现 GlobalCmxService 单例

与 `GlobalPluginManager`、`GlobalWasmEngine` 保持一致的单例模式。

### 任务 4.7：验证编译

```bash
cargo check -p cmx-service
```

### 阶段四检查点 ✅

**验证清单：**
- [ ] `cargo check -p cmx-service` 编译通过
- [ ] cmx-service 不依赖 cmx-plugin
- [ ] `CmxService` 实现了 `PluginLifecycleListener`
- [ ] 编排解析器支持串行和并行步骤执行

---

## 阶段五：重构 cmx-plugin — 接入 trait 接口

### 目标

让 PluginManager 实现 `PluginQuery` trait，注入 `PluginLifecycleListener`，实现 `PluginHostFunctions`。

### 任务 5.1：为 PluginManager 实现 PluginQuery trait

**文件**：`crates/libs/cmx-plugin/src/core/manager.rs`（末尾添加 impl 块）

**关键映射关系：**

| PluginQuery 方法 | 现有 PluginManager 方法 | 说明 |
|-----------------|------------------------|------|
| `get_plugin(id)` | `get_plugin(id)` (已有) | 需要将 `PluginInfo` 转换为 `PluginSnapshot` |
| `is_active(id)` | `activation_manager.is_active(id)` | 直接委托 |
| `get_wasm_path(id)` | `get_plugin(id)` + install_path + wasm_path | 拼接绝对路径 |
| `list_active_plugins()` | `activation_manager.get_active_plugins()` + 查询完整信息 | 组合查询 |
| `list_plugins(filter)` | `repository.list_plugins(filter)` | 转换过滤器和结果类型 |

**AI 注意事项：**
- 需要添加 `PluginInfo -> PluginSnapshot` 的转换实现（建议实现 `From<PluginInfo> for PluginSnapshot`）
- `PluginFilter` 也需要在 cmx-traits 中定义轻量版本，并在 cmx-plugin 中实现转换
- **不要删除**现有 `PluginManager` 的任何公开方法，保持向后兼容

### 任务 5.2：在 PluginManager 中注入 PluginLifecycleListener

**修改文件**：
- `crates/libs/cmx-plugin/src/core/manager.rs` — 在 `PluginManager` 结构体中添加字段
- `crates/libs/cmx-plugin/src/core/manager.rs` — 在 `PluginManagerBuilder` 中添加 `with_lifecycle_listener` 方法
- `crates/libs/cmx-plugin/src/service/activate.rs` — 在激活/停用流程中调用监听器

**具体修改：**

1. 在 `PluginManager` 结构体中添加：
   ```rust
   /// 生命周期事件监听器（可选）
   lifecycle_listener: Option<Arc<dyn PluginLifecycleListener>>,
   ```

2. 在 `PluginManagerBuilder` 中添加：
   ```rust
   /// 设置生命周期事件监听器
   pub fn with_lifecycle_listener(mut self, listener: Arc<dyn PluginLifecycleListener>) -> Self {
       self.lifecycle_listener = Some(listener);
       self
   }
   ```

3. 在 `ActivateService` 的激活流程末尾，添加通知逻辑：
   ```rust
   // 激活成功后，通知监听器
   if let Some(listener) = &self.lifecycle_listener {
       listener.on_plugin_activated(LifecycleEvent {
           plugin_id: plugin_id.clone(),
           version: version.clone(),
           wasm_path: Some(wasm_abs_path),
           timestamp: chrono::Utc::now(),
       }).await;
   }
   ```

4. 在停用和卸载流程中类似地添加通知。

**AI 注意事项：**
- `lifecycle_listener` 的通知是 fire-and-forget，不应阻塞主流程
- 可以考虑使用 `tokio::spawn` 异步执行通知，但要注意日志可观测性
- 监听器为 `Option`，为 `None` 时跳过通知（保持向后兼容）

### 任务 5.3：实现 PluginHostFunctions

**文件**：`crates/libs/cmx-plugin/src/host_functions.rs`（已在阶段三创建骨架）

完善实现，需要注入 `Arc<dyn RuntimeInvoker>` 以支持插件间调用。

### 任务 5.4：更新 GlobalPluginManager 支持新依赖

在 `GlobalPluginManager::initialize_with_deps` 中添加 `lifecycle_listener` 参数。

**AI 注意事项：**
- 保持 `initialize()` 方法向后兼容（lifecycle_listener 为 None）
- 新增 `initialize_with_listener()` 方法或在 `initialize_with_deps` 中添加参数

### 任务 5.5：验证编译

```bash
cargo check -p cmx-plugin
```

### 阶段五检查点 ✅

**验证清单：**
- [ ] `PluginManager` 实现了 `PluginQuery` trait
- [ ] `PluginManagerBuilder` 支持 `with_lifecycle_listener()`
- [ ] 激活/停用/卸载流程正确通知 `PluginLifecycleListener`
- [ ] 现有功能不受影响（向后兼容）

---

## 阶段六：重构 cmx-api — 接入服务层

### 目标

新增 cmx-service 相关的 HTTP Handler，将现有的 `GlobalPluginManager` 直接调用逐步改为通过 AppState 注入。

### 任务 6.1：扩展 CmxAppState

**文件**：`crates/libs/cmx-api/src/app_state.rs`

```rust
/// CMX 应用程序状态（扩展版）
#[derive(Debug, Clone)]
pub struct CmxAppState {
    /// 内部可修改的状态
    pub app_state: Arc<RwLock<AppStateInner>>,
    /// 插件查询器（trait 对象）
    pub plugin_query: Arc<dyn PluginQuery>,
    /// WASM 运行时调用器（trait 对象）
    pub runtime_invoker: Arc<dyn RuntimeInvoker>,
}
```

**AI 注意事项：**
- `CmxAppState` 需要实现 `Clone`（通过 `Arc` 实现），因为 Axum 要求 State 是 Clone 的
- trait 对象需要 `Send + Sync + 'static`，确保所有实现满足这些约束

### 任务 6.2：新增 cmx-service Handler

**文件**：`crates/libs/cmx-api/src/handlers/service/mod.rs` + `handler.rs`

```rust
/// 服务调用 Handler
pub async fn service_call(
    State(state): State<CmxAppState>,
    Json(req): Json<ServiceCallRequest>,
) -> Result<Json<ApiResp<ServiceCallResponse>>> {
    // 1. 构建 CallerData
    // 2. 调用 CmxService
    // 3. 返回结果
}
```

### 任务 6.3：在路由中注册新 Handler

**文件**：`crates/libs/cmx-api/src/routes/routes.rs`

```rust
// 注册通用服务路由
.route("/service/call", post(service::service_call))
.route("/service/orchestration", post(service::execute_orchestration))
```

### 任务 6.4：（可选）将现有 plugin handler 改为通过 trait 调用

**当前代码**（需要修改的模式）：
```rust
// 旧方式：直接调用全局单例
let manager = cmx_plugin::GlobalPluginManager::get().await;
```

**新方式**（渐进式迁移）：
```rust
// 新方式：通过 AppState 中的 trait 调用
let plugin = state.plugin_query.get_plugin(&plugin_id).await?;
```

**AI 注意事项：**
- **阶段六暂不强制修改**现有 plugin handler，保持渐进式迁移
- 可以在后续阶段逐步替换，避免一次性变更过大
- 新增的 cmx-service handler 使用新方式

### 任务 6.5：验证编译

```bash
cargo check -p cmx-api
```

### 阶段六检查点 ✅

**验证清单：**
- [ ] `CmxAppState` 包含 `plugin_query` 和 `runtime_invoker` 字段
- [ ] 新的 service handler 注册到路由
- [ ] 现有 plugin handler 不受影响

---

## 阶段七：重构 web-server — 统一组装层

### 目标

在 web-server 的初始化阶段统一组装所有模块的依赖关系，注册宿主函数，注入 trait 实现。

### 任务 7.1：重写初始化流程

**文件**：`crates/web/web-server/src/config.rs`

**新增初始化函数**（保留现有函数，新增以下函数）：

```rust
/// 初始化 WASM 运行时引擎
///
/// 创建 WasmEngine，注册所有宿主函数提供者，初始化全局单例
pub async fn init_runtime() {
    let config = cmx_runtime::WasmEngineConfig {
        max_memory_bytes: 256 * 1024 * 1024,  // 256MB
        enable_fuel: true,
        max_fuel: 1_000_000_000,
        enable_wasi: false,
    };

    // 使用带 builder 的初始化方式
    cmx_runtime::GlobalWasmEngine::initialize_builder(config)
        .register_provider(Box::new(
            cmx_database::host_functions::DatabaseHostFunctions::new(
                cmx_database::get_default_db_manager().clone()
            )
        ))
        .register_provider(Box::new(
            cmx_buffer::host_functions::BufferHostFunctions::new(
                cmx_buffer::GlobalCacheManager::get_arc()
            )
        ))
        .register_provider(Box::new(
            cmx_utils::host_functions::LoggingHostFunctions::new()
        ))
        // cmx_plugin 的 HostFunctions 需要 RuntimeInvoker，在 CmxService 创建后再注册
        .build()
        .await
        .expect("WASM 运行时初始化失败");

    info!("WASM 运行时引擎初始化完成");
}
```

**AI 注意事项：**
- `cmx_plugin::PluginHostFunctions` 依赖 `RuntimeInvoker`，存在循环依赖：
  `cmx-plugin -> cmx-traits(HostFunctionProvider) -> WasmLinker -> cmx-runtime -> WasmEngine`
  而 `cmx-plugin` 的宿主函数内部又需要调用 `RuntimeInvoker`
- **解决方案**：使用 `Arc<dyn RuntimeInvoker>` 延迟注入。先创建 WasmEngine，再创建 PluginHostFunctions（传入 `Arc<WasmEngine>`），最后通过 `WasmEngine::register_provider_late()` 追加注册

### 任务 7.2：重写 init_plugins 函数

```rust
pub async fn init_plugins() {
    use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
    use cmx_traits::PluginLifecycleListener;

    // 1. 创建 CmxService（先于 PluginManager）
    let runtime_arc = cmx_runtime::GlobalWasmEngine::get_arc();
    let service = Arc::new(cmx_service::CmxService::new(
        /* plugin_query 稍后注入 */
        runtime_arc.clone(),
        cmx_service::ServiceConfig::default(),
    ));

    // 2. 初始化 PluginManager，注入生命周期监听器
    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let settings = PluginManagerSettings {
        plugin_root: PathBuf::new().join("plugins").join("root"),
        backup_root: PathBuf::new().join("plugins").join("backup"),
        temp_root: PathBuf::new().join("plugins").join("temp"),
        default_database_id: default_db_id,
        node_id: ConfigManager::global().get_string("node.node_id")
            .unwrap_or("default".to_string()),
        ..Default::default()
    };

    GlobalPluginManager::initialize_with_listener(settings, service.clone())
        .await
        .unwrap_or_else(|e| panic!("初始化插件管理器失败: {:?}", e));

    // 3. 将 PluginManager（实现了 PluginQuery）注入到 CmxService
    let plugin_manager_arc = GlobalPluginManager::get_arc();
    service.set_plugin_query(plugin_manager_arc).await;

    // 4. 注册 PluginHostFunctions（需要 RuntimeInvoker）
    let plugin_host_funcs = cmx_plugin::host_functions::PluginHostFunctions::new(
        runtime_arc,
    );
    cmx_runtime::GlobalWasmEngine::get_mut().await
        .register_provider(Box::new(plugin_host_funcs));

    info!("插件管理器初始化完成（已注入生命周期监听器）");
}
```

### 任务 7.3：修改 main.rs 的初始化顺序和 AppState 构建

**文件**：`crates/web/web-server/src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // ... 日志初始化不变 ...

    init_global_config();
    init_db_datasource().await;
    init_cache().await;

    // 新增：初始化 WASM 运行时（在插件管理器之前）
    init_runtime().await;

    // 修改：使用新的 init_plugins（内部会创建 CmxService）
    init_plugins().await;

    let web_config = web_config();

    // 构建 AppState（包含 trait 实例）
    let app_state = CmxAppState::new()
        .with_plugin_query(GlobalPluginManager::get_arc())
        .with_runtime_invoker(cmx_runtime::GlobalWasmEngine::get_arc());

    // 路由使用新的 AppState
    let api_routes = routes::routes().with_state(app_state);
    // ... 其余不变 ...
}
```

### 任务 7.4：在 web-server 的 Cargo.toml 添加新依赖

```toml
cmx-traits = { workspace = true }
cmx-runtime = { workspace = true }
cmx-service = { workspace = true }
```

### 任务 7.5：全量编译验证

```bash
cargo build
```

### 阶段七检查点 ✅

**验证清单：**
- [ ] `cargo build` 全量编译通过
- [ ] WASM 引擎在插件管理器之前初始化
- [ ] `CmxService` 同时持有 `PluginQuery` 和 `RuntimeInvoker` 引用
- [ ] `PluginManager` 的生命周期事件正确通知 `CmxService`
- [ ] 所有宿主函数提供者已注册到 `WasmEngine`
- [ ] web-server 启动后 `AppState` 包含完整的 trait 实现

---

## 阶段八：集成测试与验证

### 目标

确保解耦后的系统功能正确、性能可接受、可独立测试。

### 任务 8.1：编写 cmx-traits 的 mock 实现

创建 `crates/libs/cmx-traits/src/mock.rs`（仅测试时启用）：

```rust
#[cfg(feature = "mock")]
pub struct MockPluginQuery { ... }

#[cfg(feature = "mock")]
#[async_trait]
impl PluginQuery for MockPluginQuery {
    // 返回预设的测试数据
}
```

### 任务 8.2：编写 cmx-runtime 的单元测试

测试用例覆盖：
- [ ] `WasmEngine::register_provider` 正确注册
- [ ] `WasmEngine::build_linker` 正确调用所有 provider
- [ ] `WasmEngine::load_module` / `unload_module` 生命周期
- [ ] `WasmEngine::invoke` 正确调用 WASM 导出函数
- [ ] `RuntimeLinkerAdapter` 正确适配 `WasmLinker` trait

**测试用 WASM 模块：** 需要准备一个简单的 `.wasm` 文件用于测试（可以用 Rust 编写一个简单的 `#[no_mangle]` 函数编译为 wasm32 目标）。

### 任务 8.3：编写 cmx-service 的集成测试

测试用例覆盖：
- [ ] `CmxService::call` 端到端调用流程
- [ ] `Orchestrator::execute` 编排执行（串行/并行）
- [ ] `PluginLifecycleListener` 事件通知流程
- [ ] 错误处理（插件不存在、WASM 加载失败、函数调用超时）

### 任务 8.4：编写端到端测试

测试用例覆盖：
- [ ] 启动 web-server -> 安装插件 -> 激活插件 -> 调用服务 -> 停用插件 -> 卸载插件
- [ ] 宿主函数在 WASM 内部正确调用（数据库查询、缓存读写）
- [ ] 插件间调用（通过 `cmx:plugin/call_service`）

### 任务 8.5：编译速度验证

```bash
# 修改 cmx-plugin 的某个文件后，检查哪些 crate 需要重编译
cargo check -p cmx-plugin 2>&1 | grep "Compiling"
# 预期：cmx-runtime 和 cmx-service 不应被重编译
```

### 任务 8.6：依赖关系静态检查

```bash
# 使用 cargo depgraph 或手动检查确保无循环依赖
cargo tree -p cmx-traits
cargo tree -p cmx-runtime
cargo tree -p cmx-service
```

**预期依赖树：**
```
cmx-traits
├── cmx-core
└── cmx-utils

cmx-runtime
├── cmx-core
├── cmx-traits
│   ├── cmx-core
│   └── cmx-utils
└── cmx-utils

cmx-service
├── cmx-core
├── cmx-traits
│   ├── cmx-core
│   └── cmx-utils
├── cmx-runtime
│   ├── cmx-core
│   ├── cmx-traits
│   └── cmx-utils
└── cmx-database
    ├── cmx-core
    └── cmx-utils

cmx-plugin
├── cmx-core
├── cmx-traits  ← 新增
├── cmx-metadata
│   ├── cmx-core
│   └── cmx-database
├── cmx-buffer
│   └── cmx-utils
├── cmx-database
│   ├── cmx-core
│   └── cmx-utils
└── cmx-utils
```

### 阶段八检查点 ✅

**验证清单：**
- [ ] 所有单元测试通过
- [ ] 端到端测试通过
- [ ] 修改 cmx-plugin 不会触发 cmx-runtime/cmx-service 重编译
- [ ] 无循环依赖
- [ ] 现有功能不受影响

---

## 潜在风险评估与应对措施

### 风险1：wasmtime Linker 适配复杂度高

**概率**：高 | **影响**：中

**描述**：`RuntimeLinkerAdapter` 需要将类型擦除的 `HostFuncWrapper` 适配为 wasmtime 的强类型 `Func`。wasmtime 的 `Linker::define_func()` 要求明确的参数和返回值类型，而 cmx-traits 使用 `Box<dyn Fn>` 进行类型擦除。

**应对措施：**
1. 采用统一的函数签名约定：所有宿主函数使用 `(i32, i32) -> i32` 签名（输入指针+长度，返回输出指针）
2. 在 adapter 内部做参数打包/解包
3. 如果适配过于复杂，备选方案是让 `WasmLinker` trait 直接暴露 wasmtime 类型（牺牲 cmx-traits 的 wasmtime 无关性）

### 风险2：全局单例初始化顺序

**概率**：中 | **影响**：高

**描述**：`WasmEngine`、`PluginManager`、`CmxService` 三者存在循环初始化依赖：
- `CmxService` 需要 `RuntimeInvoker`（来自 WasmEngine）
- `CmxService` 需要 `PluginQuery`（来自 PluginManager）
- `PluginManager` 需要 `PluginLifecycleListener`（来自 CmxService）
- `PluginHostFunctions` 需要 `RuntimeInvoker`（来自 WasmEngine）

**应对措施：**
1. 分步初始化：先创建 WasmEngine -> 再创建 CmxService（暂不设置 PluginQuery）-> 再创建 PluginManager（注入 CmxService）-> 再回填 CmxService 的 PluginQuery
2. 使用 `Arc` + 内部可变性（`RwLock` 或 `OnceCell`）支持延迟设置
3. 具体方案见阶段七任务 7.2 的代码

### 风险3：现有功能回归

**概率**：低 | **影响**：高

**描述**：重构 cmx-plugin 和 cmx-api 可能引入回归 bug，影响现有的插件安装/卸载/激活等核心功能。

**应对措施：**
1. **渐进式迁移**：保留 `GlobalPluginManager` 和所有现有公开 API，新功能通过 trait 接口新增
2. **现有 handler 不强制修改**：阶段六明确标注现有 plugin handler 暂不修改
3. **充分测试**：阶段八要求端到端测试覆盖所有核心流程
4. **Git 分支策略**：建议在独立分支上开发，合并前进行完整回归测试

### 风险4：宿主函数内存安全

**概率**：中 | **影响**：高

**描述**：宿主函数通过指针访问 WASM 线性内存，越界访问会导致 panic 或内存安全问题。

**应对措施：**
1. 在 `RuntimeCallerAdapter` 中严格检查所有内存访问的边界
2. 使用 wasmtime 的 `Memory::data_size()` 获取实际内存大小
3. 对 WASM 模块设置内存上限（`WasmEngineConfig.max_memory_bytes`）
4. 宿主函数中使用 `catch_unwind` 防止 panic 扩散

### 风险5：cmx-plugin 的 PluginHostFunctions 循环依赖

**概率**：中 | **影响**：中

**描述**：`PluginHostFunctions` 需要调用 `RuntimeInvoker` 来实现插件间调用，但 `RuntimeInvoker` 是由 `cmx-runtime` 提供的。cmx-plugin 已经依赖了 cmx-traits，而 cmx-runtime 也依赖 cmx-traits。如果 cmx-plugin 的宿主函数需要 cmx-runtime 的类型，就会产生额外依赖。

**应对措施：**
1. `PluginHostFunctions` 通过 `Arc<dyn RuntimeInvoker>` trait 对象引用，不需要依赖 cmx-runtime crate
2. 在 web-server 初始化时，将 `Arc<WasmEngine>` 转为 `Arc<dyn RuntimeInvoker>` 后传入
3. cmx-plugin 仅依赖 cmx-traits（trait 定义），不依赖 cmx-runtime（具体实现）

---

## 测试策略

### 单元测试

| 模块 | 测试重点 | Mock 方式 |
|------|---------|-----------|
| cmx-traits | 编译检查（纯 trait 定义） | 无需 mock |
| cmx-runtime | Linker 适配、实例生命周期、函数调用 | Mock HostFunctionProvider |
| cmx-service | 编排解析、服务调用、事件处理 | Mock PluginQuery + RuntimeInvoker |
| cmx-database/host_functions | 参数提取、API 调用、结果序列化 | Mock WasmCallerAccess |
| cmx-plugin | PluginQuery impl、生命周期通知 | Mock PluginLifecycleListener |

### 集成测试

| 测试场景 | 验证内容 |
|---------|---------|
| 插件激活 -> WASM 加载 | PluginManager 激活后 CmxService 收到通知并加载 WASM |
| HTTP 请求 -> 服务调用 -> WASM 执行 | 端到端请求链路完整 |
| 宿主函数调用 | WASM 内部调用 cmx:database/execute_sql 成功 |
| 插件间调用 | 插件A 通过 cmx:plugin/call_service 调用插件B |
| 错误恢复 | WASM 执行失败不影响宿主进程稳定性 |

### 编译隔离测试

```bash
# 触发 cmx-plugin 重编译
touch crates/libs/cmx-plugin/src/domain/plugin.rs
cargo check 2>&1 | grep "Compiling cmx-runtime\|Compiling cmx-service"
# 预期：无输出（cmx-runtime 和 cmx-service 不应重编译）
```

---

## 实施顺序总结

```
阶段一 [cmx-traits]  ──→  阶段二 [cmx-runtime]  ──→  阶段三 [宿主函数适配]
                                                              │
                                                              ▼
阶段七 [web-server 组装]  ←──  阶段六 [cmx-api 接入]  ←──  阶段四 [cmx-service]
        │
        ▼
   阶段八 [测试验证]

注：阶段五 [cmx-plugin 重构] 可以在阶段三完成后与阶段四并行开发
```

**每个阶段结束后必须确保 `cargo check`（或 `cargo build`）通过后，才能进入下一阶段。**
## 八、代码规范与 AI 指导

### 8.1 代码规范

1. **注释要求**：所有公共 API 必须添加文档注释（`///`）
2. **错误处理**：使用 `thiserror` 派生错误类型
3. **异步 Trait**：使用 `async-trait` crate
4. **命名规范**：
    - Trait 名称：`PascalCase`（如 `PluginQuery`）
    - 方法名称：`snake_case`
    - 宿主函数命名：`namespace:function_name`（如 `cmx:database/execute_sql`）

###  AI 开发指导

#### 每次开始新任务前

1. 阅读相关模块的现有代码结构
2. 确认要实现的 trait 或方法的签名
3. 检查是否有类似的实现可以参考


#### 实现 trait 接口时

1. 确保方法签名与 trait 定义一致
2. 使用 `#[async_trait]` 派生宏
3. 在实现中调用已有的内部方法
4. 添加错误转换逻辑

#### 重构现有代码时

1. **保留原有实现**：先添加新实现，再删除旧代码
2. **向后兼容**：可以不向后兼容
