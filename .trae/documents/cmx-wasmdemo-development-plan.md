# cmx-wasmdemo 模块开发计划

## 目标

创建一个 WASM demo 模块 (`cmx-wasmdemo`)，用于验证 WASM 宿主函数功能。

## 技术背景

### 目标平台

- **目标**: `wasm32-wasip1`（WASI Preview 1）
- **运行时**: wasmtime 39.x
- **原因**: wasmtime 支持 WASI (WebAssembly System Interface)，需要使用 `wasm32-wasip1` 目标而非 `wasm32-unknown-unknown`

### WASI Preview 1 说明

WASI Preview 1 提供了系统调用接口，允许 WASM 模块：
- 访问文件系统
- 使用标准输入输出
- 获取环境变量
- 支持异步操作

## 模块结构

```
crates/libs/cmx-wasmdemo/
├── Cargo.toml          # 模块配置（cdylib + wasip1 target）
├── src/
│   ├── lib.rs          # 模块入口，导出 WASM 函数
│   ├── host_funcs.rs   # 宿主函数导入声明（extern "C"）
│   ├── memory.rs       # WASM 内存管理（分配/释放）
│   └── demo.rs         # 各功能演示实现
└── README.md           # 使用文档
```

## 现有 HostFunctions 清单

| 命名空间 | 函数名 | 功能描述 | 输入格式 | 输出格式 |
|---------|--------|----------|----------|----------|
| `cmx:log` | `info` | 记录 info 日志 | UTF-8 字符串 | 无 |
| `cmx:log` | `warn` | 记录 warn 日志 | UTF-8 字符串 | 无 |
| `cmx:log` | `error` | 记录 error 日志 | UTF-8 字符串 | 无 |
| `cmx:database` | `execute_sql` | 执行写操作 SQL | JSON | JSON |
| `cmx:database` | `query_sql` | 执行查询 SQL | JSON | JSON |
| `cmx:database` | `txn_begin` | 开启事务 | 无 | JSON (txn_id) |
| `cmx:database` | `txn_commit` | 提交事务 | 无 | JSON |
| `cmx:database` | `txn_rollback` | 回滚事务 | 无 | JSON |
| `cmx:buffer` | `cache_get` | 读取缓存 | JSON | JSON |
| `cmx:buffer` | `cache_set` | 写入缓存 | JSON | JSON |
| `cmx:buffer` | `cache_delete` | 删除缓存 | JSON | JSON |
| `cmx:plugin` | `call_service` | 调用其他插件 | JSON | JSON |
| `cmx:plugin` | `get_info` | 获取当前插件信息 | 无 | JSON |

## 实现步骤

### 步骤 1：创建模块目录结构

创建 `crates/libs/cmx-wasmdemo/` 目录及文件。

### 步骤 2：配置 Cargo.toml

```toml
[package]
name = "cmx-wasmdemo"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]  # 编译为动态库（.wasm）

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# WASI 相关（可选，用于标准输出等）
# wasi = "0.11"
```

### 步骤 3：实现宿主函数导入声明

在 `src/host_funcs.rs` 中声明所有宿主函数：

```rust
// 宿主函数签名: (input_ptr: i32, input_len: i32) -> i32
// 返回值: 输出数据的指针，输出长度通过共享内存传递

#[link(wasm_import_module = "cmx:log")]
extern "C" {
    fn info(input_ptr: i32, input_len: i32) -> i32;
    fn warn(input_ptr: i32, input_len: i32) -> i32;
    fn error(input_ptr: i32, input_len: i32) -> i32;
}

#[link(wasm_import_module = "cmx:database")]
extern "C" {
    fn execute_sql(input_ptr: i32, input_len: i32) -> i32;
    fn query_sql(input_ptr: i32, input_len: i32) -> i32;
    fn txn_begin(input_ptr: i32, input_len: i32) -> i32;
    fn txn_commit(input_ptr: i32, input_len: i32) -> i32;
    fn txn_rollback(input_ptr: i32, input_len: i32) -> i32;
}

#[link(wasm_import_module = "cmx:buffer")]
extern "C" {
    fn cache_get(input_ptr: i32, input_len: i32) -> i32;
    fn cache_set(input_ptr: i32, input_len: i32) -> i32;
    fn cache_delete(input_ptr: i32, input_len: i32) -> i32;
}

#[link(wasm_import_module = "cmx:plugin")]
extern "C" {
    fn call_service(input_ptr: i32, input_len: i32) -> i32;
    fn get_info(input_ptr: i32, input_len: i32) -> i32;
}
```

### 步骤 4：实现内存管理

在 `src/memory.rs` 中实现内存分配和释放：

```rust
/// 分配 WASM 内存
#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr() as i32;
    std::mem::forget(buf);  // 防止 Rust 释放
    ptr
}

/// 释放 WASM 内存
#[no_mangle]
pub extern "C" fn dealloc(ptr: i32, size: i32) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr as *mut u8, size as usize, size as usize);
    }
}
```

### 步骤 5：实现演示函数

在 `src/demo.rs` 中实现各功能演示：

```rust
use crate::host_funcs::*;
use crate::memory::*;

/// 演示日志功能
#[no_mangle]
pub extern "C" fn demo_log() -> i32 {
    // 调用 info 日志
    let msg = b"Hello from WASM!";
    unsafe { info(msg.as_ptr() as i32, msg.len() as i32) };
    
    // 调用 warn 日志
    let msg = b"This is a warning!";
    unsafe { warn(msg.as_ptr() as i32, msg.len() as i32) };
    
    // 调用 error 日志
    let msg = b"This is an error!";
    unsafe { error(msg.as_ptr() as i32, msg.len() as i32) };
    
    0  // 成功
}

/// 演示缓存功能
#[no_mangle]
pub extern "C" fn demo_cache() -> i32 {
    // 设置缓存
    let set_req = r#"{"key": "demo_key", "value": "demo_value", "ttl_seconds": 60}"#;
    unsafe { cache_set(set_req.as_ptr() as i32, set_req.len() as i32) };
    
    // 读取缓存
    let get_req = r#"{"key": "demo_key"}"#;
    unsafe { cache_get(get_req.as_ptr() as i32, get_req.len() as i32) };
    
    // 删除缓存
    let del_req = r#"{"key": "demo_key"}"#;
    unsafe { cache_delete(del_req.as_ptr() as i32, del_req.len() as i32) };
    
    0
}

/// 演示数据库功能
#[no_mangle]
pub extern "C" fn demo_database() -> i32 {
    // 执行查询
    let query_req = r#"{"sql": "SELECT 1"}"#;
    unsafe { query_sql(query_req.as_ptr() as i32, query_req.len() as i32) };
    
    0
}

/// 演示插件信息获取
#[no_mangle]
pub extern "C" fn demo_plugin_info() -> i32 {
    unsafe { get_info(0, 0) };
    0
}

/// 综合测试入口
#[no_mangle]
pub extern "C" fn run_all_demos() -> i32 {
    demo_log();
    demo_cache();
    demo_database();
    demo_plugin_info();
    0
}
```

### 步骤 6：更新工作空间

在根 `Cargo.toml` 中添加新成员：

```toml
[workspace]
members = [
    # ... 现有成员
    "crates/libs/cmx-wasmdemo",
]
```

### 步骤 7：在 cmx-service 中添加集成测试

在 `crates/libs/cmx-service/tests/wasm_demo_test.rs` 中添加测试：

```rust
//! WASM Demo 集成测试

use cmx_service::{CmxService, InvokeRequest, ServiceConfig};
use cmx_runtime::{GlobalWasmEngine, WasmEngineConfig};
use cmx_traits::{PluginQuery, PluginSnapshot, RuntimeInvoker, CallerData, TraitError};
use std::sync::Arc;
use std::path::PathBuf;
use async_trait::async_trait;

// ... Mock PluginQuery 实现 ...

#[tokio::test]
async fn test_wasm_demo_log() {
    // 初始化 WASM 引擎
    GlobalWasmEngine::initialize(WasmEngineConfig::default()).unwrap();
    
    // 加载 demo WASM
    let wasm_path = PathBuf::from("../cmx-wasmdemo/target/wasm32-wasip1/release/cmx_wasmdemo.wasm");
    let runtime = GlobalWasmEngine::get();
    runtime.load_module("demo-plugin", &wasm_path).await.unwrap();
    
    // 调用 demo_log 函数
    let caller_data = CallerData::new("demo-plugin", "default");
    let result = runtime.invoke("demo-plugin", "demo_log", &[], &caller_data).await;
    
    assert!(result.is_ok());
}

// 更多测试...
```

## 编译命令

```bash
# 安装 WASI 目标
rustup target add wasm32-wasip1

# 编译 WASM 模块
cd crates/libs/cmx-wasmdemo
cargo build --release --target wasm32-wasip1

# 输出文件
# target/wasm32-wasip1/release/cmx_wasmdemo.wasm
```

## 文件清单

| 文件路径 | 说明 |
|---------|------|
| `crates/libs/cmx-wasmdemo/Cargo.toml` | 模块配置 |
| `crates/libs/cmx-wasmdemo/src/lib.rs` | 模块入口 |
| `crates/libs/cmx-wasmdemo/src/host_funcs.rs` | 宿主函数导入声明 |
| `crates/libs/cmx-wasmdemo/src/memory.rs` | WASM 内存管理 |
| `crates/libs/cmx-wasmdemo/src/demo.rs` | 演示函数实现 |
| `crates/libs/cmx-service/tests/wasm_demo_test.rs` | 集成测试 |
| `Cargo.toml` | 更新工作空间成员 |

## 验证标准

1. ✅ `cargo build --target wasm32-wasip1 --package cmx-wasmdemo` 编译成功
2. ✅ 生成的 `.wasm` 文件可被 wasmtime 加载
3. ✅ 各演示函数能正确调用宿主函数并返回预期结果
4. ✅ 集成测试全部通过

## 注意事项

1. **WASI 目标**: 使用 `wasm32-wasip1` 而非 `wasm32-unknown-unknown`
2. **内存管理**: WASM 与宿主之间的数据传递需要通过线性内存
3. **数据格式**: 所有输入输出使用 JSON 格式，便于序列化/反序列化
4. **错误处理**: WASM 中不能使用 `std::error::Error`，需要简化错误处理
5. **异步限制**: WASM 内部是同步的，宿主函数通过 `block_on` 处理异步
