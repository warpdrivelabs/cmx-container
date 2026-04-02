# cmx-runtime — WASM 运行时引擎

基于 [wasmtime](https://wasmtime.dev/) 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。

## 目录

- [模块概述](#模块概述)
- [设计思想](#设计思想)
- [代码结构](#代码结构)
- [核心类型](#核心类型)
- [使用指南](#使用指南)
- [宿主函数注册](#宿主函数注册)
- [依赖约束](#依赖约束)

---

## 模块概述

`cmx-runtime` 是 CMX 插件系统的 WASM 运行时核心组件，提供：

- **模块加载** — 编译和实例化 WASM 模块
- **宿主函数注册** — 通过 `HostFunctionProvider` trait 注册宿主函数
- **函数调用** — 调用 WASM 导出函数
- **生命周期管理** — 加载/卸载模块
- **全局单例** — `GlobalWasmEngine` 提供应用级单例访问

---

## 设计思想

### 1. 依赖倒置原则

`cmx-runtime` 仅依赖 `cmx-traits` 中的 trait 定义，不依赖任何业务模块：

```
cmx-runtime ──► cmx-traits ◄── cmx-database
                               ◄── cmx-buffer
                               ◄── cmx-plugin
                               ◄── cmx-utils
```

各业务模块通过实现 `HostFunctionProvider` trait 注册宿主函数，`cmx-runtime` 通过 trait 对象调用，实现解耦。

### 2. 宿主函数注册模式

采用 **Provider 模式**，各模块实现 `HostFunctionProvider` trait：

```rust
pub trait HostFunctionProvider: Send + Sync {
    fn namespace(&self) -> &str;
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError>;
    fn provided_functions(&self) -> Vec<&str> { Vec::new() }
}
```

### 3. 全局单例模式

使用 `OnceLock` + `RwLock` 实现线程安全的全局单例：

```rust
static GLOBAL_WASM_ENGINE: OnceLock<Arc<RwLock<WasmEngine>>> = OnceLock::new();
```

### 4. 内存安全

通过 `WasmStoreData` 传递调用上下文，确保每次调用都有独立的 Store：

```rust
pub struct WasmStoreData {
    pub caller_data: CallerData,
}
```

---

## 代码结构

```
crates/libs/cmx-runtime/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口 + GlobalWasmEngine
│   ├── engine.rs           # WasmEngine 核心引擎
│   ├── instance.rs         # WasmInstance 实例包装
│   ├── linker_adapter.rs   # WasmLinker trait 适配器
│   ├── invoker_adapter.rs  # RuntimeInvoker trait 适配器
│   └── error.rs            # RuntimeError 错误类型
└── tests/
    └── engine_test.rs      # 单元测试
```

### 文件说明

| 文件 | 职责 |
|------|------|
| `lib.rs` | 模块入口，定义 `GlobalWasmEngine` 全局单例 |
| `engine.rs` | `WasmEngine` 核心结构，实现 `RuntimeInvoker` trait |
| `instance.rs` | `WasmInstance` 包装 wasmtime 实例，`WasmStoreData` 存储调用上下文 |
| `linker_adapter.rs` | `RuntimeLinkerAdapter` 实现 `WasmLinker` trait，适配 wasmtime::Linker |
| `invoker_adapter.rs` | `WasmEngineInvokerAdapter` 将 `Arc<RwLock<WasmEngine>>` 包装为 `Arc<dyn RuntimeInvoker>` |
| `error.rs` | `RuntimeError` 错误枚举 |

---

## 核心类型

### WasmEngine

WASM 运行时引擎核心：

```rust
pub struct WasmEngine {
    engine: wasmtime::Engine,
    instances: Arc<RwLock<HashMap<String, WasmInstance>>>,
    host_providers: Vec<Box<dyn HostFunctionProvider>>,
    config: WasmEngineConfig,
}
```

**主要方法：**

| 方法 | 说明 |
|------|------|
| `new(config)` | 创建新引擎 |
| `register_provider(provider)` | 注册宿主函数提供者 |
| `get_exports(plugin_id)` | 获取已加载模块的导出函数列表 |

### WasmEngineConfig

引擎配置：

```rust
pub struct WasmEngineConfig {
    pub max_memory_bytes: u64,    // 默认 256MB
    pub enable_fuel: bool,        // 是否启用燃料计量
    pub max_fuel: u64,            // 最大燃料量
    pub enable_wasi: bool,        // 是否启用 WASI
}
```

### GlobalWasmEngine

全局单例访问：

```rust
impl GlobalWasmEngine {
    pub fn initialize(config: WasmEngineConfig) -> Result<(), RuntimeError>;
    pub async fn get() -> RwLockReadGuard<'static, WasmEngine>;
    pub async fn get_mut() -> RwLockWriteGuard<'static, WasmEngine>;
    pub fn get_arc() -> Arc<RwLock<WasmEngine>>;
    pub fn get_as_invoker() -> Arc<dyn RuntimeInvoker>;
    pub fn is_initialized() -> bool;
}
```

### WasmInstance

WASM 实例包装：

```rust
pub struct WasmInstance {
    pub plugin_id: String,
    instance: wasmtime::Instance,
    store: wasmtime::Store<WasmStoreData>,
    module_info: WasmModuleInfo,
}
```

---

## 使用指南

### 1. 初始化引擎

```rust
use cmx_runtime::{GlobalWasmEngine, WasmEngineConfig};

fn init() {
    let config = WasmEngineConfig {
        max_memory_bytes: 256 * 1024 * 1024,
        enable_fuel: true,
        max_fuel: 1_000_000_000,
        enable_wasi: false,
    };
    
    GlobalWasmEngine::initialize(config).expect("初始化失败");
}
```

### 2. 注册宿主函数

```rust
use cmx_runtime::GlobalWasmEngine;
use cmx_database::host_functions::DatabaseHostFunctions;
use cmx_buffer::host_functions::BufferHostFunctions;
use cmx_utils::host_functions::LoggingHostFunctions;

async fn register_providers() {
    let mut engine = GlobalWasmEngine::get_mut().await;
    
    // 注册数据库宿主函数
    engine.register_provider(Box::new(DatabaseHostFunctions::new(db_manager)));
    
    // 注册缓存宿主函数
    engine.register_provider(Box::new(BufferHostFunctions::new(cache_manager)));
    
    // 注册日志宿主函数
    engine.register_provider(Box::new(LoggingHostFunctions::new()));
}
```

### 3. 加载 WASM 模块

```rust
use cmx_traits::RuntimeInvoker;
use cmx_runtime::GlobalWasmEngine;

async fn load_module() {
    let engine = GlobalWasmEngine::get().await;
    
    engine.load_module(
        "my-plugin",
        std::path::Path::new("./plugins/my-plugin/main.wasm")
    ).await.expect("加载失败");
}
```

### 4. 调用 WASM 函数

```rust
use cmx_traits::{RuntimeInvoker, CallerData};
use cmx_runtime::GlobalWasmEngine;

async fn call_wasm() {
    let invoker = GlobalWasmEngine::get_as_invoker();
    
    let caller_data = CallerData::new("my-plugin", "default-db")
        .with_request_id("req-001");
    
    let result = invoker.invoke(
        "my-plugin",
        "handle_request",
        br#"{"action": "process"}"#,
        &caller_data
    ).await.expect("调用失败");
    
    println!("输出: {:?}", result.output);
    println!("耗时: {} μs", result.elapsed_us);
}
```

### 5. 作为 trait 对象注入

```rust
use std::sync::Arc;
use cmx_traits::RuntimeInvoker;
use cmx_runtime::GlobalWasmEngine;

struct MyService {
    runtime: Arc<dyn RuntimeInvoker>,
}

impl MyService {
    fn new() -> Self {
        Self {
            runtime: GlobalWasmEngine::get_as_invoker(),
        }
    }
}
```

---

## 宿主函数注册

### 宿主函数签名约定

所有宿主函数使用统一的 WASM 签名：

```wat
(import "cmx:module" "function_name" (func (param i32 i32) (result i32)))
```

- **参数1 (i32)**: 输入数据指针
- **参数2 (i32)**: 输入数据长度
- **返回值 (i32)**: 输出数据长度（负值表示错误）

### 命名空间约定

使用 `cmx:模块名` 格式：

| 模块 | 命名空间 | 示例函数 |
|------|---------|---------|
| cmx-database | `cmx:database` | `cmx:database/execute_sql` |
| cmx-buffer | `cmx:buffer` | `cmx:buffer/cache_get` |
| cmx-utils | `cmx:log` | `cmx:log/info` |
| cmx-plugin | `cmx:plugin` | `cmx:plugin/call_service` |

### 自定义宿主函数

```rust
use cmx_traits::{HostFunctionProvider, WasmLinker, HostFuncError};

struct MyHostFunctions;

impl HostFunctionProvider for MyHostFunctions {
    fn namespace(&self) -> &str {
        "cmx:my_module"
    }
    
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        let my_func = Box::new(|caller, input| {
            // input 已从 WASM 内存预读取
            let output = process_input(input);
            Ok(output)
        });
        
        linker.define("cmx:my_module", "my_func", my_func)?;
        Ok(())
    }
    
    fn provided_functions(&self) -> Vec<&str> {
        vec!["cmx:my_module/my_func"]
    }
}
```

---

## 依赖约束

### 允许的依赖

- `cmx-core` — 基础类型
- `cmx-traits` — trait 定义
- `cmx-utils` — 工具库
- `wasmtime` — WASM 运行时
- `tokio`, `tracing`, `thiserror`, `serde`, `chrono`

### 禁止的依赖

- `cmx-database` — 数据库模块
- `cmx-buffer` — 缓存模块
- `cmx-plugin` — 插件模块
- `cmx-service` — 服务模块
- `cmx-metadata` — 元数据模块
- `cmx-api` — API 模块

---

## 错误处理

```rust
pub enum RuntimeError {
    ConfigError(String),
    ModuleNotFound(String),
    InstanceNotFound(String),
    ExportNotFound(String),
    InvokeFailed(String),
    MemoryAccessFailed(String),
    HostFuncRegistrationFailed(String),
    Internal(String),
}
```

---

## 测试

```bash
# 运行单元测试
cargo test -p cmx-runtime

# 运行特定测试
cargo test -p cmx-runtime test_engine_initialization
```

---

## 版本信息

- **wasmtime**: 39.0.1
- **Rust edition**: 2021
