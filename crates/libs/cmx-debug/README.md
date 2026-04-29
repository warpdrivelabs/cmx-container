# cmx-debug

> 调试会话管理和 WASM 插件调用模块，为 cmx-container 提供运行时调试能力。

## 项目简介

本 crate 提供调试会话管理、WASM 插件函数调用、调试器附加检测等功能，支持在开发过程中对插件进行调试。

## 快速开始

### 安装

```toml
[dependencies]
cmx-debug = "0.1.0"
```

### 核心示例

```rust
use cmx_debug::{start_debug_session, DebugSession, DebugRequest};
use serde_json::json;

let response = start_debug_session(
    "plugin_id".to_string(),
    "my_plugin".to_string(),
    "1.0.0".to_string(),
    "my_function".to_string(),
    "/path/to/plugin.wasm".to_string(),
    "/path/to/source".to_string(),
    vec![], // wasm_functions
    json!({}),
    json!({"input": "test"}),
);
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 调试会话管理 | 创建、获取、删除调试会话 |
| 调试器检测 | 检测 LLDB/CodeLLDB 是否附加到目标进程 |
| WASM 函数调用 | 直接调用 WASM 插件函数 |
| 会话自动清理 | 自动清理已失效的调试会话 |
| 代码服务器集成 | 支持从配置获取 code_server_url |

## 模块结构

```
cmx-debug
├── src/
│   ├── lib.rs             # 库入口
│   └── plugin.rs          # 插件相关调试功能
└── Cargo.toml
```

## 主要结构说明

### `DebugSession`

调试会话信息，包含会话 ID、插件信息、函数名、WASM 路径等。

### `DebugRequest`

调试请求结构，包含函数名、参数、数据等。

### `DebugResponse`

调试响应结构，包含代码服务器 URL、可用函数列表等。

## 使用指南

### 一、初始化调试模块

#### 1.1 基础初始化

```rust
use cmx_debug::init;

fn main() {
    // 初始化调试模块
    // 启动后台清理线程
    init();
    println!("Debug module initialized");
}
```

#### 1.2 带配置初始化

```rust
use cmx_debug::{init_with_config, DebugConfig};

fn main() {
    let config = DebugConfig {
        // 调试会话超时时间（秒）
        session_timeout: 3600,
        // 最大并发调试会话数
        max_sessions: 10,
        // 代码服务器 URL
        code_server_url: Some("http://localhost:8081".to_string()),
        // 启用详细日志
        verbose: true,
    };

    init_with_config(config);
}
```

### 二、调试会话管理

#### 2.1 创建调试会话

```rust
use cmx_debug::{start_debug_session, DebugSession, DebugRequest};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建调试会话
    let session = start_debug_session(
        "plugin_001".to_string(),           // plugin_id
        "my_plugin".to_string(),            // plugin_name
        "1.0.0".to_string(),                // version
        "process_data".to_string(),         // function_name
        "/plugins/my_plugin/1.0.0/plugin.wasm".to_string(),  // wasm_path
        "/workspace/my_plugin/src".to_string(),  // source_path
        vec!["log_info".to_string()],      // wasm_functions
        serde_json::json!({}),             // context
        serde_json::json!({"input": "test"}),  // input_data
    )?;

    println!("Debug session created: {}", session.id);
    println!("Code server URL: {}", session.code_server_url);

    Ok(())
}
```

#### 2.2 获取调试会话

```rust
use cmx_debug::{get_debug_session, get_all_debug_sessions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 获取单个会话
    if let Some(session) = get_debug_session("session_id_001")? {
        println!("Session: {:?}", session);
    }

    // 获取所有活动会话
    let all_sessions = get_all_debug_sessions()?;
    for session in all_sessions {
        println!("Active session: {} - {}", session.id, session.function_name);
    }

    Ok(())
}
```

#### 2.3 删除调试会话

```rust
use cmx_debug::delete_debug_session;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 删除单个会话
    delete_debug_session("session_id_001")?;

    // 批量删除所有会话
    delete_all_debug_sessions()?;

    Ok(())
}
```

### 三、调试器检测

#### 3.1 检测 LLDB/CodeLLDB 附加状态

```rust
use cmx_debug::is_debugger_attached;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_pid = 12345;

    // 检测是否有调试器附加到目标进程
    if is_debugger_attached(target_pid)? {
        println!("Debugger is attached to process {}", target_pid);
    } else {
        println!("No debugger attached to process {}", target_pid);
    }

    Ok(())
}
```

#### 3.2 获取调试器信息

```rust
use cmx_debug::{get_debugger_info, DebuggerInfo};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pid = 12345;

    if let Some(info) = get_debugger_info(pid)? {
        match info debugger_type {
            DebuggerType::LLDB => println!("LLDB attached"),
            DebuggerType::CodeLLDB => println!("CodeLLDB attached"),
            DebuggerType::GDB => println!("GDB attached"),
            DebuggerType::Unknown => println!("Unknown debugger"),
        }
        println!("PID: {}", info.pid);
        println!("Breakpoints: {:?}", info.breakpoints);
    }

    Ok(())
}
```

### 四、WASM 函数调用

#### 4.1 基础调用

```rust
use cmx_debug::call_plugin_function;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = std::fs::read("plugin.wasm")?;

    let result = call_plugin_function(
        &wasm_bytes,
        "my_function",
        &json!({"input": "test data"}),
    )?;

    println!("Function result: {:?}", result);

    Ok(())
}
```

#### 4.2 带上下文的调用

```rust
use cmx_debug::{call_plugin_function_with_context, CallContext};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = std::fs::read("plugin.wasm")?;

    let context = CallContext {
        trace_id: Some("trace-001".to_string()),
        timeout_ms: Some(5000),
        debug_mode: true,
        environment: serde_json::json!({
            "LOG_LEVEL": "debug"
        }),
    };

    let result = call_plugin_function_with_context(
        &wasm_bytes,
        "process_data",
        &json!({"input": "test"}),
        &context,
    )?;

    println!("Result: {:?}", result);

    Ok(())
}
```

#### 4.3 调用并获取调试信息

```rust
use cmx_debug::{call_with_debug_info, DebugResult};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = std::fs::read("plugin.wasm")?;

    let debug_result: DebugResult = call_with_debug_info(
        &wasm_bytes,
        "my_function",
        &json!({"input": "test"}),
    )?;

    // 函数执行结果
    println!("Result: {:?}", debug_result.output);

    // 调试信息
    if let Some(debug_info) = debug_result.debug_info {
        println!("Execution time: {}ms", debug_info.execution_time_ms);
        println!("Memory usage: {} bytes", debug_info.memory_usage);
        println!("Called functions: {:?}", debug_info.called_functions);
    }

    Ok(())
}
```

### 五、断点管理

#### 5.1 设置断点

```rust
use cmx_debug::{set_breakpoint, Breakpoint};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let breakpoint = Breakpoint {
        function_name: "my_function".to_string(),
        line_number: Some(42),
        condition: Some("x > 10".to_string()),
    };

    let breakpoint_id = set_breakpoint(&breakpoint)?;
    println!("Breakpoint set: {}", breakpoint_id);

    Ok(())
}
```

#### 5.2 列出断点

```rust
use cmx_debug::list_breakpoints;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let breakpoints = list_breakpoints()?;

    for bp in breakpoints {
        println!("{}: {}:{} - {:?}",
            bp.id,
            bp.function_name,
            bp.line_number.unwrap_or(0),
            bp.condition
        );
    }

    Ok(())
}
```

#### 5.3 删除断点

```rust
use cmx_debug::{delete_breakpoint, clear_all_breakpoints};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 删除单个断点
    delete_breakpoint("bp_001")?;

    // 清空所有断点
    clear_all_breakpoints()?;

    Ok(())
}
```

### 六、调试上下文

#### 6.1 创建调试上下文

```rust
use cmx_debug::{DebugContext, DebugVariables};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = DebugContext::new("my_function");

    // 设置局部变量
    context.set_variable("x", 10);
    context.set_variable("name", "test");

    // 设置 Watch 表达式
    context.add_watch("x > 0");
    context.add_watch("result.len() > 0");

    Ok(())
}
```

#### 6.2 获取变量值

```rust
use cmx_debug::{get_local_variable, get_all_variables};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_id = "session_001";

    // 获取单个变量
    if let Some(value) = get_local_variable(session_id, "x")? {
        println!("x = {:?}", value);
    }

    // 获取所有变量
    let variables = get_all_variables(session_id)?;
    for (name, value) in variables {
        println!("{} = {:?}", name, value);
    }

    Ok(())
}
```

### 七、调试会话生命周期

#### 7.1 完整调试流程

```rust
use cmx_debug::{
    init, start_debug_session, is_debugger_attached,
    set_breakpoint, call_plugin_function,
    get_debug_session, delete_debug_session,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化
    init();

    // 2. 检查目标进程是否有调试器附加
    let pid = 12345;
    if !is_debugger_attached(pid)? {
        eprintln!("Please attach debugger to process {} first", pid);
        return Ok(());
    }

    // 3. 创建调试会话
    let session = start_debug_session(
        "plugin_001".to_string(),
        "my_plugin".to_string(),
        "1.0.0".to_string(),
        "my_function".to_string(),
        "/path/to/plugin.wasm".to_string(),
        "/path/to/source".to_string(),
        vec![],
        json!({}),
        json!({"input": "test"}),
    )?;

    println!("Created session: {}", session.id);

    // 4. 设置断点
    let bp_id = set_breakpoint(&Breakpoint {
        function_name: "my_function".to_string(),
        line_number: Some(42),
        condition: None,
    })?;
    println!("Set breakpoint: {}", bp_id);

    // 5. 调用函数（会在断点处暂停）
    let result = call_plugin_function(
        &std::fs::read("plugin.wasm")?,
        "my_function",
        &json!({"input": "test"}),
    )?;

    // 6. 检查执行结果
    println!("Function result: {:?}", result);

    // 7. 删除会话
    delete_debug_session(&session.id)?;

    Ok(())
}
```

### 八、错误处理

```rust
use cmx_debug::{DebugError, call_plugin_function};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = std::fs::read("plugin.wasm")?;

    match call_plugin_function(&wasm_bytes, "nonexistent", &json!({})) {
        Ok(result) => println!("Result: {:?}", result),
        Err(e) => {
            match e.downcast_ref::<DebugError>() {
                Some(DebugError::SessionNotFound(id)) => {
                    eprintln!("Debug session not found: {}", id);
                }
                Some(DebugError::DebuggerNotAttached) => {
                    eprintln!("Debugger not attached to process");
                }
                Some(DebugError::FunctionNotFound(name)) => {
                    eprintln!("Function not found in WASM: {}", name);
                }
                Some(DebugError::InvocationTimeout) => {
                    eprintln!("Function invocation timed out");
                }
                Some(DebugError::WasmError(msg)) => {
                    eprintln!("WASM execution error: {}", msg);
                }
                _ => {
                    eprintln!("Unknown error: {}", e);
                }
            }
        }
    }

    Ok(())
}
```

### 九、配置参考

#### DebugConfig 配置项

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `session_timeout` | u64 | 3600 | 调试会话超时时间（秒） |
| `max_sessions` | usize | 10 | 最大并发调试会话数 |
| `code_server_url` | Option<String> | None | 代码服务器 URL |
| `verbose` | bool | false | 是否启用详细日志 |
| `cleanup_interval` | u64 | 300 | 清理过期会话的间隔（秒） |

#### 环境变量

```bash
# 调试模块日志级别
CMX_DEBUG_LOG=debug

# 代码服务器地址
CMX_CODE_SERVER_URL=http://localhost:8081

# 调试会话超时时间（秒）
CMX_DEBUG_SESSION_TIMEOUT=3600
```
