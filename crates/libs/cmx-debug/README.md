# cmx-debug

> 调试会话管理和 WASM 插件直调模块，为 cmx-container 提供运行时调试能力。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

本 crate 提供插件调试会话管理（内存会话表 + 后台清理线程）、LLDB/CodeLLDB 调试器附加检测、WASM 插件函数直调（设置 `EXTISM_DEBUG=1` 构建 Extism 插件），以及插件目录/清单/wasm/wit 文件定位辅助函数。

被 cmx-service（编排调试暂停 `debug_prepare`）、cmx-common-api（调试 HTTP handler）、cmx-platform-app / cmx-service-base（装配）依赖。

## 快速开始

### 安装

```toml
[dependencies]
cmx-debug = "0.1.12"
```

### 核心示例

```rust
use cmx_debug::{start_debug_session, WasmFunctionInfo};
use serde_json::json;

// 创建调试会话（返回 DebugResponse，含 code-server URL 与会话 ID）
let response = start_debug_session(
    "plugin_001".to_string(),          // plugin_id
    "my_plugin".to_string(),           // plugin_name
    "1.0.0".to_string(),               // plugin_version
    "my_function".to_string(),         // function_name
    "/plugins/my_plugin/1.0.0/plugin.wasm".to_string(), // wasm_path
    "/workspace/my_plugin/src".to_string(),             // source_path
    vec![WasmFunctionInfo { name: "my_function".to_string() }], // 可调用函数列表
    json!({}),                         // previous_output（失败前上下文）
    json!({"input": "test"}),          // initial_input（服务初始入参）
);
println!("session: {:?}", response.session_id);
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 调试会话管理 | 进程内 `DEBUG_SESSIONS` 会话表（create / get / remove / clear） |
| 会话自动清理 | `init()` 启动后台线程，每 500ms 调 `cleanup_dead_sessions()` 回收失效会话 |
| 调试器检测 | `is_debugger_attached(pid)` 检测 lldb/codelldb 是否附加到目标进程 |
| WASM 函数直调 | `call_plugin_function` 以 `EXTISM_DEBUG=1` 构建 Extism 插件并调用导出函数 |
| code-server 集成 | 从 `CODE_SERVER_URL` 环境变量或插件配置服务获取 URL |
| 插件定位辅助 | plugin.rs：按 ID/名称定位插件目录，解析 manifest.json，查找 wasm/wit 文件 |

## 模块结构

```
cmx-debug
├── src/
│   ├── lib.rs        # 调试会话管理 / 调试器检测 / WASM 直调 / code-server URL
│   └── plugin.rs     # 插件目录与清单定位辅助（CMX_PLUGINS_DIR）
└── Cargo.toml
```

## 主要结构说明

### `DebugSession`

调试会话记录：`id`（`debug_{plugin_name}_{毫秒时间戳}`）/ `plugin_id` / `plugin_name` / `plugin_version` / `function_name` / `wasm_path` / `source_path` / `cmx_pid`（当前进程 PID）/ `start_time`（Instant）/ `is_active` / `is_protected`（新建会话保护位，清理线程在调试器附加后解除）/ `previous_output` / `initial_input`。

### `DebugRequest`

调试调用请求：`function` / `args: Vec<Value>` / `data: Value` / `is_self`（serde 重命名为 `self`）。

### `DebugResponse`

调试会话创建响应（实现 `utoipa::ToSchema`）：`code` / `source_path` / `wasm_path` / `code_server_url: Option<String>` / `plugin` / `functions: Vec<WasmFunctionInfo>` / `cmx_pid` / `debug_function` / `message: Option<String>` / `session_id: Option<String>`。

### 其他类型

- `StartDebugRequest`：`function` / `args` / `data`（HTTP 层调试启动请求体）。
- `WasmFunctionInfo`：`{ name }`（插件可调用函数描述）。
- `InvokeResponse`：`{ code, result: Option<Value>, error: Option<String> }`（调用结果信封）。

## 使用指南

### 一、初始化与调试器检测

#### 1.1 初始化调试模块

```rust
use cmx_debug::init;

fn main() {
    // 启动后台清理线程（幂等，重复调用不会启动第二个线程）
    init();
}
```

#### 1.2 检测 LLDB/CodeLLDB 附加状态

```rust
use cmx_debug::is_debugger_attached;

let target_pid = std::process::id();

// 检测流程：pgrep 找 lldb/codelldb 进程 → lsof / /proc/<pid>/fd 检查是否附加到 cmx-container
// 注意：返回 bool；lldb 进程存在但无法确认附加关系时保守返回 true
if is_debugger_attached(target_pid) {
    println!("Debugger may be attached to process {}", target_pid);
}
```

### 二、调试会话管理

#### 2.1 创建调试会话

```rust
use cmx_debug::{start_debug_session, start_debug_session_async, WasmFunctionInfo};
use serde_json::json;

// 同步版本：code_server_url 取 CODE_SERVER_URL 环境变量（缺省有内置默认值）
let resp = start_debug_session(
    "plugin_001".to_string(),
    "my_plugin".to_string(),
    "1.0.0".to_string(),
    "process_data".to_string(),
    "/plugins/my_plugin/1.0.0/plugin.wasm".to_string(),
    "/workspace/my_plugin/src".to_string(),
    vec![WasmFunctionInfo { name: "process_data".to_string() }],
    json!({}),                       // previous_output
    json!({"input": "test"}),        // initial_input
);

// 异步版本：先查 CODE_SERVER_URL，未设置时请求 http://localhost:{PLUGIN_PORT}/config 获取
let resp = start_debug_session_async(/* 同上 9 参数 */).await;
```

#### 2.2 查询与删除会话

```rust
use cmx_debug::{get_session, get_active_session, remove_session, clear_all_sessions};

// 按 ID 查询
if let Some(session) = get_session("debug_my_plugin_1719000000000") {
    println!("Session {} for function {}", session.id, session.function_name);
}

// 获取当前活动会话（任一 is_active 会话）
let active = get_active_session();

// 删除 / 清空
let removed = remove_session("debug_my_plugin_1719000000000");
clear_all_sessions();
```

#### 2.3 会话自动清理

后台清理线程每 500ms 调用一次 `cleanup_dead_sessions()`：受保护（`is_protected=true`）的新会话在调试器附加后解除保护；未受保护且调试器已脱离的会话被回收。也可手动调用：

```rust
use cmx_debug::cleanup_dead_sessions;

cleanup_dead_sessions();
```

### 三、WASM 函数直调

```rust
use cmx_debug::call_plugin_function;
use serde_json::json;

let wasm_bytes = std::fs::read("/plugins/my_plugin/1.0.0/plugin.wasm")?;

// 以 EXTISM_DEBUG=1 构建 Extism 插件（构建后立即移除该环境变量），JSON 输入输出
let result = call_plugin_function(
    &wasm_bytes,
    "process_data",
    &json!({"input": "test"}),
)?;

println!("Function result: {:?}", result);
// 返回非 JSON 字符串时包装为 { "success": true, "data": { "result": <字符串> } }
```

### 四、code-server URL 获取

```rust
use cmx_debug::{get_code_server_url, get_code_server_url_async};

// 同步：读 CODE_SERVER_URL 环境变量，未设置时返回内置默认地址
let url = get_code_server_url();

// 异步：CODE_SERVER_URL → http://localhost:{PLUGIN_PORT}/config 响应中的 code_server_url → 默认地址
let url = get_code_server_url_async().await;
```

### 五、插件定位辅助（plugin.rs）

```rust
use cmx_debug::plugin::*;

// 插件根目录：环境变量 CMX_PLUGINS_DIR，默认 ./published_plugins
let dir = plugins_dir();

// 遍历根目录按 manifest.json 中的 plugin.id 定位插件目录
let plugin_dir = find_plugin_dir_by_id("plugin_001");

// 按名称拼接目录（不验证存在性）
let dir_by_name = find_plugin_dir_by_name("my_plugin");

// 在插件目录内递归查找第一个 .wasm / .wit 文件
let wasm = find_wasm_file(&dir_by_name);
let wit = find_wit_file(&dir_by_name);

// 清单读取：优先 manifest.json，回退 cmx-plugin.json
let json = read_plugin_json(&dir_by_name);
let (name, version) = get_plugin_info_from_json(&dir_by_name).unwrap();
let (id, name, source_path) = get_plugin_info_from_manifest(&dir_by_name).unwrap();
let source_dir = get_source_path_from_plugin_json(&dir_by_name);
```

### 六、完整调试流程（配合编排器）

```rust
use cmx_debug::{init, is_debugger_attached, call_plugin_function};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 启动会话清理线程
    init();

    // 2. 典型链路：cmx-service 编排执行带 debug 选项时，在 debug_prepare 阶段
    //    调用 start_debug_session(_async) 创建会话并把 DebugResponse 返回给前端；
    //    前端据 code_server_url 发起 code-server 会话、附加 LLDB 调试。

    // 3. 调试器就绪后直调插件函数验证
    let wasm_bytes = std::fs::read("/plugins/my_plugin/1.0.0/plugin.wasm")?;
    let result = call_plugin_function(&wasm_bytes, "process_data", &json!({"input": "test"}))?;
    println!("Result: {:?}", result);

    Ok(())
}
```

### 七、错误处理

本 crate 不定义专用错误枚举，公开函数统一返回 `anyhow::Result`（WASM 构建失败、JSON 序列化失败等由 anyhow 上下文携带）；`DebugResponse.code` / `message` 字段用于向调用方传递业务级状态。

### 八、环境变量参考

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CODE_SERVER_URL` | 内置云端调试地址 | code-server 地址（同步与异步获取的第一个来源） |
| `PLUGIN_PORT` | `9000` | 插件配置服务端口（异步获取 URL 时请求 `http://localhost:{PLUGIN_PORT}/config`） |
| `CMX_PLUGINS_DIR` | `./published_plugins` | 插件发布根目录（plugin.rs 定位插件用） |

## 依赖说明

- `extism` — WASM 直调（`EXTISM_DEBUG=1` 构建以输出调试符号信息）
- `reqwest` — 异步获取 code-server URL
- `walkdir` — 插件目录递归查找 wasm / wit 文件
- `utoipa` — DebugResponse / WasmFunctionInfo 的 OpenAPI schema
- `lazy_static` + `std::sync::Mutex` — 进程内会话表
