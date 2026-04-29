# cmx-runtime

> 基于 Extism 的 WASM 运行时引擎，负责 WASM 模块的加载、实例化和调用。

## 项目简介

本 crate 实现 cmx-traits::RuntimeInvoker trait，提供 WASM 模块的加载、实例化和调用功能。依赖 cmx-traits（trait 定义）、cmx-utils（ConfigManager 配置读取）和 extism（WASM 运行时）。

## 快速开始

### 安装

```toml
[dependencies]
cmx-runtime = "0.1.0"
```

### 核心示例

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine};
use cmx_traits::InvokeContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ExtismEngineConfig::default();
    let engine = ExtismEngine::new(config)?;

    let wasm_bytes = std::fs::read("plugin.wasm")?;
    let result = engine
        .invoke(&wasm_bytes, "my_function", b"input data", &InvokeContext::default())
        .await?;

    println!("Result: {:?}", result);
    Ok(())
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| WASM 运行时 | 基于 Extism 的 WASM 模块加载和执行 |
| 宿主函数桥接 | HostFunctionContext + Extism 回调机制 |
| 全局单例 | GlobalExtismEngine 提供全局运行时访问 |
| 引擎配置 | ExtismEngineConfig 支持缓存/Fuel 初始化 |
| 运行时指标 | EngineMetrics 提供无锁原子计数器 |

## 模块结构

```
cmx-runtime
├── src/
│   ├── lib.rs                  # 库入口
│   ├── config.rs               # 引擎配置
│   ├── engine.rs               # 核心引擎
│   ├── error.rs                # 错误类型
│   ├── global.rs               # 全局单例
│   ├── host_function.rs        # 宿主函数桥接
│   ├── lifecycle_listener.rs    # 生命周期监听器
│   └── metrics.rs              # 运行时指标
└── Cargo.toml
```

## 主要模块说明

### `engine`

ExtismEngine 是核心引擎，实现了 RuntimeInvoker trait。

### `config`

ExtismEngineConfig 用于配置引擎行为，包括缓存大小、Fuel 限制等。

### `host_function`

HostFunctionContext 提供宿主函数桥接功能。

## 使用指南

### 一、引擎初始化与配置

#### 1.1 基础初始化

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig};

let config = ExtismEngineConfig::default();
let engine = ExtismEngine::new(config)?;
```

#### 1.2 自定义配置

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, CacheConfig, FuelConfig};

let config = ExtismEngineConfig::builder()
    // 配置 WASM 模块缓存
    .with_cache_config(CacheConfig {
        max_size: 100,           // 最大缓存模块数
        ttl_seconds: 3600,       // 缓存 TTL（秒）
    })
    // 配置 Fuel 限制（防止无限循环）
    .with_fuel_config(FuelConfig {
        initial_fuel: 10000000, // 初始 fuel 值
        max_fuel: 100000000,    // 最大 fuel 值
    })
    // 配置日志级别
    .with_log_level("debug")
    .build();

let engine = ExtismEngine::new(config)?;
```

### 二、全局单例管理

#### 2.1 初始化全局运行时

```rust
use cmx_runtime::GlobalExtismEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化全局运行时
    GlobalExtismEngine::initialize(ExtismEngineConfig::default())?;

    // 验证初始化成功
    assert!(GlobalExtismEngine::is_initialized());

    Ok(())
}
```

#### 2.2 获取全局引擎

```rust
use cmx_runtime::GlobalExtismEngine;
use cmx_traits::{RuntimeInvoker, InvokeContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式一：获取 Arc<dyn RuntimeInvoker>
    let invoker = GlobalExtismEngine::get().await;
    let result = invoker
        .invoke(wasm_bytes, "my_function", input, &InvokeContext::default())
        .await?;

    // 方式二：直接作为 trait 对象使用
    let invoker = GlobalExtismEngine::get_as_invoker();
    let result = invoker
        .invoke(wasm_bytes, "my_function", input, &InvokeContext::default())
        .await?;

    Ok(())
}
```

#### 2.3 全局引擎替换

```rust
use cmx_runtime::{GlobalExtismEngine, ExtismEngine, ExtismEngineConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 替换全局运行时
    let new_engine = ExtismEngine::new(ExtismEngineConfig::default())?;
    GlobalExtismEngine::set(new_engine).await?;

    Ok(())
}
```

### 三、WASM 函数调用

#### 3.1 基础调用

```rust
use cmx_runtime::ExtismEngine;
use cmx_traits::{InvokeContext, WasmInvokeResult, InvokeOutput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = ExtismEngine::new(ExtismEngineConfig::default())?;
    let wasm_bytes = std::fs::read("plugin.wasm")?;

    let context = InvokeContext::default();
    let result = engine
        .invoke(&wasm_bytes, "my_function", b"input data", &context)
        .await?;

    match result {
        WasmInvokeResult::Success(output) => {
            println!("Function returned: {:?}", output);
        }
        WasmInvokeResult::Failure(code, msg) => {
            eprintln!("Function failed: {} - {}", code, msg);
        }
    }

    Ok(())
}
```

#### 3.2 使用 InvokeContext

```rust
use cmx_traits::invoke_context::InvokeContext;

let mut context = InvokeContext::default();

// 设置追踪 ID
context.set_trace_id("request-12345");

// 设置超时（毫秒）
context.set_timeout(30000);

// 设置调试模式
context.set_debug(true);

// 添加自定义数据
context.set("user_id", "user-001");
context.set("session_id", "sess-xyz");

let result = engine
    .invoke(&wasm_bytes, "process", input, &context)
    .await?;
```

#### 3.3 解析 WASM 返回值

```rust
use cmx_traits::{InvokeOutput, WasmInvokeResult};

match result {
    WasmInvokeResult::Success(output) => {
        // 获取返回的字节数组
        let bytes = output.data();

        // 解析为字符串
        if let Ok(s) = std::str::from_utf8(bytes) {
            println!("Returned string: {}", s);
        }

        // 解析为 JSON
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
            println!("Returned JSON: {:?}", json);
        }
    }
    WasmInvokeResult::Failure(code, msg) => {
        eprintln!("Error {}: {}", code, msg);
    }
}
```

### 四、宿主函数注册

#### 4.1 注册宿主函数

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, HostFunctionContext};
use extism_pdk::*;

fn register_host_functions(ctx: &mut HostFunctionContext) {
    // 注册 log_info 函数
    ctx.register_fn("log_info", |msg: String| {
        tracing::info!("[WASM] {}", msg);
    });

    // 注册 log_error 函数
    ctx.register_fn("log_error", |msg: String| {
        tracing::error!("[WASM] {}", msg);
    });

    // 注册数据库查询函数
    ctx.register_fn("db_query", |request: DbRequest| -> DbResponse {
        // 处理数据库查询
    });

    // 注册缓存操作函数
    ctx.register_fn("cache_get", |key: String| -> Option<String> {
        cache.get(&key)
    });

    ctx.register_fn("cache_set", |key: String, value: String, ttl: Option<i64>| {
        cache.set(&key, &value, ttl);
    });
}
```

#### 4.2 在引擎中使用宿主函数

```rust
use cmx_runtime::{ExtismEngine, HostFunctionContext};

let mut host_ctx = HostFunctionContext::new();
register_host_functions(&mut host_ctx);

let config = ExtismEngineConfig::builder()
    .with_host_context(host_ctx)
    .build();

let engine = ExtismEngine::new(config)?;
```

### 五、运行时指标

#### 5.1 获取运行时指标

```rust
use cmx_runtime::{ExtismEngine, EngineMetrics};

let engine = ExtismEngine::new(ExtismEngineConfig::default())?;
let metrics = engine.metrics();

// 获取各指标的当前值
println!("Total invocations: {}", metrics.total_invocations());
println!("Successful invocations: {}", metrics.successful_invocations());
println!("Failed invocations: {}", metrics.failed_invocations());
println!("Total execution time: {}ms", metrics.total_execution_time_ms());
```

#### 5.2 重置指标

```rust
use cmx_runtime::EngineMetrics;

let metrics = EngineMetrics::default();
metrics.reset();

assert_eq!(metrics.total_invocations(), 0);
```

### 六、生命周期监听

#### 6.1 实现生命周期监听器

```rust
use cmx_runtime::{
    ExtismEngine, ExtismEngineConfig,
    LifecycleEvent, LifecycleListener,
};

struct MyLifecycleListener;

impl LifecycleListener for MyLifecycleListener {
    fn on_module_loaded(&self, module_name: &str) {
        tracing::info!("Module loaded: {}", module_name);
    }

    fn on_module_unloaded(&self, module_name: &str) {
        tracing::info!("Module unloaded: {}", module_name);
    }

    fn on_invocation_started(&self, func_name: &str) {
        tracing::debug!("Invocation started: {}", func_name);
    }

    fn on_invocation_finished(&self, func_name: &str, duration_ms: u64) {
        tracing::debug!("Invocation finished: {} ({}ms)", func_name, duration_ms);
    }
}

let listener = Arc::new(MyLifecycleListener {});
let config = ExtismEngineConfig::builder()
    .with_lifecycle_listener(listener)
    .build();
```

### 七、错误处理

```rust
use cmx_runtime::{ExtismEngine, ExtismEngineError};

match engine.invoke(&wasm_bytes, func_name, input, &context).await {
    Ok(WasmInvokeResult::Success(output)) => {
        // 处理成功
    }
    Ok(WasmInvokeResult::Failure(code, msg)) => {
        // 处理 WASM 函数返回的错误
        eprintln!("WASM error {}: {}", code, msg);
    }
    Err(e) => {
        match e.downcast_ref::<ExtismEngineError>() {
            Some(ExtismEngineError::ModuleNotFound(name)) => {
                eprintln!("WASM module not found: {}", name);
            }
            Some(ExtismEngineError::FunctionNotFound(name)) => {
                eprintln!("Function not found: {}", name);
            }
            Some(ExtismEngineError::InvocationTimeout) => {
                eprintln!("Invocation timed out");
            }
            Some(ExtismEngineError::OutOfFuel) => {
                eprintln!("WASM execution ran out of fuel");
            }
            _ => {
                eprintln!("Unknown error: {}", e);
            }
        }
    }
}
```

### 八、完整示例

```rust
use cmx_runtime::{
    ExtismEngine, ExtismEngineConfig, GlobalExtismEngine,
    HostFunctionContext, CacheConfig,
};
use cmx_traits::{RuntimeInvoker, InvokeContext, WasmInvokeResult};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 配置并初始化全局运行时
    let mut host_ctx = HostFunctionContext::new();

    // 注册宿主函数
    host_ctx.register_fn("log_info", |msg: String| {
        tracing::info!("[Plugin] {}", msg);
    });

    let config = ExtismEngineConfig::builder()
        .with_cache_config(CacheConfig { max_size: 50, ttl_seconds: 1800 })
        .with_host_context(host_ctx)
        .build();

    GlobalExtismEngine::initialize(config).await?;

    // 2. 加载并调用 WASM 插件
    let wasm_bytes = std::fs::read("my_plugin.wasm")?;

    let mut context = InvokeContext::default();
    context.set_trace_id("trace-001");
    context.set_timeout(5000);

    let invoker = GlobalExtismEngine::get_as_invoker();
    let result = invoker
        .invoke(&wasm_bytes, "process", b"{\"input\":\"test\"}", &context)
        .await?;

    // 3. 处理结果
    match result {
        WasmInvokeResult::Success(output) => {
            let response = serde_json::from_slice::<serde_json::Value>(output.data())?;
            println!("Result: {:?}", response);
        }
        WasmInvokeResult::Failure(code, msg) => {
            return Err(format!("Plugin error ({}): {}", code, msg).into());
        }
    }

    Ok(())
}
```
