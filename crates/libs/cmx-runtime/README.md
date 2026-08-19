# cmx-runtime

> 基于 Extism 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

本 crate 实现 cmx-traits::RuntimeInvoker trait，提供 WASM 模块的加载、实例化和调用功能。依赖 cmx-traits（trait 定义）、cmx-utils（ConfigManager 配置读取）和 extism（WASM 运行时），被 cmx-service 通过 `RuntimeInvoker` trait 对象使用。

## 快速开始

### 安装

```toml
[dependencies]
cmx-runtime = "0.1.12"
```

### 核心示例

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine, LoggingHostFunctions};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建引擎（构建时自动从 ConfigManager 覆盖默认参数）
    let mut engine = ExtismEngine::new(ExtismEngineConfig::default())?;

    // 2. 注册宿主函数提供者（示例：内置日志函数）
    engine.register_provider(Arc::new(LoggingHostFunctions::new()))?;

    // 3. 初始化全局单例
    GlobalExtismEngine::initialize(Arc::new(engine))?;

    // 4. 按插件 ID 调用已加载模块的导出函数
    let invoker = GlobalExtismEngine::get_as_invoker();
    invoker.load_module("my-plugin", "plugins/my-plugin/main.wasm".as_ref()).await?;
    let result = invoker.invoke("my-plugin", "my_function", b"input data").await?;
    println!("elapsed: {}us", result.elapsed_us);
    Ok(())
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| WASM 运行时 | 基于 Extism 的 WASM 模块加载和执行，每插件一个实例池（Pool） |
| 并发模型 | `invoke` 经 `tokio::spawn_blocking` 在阻塞线程池执行，不占用异步 worker |
| 宿主函数桥接 | 通过 `register_provider(Arc<dyn HostFunctionProvider>)` 注册，MsgPack 编解码 |
| 全局单例 | GlobalExtismEngine 提供全局运行时访问 |
| 引擎配置 | ExtismEngineConfig 支持 ConfigManager（dev.toml 等）运行时参数覆盖 |
| 运行时指标 | EngineMetrics 提供无锁原子计数器 |

## 模块结构

```
cmx-runtime
├── src/
│   ├── lib.rs                   # 库入口
│   ├── config.rs                # 引擎配置（ExtismEngineConfig + ConfigManager 覆盖）
│   ├── engine.rs                # 核心引擎（ExtismEngine + RuntimeInvoker 实现）
│   ├── error.rs                 # 错误类型（ExtismError）
│   ├── global.rs                # 全局单例（GlobalExtismEngine）
│   ├── host_function.rs         # 宿主函数桥接（HostFunctionProvider → Extism Function）
│   ├── lifecycle_listener.rs    # 生命周期监听器（RuntimeLifecycleListener）
│   ├── logging_host_functions.rs # 内置日志宿主函数（LoggingHostFunctions）
│   └── metrics.rs               # 运行时指标（EngineMetrics）
└── Cargo.toml
```

## 主要模块说明

### `engine`

ExtismEngine 是核心引擎，实现了 RuntimeInvoker trait。每个插件 ID 对应一个独立的 Extism 实例池（`Pool`），调用时通过 `Pool::with_plugin()` 自动管理实例的获取与归还。

### `config`

ExtismEngineConfig 用于配置引擎行为（WASI、内存上限、超时、实例池大小、Fuel 限制），所有参数均可在构建时被 ConfigManager 覆盖。

### `host_function`

将 cmx-traits 的 `HostFunctionProvider`（namespace / functions / call）包装为 Extism 宿主函数注册进引擎，桥接层对外不暴露公开类型。

## 内部机制

### 高并发架构

```text
ExtismEngine
  ├── plugin_pools: RwLock<HashMap<String, Pool>>
  │     └── Pool（每个 plugin_id 一个）
  │           ├── 工厂函数（PluginBuilder 快速创建实例）
  │           └── 内置 Condvar 等待机制
  ├── cached_functions: RwLock<Vec<extism::Function>>
  │     └── register_provider 预编译的宿主函数，在 load_module 时
  │        经 PluginBuilder::with_functions() 注入每个插件实例
  └── metrics: Arc<EngineMetrics>
```

### 并发模型：spawn_blocking

```text
tokio worker
  → spawn_blocking {  （任务迁移到阻塞线程池，worker 被释放）
      pool.with_plugin { plugin.call() }
    }
  → .await JoinHandle（获取结果）
```

同步阻塞的 `plugin.call()` 不占用异步 worker 线程，实例的获取与归还由 Extism Pool 自动管理。

### 多层防护机制

1. **调用深度限制** — 防止无限递归（默认最大 8 层，由 cmx-traits InvokeContext 实现）
2. **循环检测** — 检测同插件同函数的递归调用（A→B→A 或 A→A）
3. **Extism 原生超时** — 单次 plugin.call() 超时自动中断（Manifest::with_timeout）
4. **Fuel 限制** — 限制 Wasm 指令执行数，防止死循环和恶意代码消耗 CPU

### ExtismEngine 公开方法

| 方法 | 说明 |
|------|------|
| `new(config)` | 创建引擎（内部调 load_runtime_config 覆盖默认参数） |
| `register_provider(provider)` | 注册宿主函数提供者 |
| `cached_function_count()` | 已缓存的宿主函数数量 |
| `get_metrics()` | 获取运行时指标（Arc<EngineMetrics>） |
| `get_pool_count(plugin_id)` | 查询指定插件的实例池大小 |

另有 RuntimeInvoker trait 方法（load_module / unload_module / is_loaded / invoke / invoke_with_options）。

## 使用指南

### 一、引擎初始化与配置

引擎构建时会从 ConfigManager 读取运行时参数覆盖默认值；若 ConfigManager 尚未初始化或配置值无效（如 memory_max <= 0），则保留代码默认值并输出 tracing 警告。

#### 1.1 基础初始化

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig};

let engine = ExtismEngine::new(ExtismEngineConfig::default())?;
```

#### 1.2 配置项与运行时覆盖

```rust
use cmx_runtime::ExtismEngineConfig;
use std::time::Duration;

let config = ExtismEngineConfig {
    enable_wasi: true,                       // 启用 WASI（默认 true）
    memory_max: 4096,                        // 内存上限（页，每页 64KB，默认 4096 = 256MB）
    timeout: Duration::from_secs(30),        // 单次调用超时（默认 30s）
    pool_max_instances: 8,                    // 每插件实例池上限（默认取 CPU 核心数）
    fuel_limit: Some(10_000_000),            // WASM 指令步数限制（None 不限制）
};

let engine = ExtismEngine::new(config)?;
```

`ExtismEngine::new` 构建时会调用 `load_runtime_config`，从 ConfigManager 读取以下键覆盖上述默认值（未初始化 ConfigManager 或值无效时保持默认）：

```
runtime.memory_max          # 内存上限（页数）
runtime.timeout             # 超时（秒）
runtime.pool_max_instances  # 实例池上限
runtime.fuel_limit          # Fuel 限制（Wasm 指令数；0 表示不限制）
```

### 二、全局单例管理

#### 2.1 初始化全局运行时

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = ExtismEngine::new(ExtismEngineConfig::default())?;
    GlobalExtismEngine::initialize(Arc::new(engine))?; // 同步方法，只允许初始化一次

    assert!(GlobalExtismEngine::is_initialized());
    Ok(())
}
```

#### 2.2 获取全局引擎

```rust
use cmx_runtime::GlobalExtismEngine;
use cmx_traits::runtime::InvokeOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式一：获取 Arc<dyn RuntimeInvoker>（用于依赖注入）
    let invoker = GlobalExtismEngine::get_as_invoker();
    let result = invoker
        .invoke_with_options("my-plugin", "my_function", b"input", &InvokeOptions::new())
        .await?;

    // 方式二：获取引擎本体（用于注册宿主函数、读取指标等）
    let engine = GlobalExtismEngine::get().engine();
    let pool_size = engine.get_pool_count("my-plugin"); // Option<usize>

    Ok(())
}
```

全局单例不支持替换（无 `set`），如需重建引擎应重启进程或重建 `ExtismEngine` 实例自持使用。

### 三、WASM 函数调用

#### 3.1 基础调用（按插件 ID）

调用入口与 cmx-traits 的 `RuntimeInvoker` 一致：先 `load_module` 按插件 ID 加载 WASM 文件（双重检查锁 + PoolBuilder），再 `invoke` 调用导出函数。输入字节通常为 `FunctionInput` 的序列化形式。

```rust
use cmx_traits::runtime::{RuntimeInvoker, WasmInvokeResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let invoker = GlobalExtismEngine::get_as_invoker();
    invoker.load_module("my-plugin", "published_plugins/my-plugin/main.wasm".as_ref()).await?;

    // WasmInvokeResult 是结构体：{ output: Vec<u8>, elapsed_us: u64, fuel_consumed: Option<u64> }
    let WasmInvokeResult { output, elapsed_us, .. } = invoker
        .invoke("my-plugin", "my_function", br#"{"input":"test"}"#)
        .await?;

    // 解析返回值（通常为 MsgPack/JSON 编码的 FunctionOutput）
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output) {
        println!("Returned JSON: {:?}", json);
    }
    println!("elapsed: {}us", elapsed_us);
    Ok(())
}
```

超时控制通过 `invoke_with_options` 的 `InvokeOptions::with_timeout` 传入（见 cmx-traits 文档）；超时与失败会计入指标（见第五节）。

#### 3.2 模块生命周期

```rust
// 卸载模块（清空实例池）
invoker.unload_module("my-plugin").await?;

// 查询加载状态
let loaded = invoker.is_loaded("my-plugin");
```

### 四、宿主函数注册

宿主函数以 `HostFunctionProvider`（cmx-traits）为单位注册，桥接层将其包装为 Extism Function 并在构建插件实例时注入；通信使用 MsgPack 编解码。

```rust
use cmx_runtime::{ExtismEngine, LoggingHostFunctions};
use std::sync::Arc;

let mut engine = ExtismEngine::new(ExtismEngineConfig::default())?;

// 注册内置的日志宿主函数（log_info / log_error / ...）
engine.register_provider(Arc::new(LoggingHostFunctions::new()))?;

// 注册自定义提供者：实现 cmx_traits::runtime::HostFunctionProvider
// （namespace 返回 "cmx:模块名"，functions 返回 HostFunctionDef 列表，
//  call 接收 MsgPack 字节并返回 MsgPack 字节，详见 cmx-traits README）
engine.register_provider(Arc::new(my_provider))?;

println!("host functions cached: {}", engine.cached_function_count());
```

### 五、运行时指标

`EngineMetrics` 暴露 4 个公共原子字段（`AtomicU64`），由引擎在每次调用后自动记录（`record_success` / `record_failure` / `record_timeout`）：

```rust
use cmx_runtime::GlobalExtismEngine;
use std::sync::atomic::Ordering;

let metrics = GlobalExtismEngine::get().engine().get_metrics();

let total = metrics.total_calls.load(Ordering::Relaxed);      // 总调用次数
let failed = metrics.failed_calls.load(Ordering::Relaxed);    // 失败次数（不含超时）
let timeouts = metrics.timeout_calls.load(Ordering::Relaxed); // 超时次数
let elapsed = metrics.total_elapsed_us.load(Ordering::Relaxed); // 累计耗时（微秒）

let avg_us = if total > 0 { elapsed / total } else { 0 };
println!("avg latency: {}us", avg_us);
```

### 六、生命周期监听

`RuntimeLifecycleListener` 订阅全局事件总线的插件升级/卸载/降级事件，自动清除对应插件的 WASM 实例池缓存（按 app_id 过滤事件）：

```rust
use cmx_runtime::RuntimeLifecycleListener;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let invoker = GlobalExtismEngine::get_as_invoker();
    let listener = RuntimeLifecycleListener::new(invoker, "app-001".to_string());
    listener.register().await; // 订阅 plugin.upgraded / uninstalled / downgraded
}
```

### 七、错误处理

`ExtismError` 为引擎层错误，实现 `RuntimeInvoker` 时会转换为 `TraitError`（如 `WasmLoadFailed` / `WasmInvokeFailed`）抛给调用方：

```rust
use cmx_runtime::ExtismError;

match err {
    ExtismError::PluginLoadFailed(msg) => {
        eprintln!("WASM 模块加载失败: {}", msg);
    }
    ExtismError::PluginCallFailed(msg) => {
        eprintln!("WASM 函数调用失败: {}", msg);
    }
    ExtismError::ConfigError(msg) => {
        eprintln!("引擎配置错误: {}", msg);
    }
    ExtismError::InternalError(msg) => {
        eprintln!("内部错误: {}", msg);
    }
}
```

### 八、完整示例

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine, LoggingHostFunctions};
use cmx_traits::runtime::{InvokeOptions, RuntimeInvoker};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置并创建引擎（ConfigManager 中的 runtime.* 键会覆盖默认值）
    let mut engine = ExtismEngine::new(ExtismEngineConfig {
        fuel_limit: Some(10_000_000),
        ..Default::default()
    })?;

    // 2. 注册宿主函数
    engine.register_provider(Arc::new(LoggingHostFunctions::new()))?;

    // 3. 初始化全局运行时
    GlobalExtismEngine::initialize(Arc::new(engine))?;

    // 4. 加载并调用 WASM 插件
    let invoker = GlobalExtismEngine::get_as_invoker();
    invoker.load_module("my-plugin", "published_plugins/my-plugin/main.wasm".as_ref()).await?;

    let options = InvokeOptions::new()
        .with_timeout(Duration::from_secs(5))
        .with_max_depth(16);

    let result = invoker
        .invoke_with_options("my-plugin", "process", br#"{"input":"test"}"#, &options)
        .await?;

    // 5. 处理结果
    let response = serde_json::from_slice::<serde_json::Value>(&result.output)?;
    println!("Result: {:?} ({}us, fuel: {:?})",
        response, result.elapsed_us, result.fuel_consumed);

    Ok(())
}
```
