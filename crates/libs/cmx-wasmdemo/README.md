# cmx-wasmdemo

> WASM 插件演示模块，基于 Extism PDK + cmx-plugin-sdk 开发，用于验证插件功能并提供各种演示函数。

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2021-orange.svg)]()

## 当前状态：非活跃 member（已注释）

本 crate 已从 workspace members 中注释掉（见根 `Cargo.toml` 中 `#    "crates/libs/cmx-wasmdemo",`），不参与常规构建；源码保留供参考。因此它自带版本号（`0.1.0`）与 `edition 2021`，不随 workspace 版本（0.1.12 / 2024）走。

**活跃的替代示范**：新插件开发请参考 `crates/libs/cmx-plugin-demo`（workspace 活跃 member，以「订单管理」业务场景、三层分离架构演示 cmx-plugin-sdk 全部宿主能力，含 IAM 权限等新增能力）。

## 概述

本模块展示了如何开发 WASM 插件并与宿主函数交互，涵盖日志、缓存、数据库、插件调用和服务编排等核心能力。

## 模块结构

```
cmx-wasmdemo
├── src/
│   ├── lib.rs              # 模块入口，条件编译 extism_layer
│   ├── models.rs           # 公共数据模型 (DemoRequest, DemoResponse, RouteInput 等)
│   ├── host_traits.rs      # 宿主功能 trait (HostFunctions)
│   ├── core.rs             # 插件核心业务逻辑 (PluginCore<H>)
│   ├── extism_layer.rs     # Extism 导出层 (#[plugin_fn] 入口，需 extism feature)
│   └── tests.rs            # 单元测试（MockHost）
└── Cargo.toml
```

### 架构说明

- **`core::PluginCore<H>`** — 所有业务逻辑实现，泛型参数 `H: HostFunctions`，便于单元测试时 mock。
- **`host_traits::HostFunctions`** — 宿主能力 trait，定义日志、缓存、数据库、插件调用等接口。
- **`extism_layer`** — 仅在 `extism` feature 开启时编译，通过 `#[plugin_fn]` 宏将函数暴露给 Extism 运行时，内部委托给 `PluginCore<ExtismHost>`。

---

## 导出函数列表

| 函数名 | 功能 | 分类 |
|--------|------|------|
| `count_vowels` | 统计字符串中的元音字母数量 | 基础函数 |
| `demo_log` | 演示日志功能（info/error/debug/warn） | 基础函数 |
| `demo_cache` | 演示缓存写入、读取操作 | 基础函数 |
| `demo_database` | 演示数据库查询 | 基础函数 |
| `demo_call_plugin` | 演示插件间调用 | 基础函数 |
| `demo_call_service_by_key` | 演示服务编排调用 | 基础函数 |
| `run_all_demos` | 综合测试（日志+缓存+数据库） | 基础函数 |
| `route_check` | 路由判断，返回分支标识 | 服务编排 |
| `branch_1_process` | 分支1处理 | 服务编排 |
| `branch_2_process` | 分支2处理 | 服务编排 |
| `branch_3_process` | 分支3处理 | 服务编排 |
| `merge_result` | 合并各分支结果 | 服务编排 |
| `tx_insert` | 事务插入 | 事务处理 |
| `tx_update` | 事务更新 | 事务处理 |
| `tx_query` | 事务查询 | 事务处理 |
| `tx_delete` | 事务删除 | 事务处理 |
| `final_process` | 最终处理，整合各步骤输出 | 服务编排 |

---

## 构建说明

### 前置条件

```bash

# 添加 WASM 编译目标
rustup target add wasm32-unknown-unknown
rustup target add wasm32-wasip1
```

### Features 说明

| Feature | 说明 | 启用的依赖 |
|---------|------|-----------|
| *(default)* | 纯逻辑模式，不依赖 Extism | cmx-plugin-sdk (default-features = false) |
| `extism` | Extism 插件模式，启用 `#[plugin_fn]` 导出 | extism-pdk, cmx-plugin-sdk/extism |

### 构建命令

#### wasm32-unknown-unknown 目标

```bash
# Debug 构建（包含调试信息）
cargo build --target wasm32-unknown-unknown

# Release 构建（优化体积和性能）
cargo build --release --target wasm32-unknown-unknown
```

#### wasm32-wasip1 目标（需 extism feature）

```bash
# Debug 构建
cargo build --target wasm32-wasip1 --features extism

# Release 构建
cargo build --release --target wasm32-wasip1 --features extism
```

### 构建输出位置

```
target/wasm32-unknown-unknown/debug/cmx_wasmdemo.wasm       # Debug 版本
target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm     # Release 版本
target/wasm32-wasip1/debug/cmx_wasmdemo.wasm                # WASI Debug 版本
target/wasm32-wasip1/release/cmx_wasmdemo.wasm              # WASI Release 版本
```

### Release 优化配置

`Cargo.toml` 中已配置 Release 优化：

```toml
[profile.release]
lto = true          # 链接时优化，减小体积
opt-level = "s"     # 优化体积而非速度
```

### 构建打包示例

```bash
# 构建 WASI Release 版本（推荐用于生产部署）
cargo build --release --target wasm32-wasip1 --features extism

# 查看输出文件大小
ls -lh target/wasm32-wasip1/release/cmx_wasmdemo.wasm

# 复制到部署目录
cp target/wasm32-wasip1/release/cmx_wasmdemo.wasm /path/to/plugins/cmx-wasmdemo/
```

---

## 使用指南

### 数据结构

#### 通用入参/出参

插件函数统一使用 `FunctionInput` / `FunctionOutput`（来自 cmx-plugin-sdk）：

```rust
pub struct FunctionInput {
    pub input: serde_json::Value,                        // 业务输入
    pub context: SVRContext,                             // 上下文（含 initial_input、step_outputs、txn_id 等）
    pub binary_data: HashMap<String, Vec<u8>>,          // 二进制附件（按名称索引）
}
```

#### 业务数据结构

```rust
// 示例请求
pub struct DemoRequest {
    pub name: String,
    pub count: u32,
}

// 示例响应
pub struct DemoResponse {
    pub message: String,
    pub total: u32,
}

// 路由输入
pub struct RouteInput {
    pub route: String,    // "1" | "2" | "3" | "4"
}

// 事务操作数据
pub struct InsertData { pub table: String, pub name: String, pub value: i32, }
pub struct UpdateData { pub table: String, pub name: String, pub value: i32, }
pub struct QueryData  { pub table: String, pub name: String, }
pub struct DeleteData { pub table: String, pub name: String, }
```

### 一、基础函数

#### 1.1 count_vowels — 元音字母统计

统计输入字符串中的元音字母数量。

```json
// 输入
{ "input": "Hello World", "context": {} }
// 输出
{ "count": 3, "total": 3, "input": "Hello World" }
```

#### 1.2 demo_log — 日志演示

调用宿主日志接口，记录 info、error、debug、warn 四个级别的日志。

```json
// 输入
{ "input": "任意内容", "context": {} }
// 输出
{ "message": "日志记录完成", "total": 4 }
```

#### 1.3 demo_cache — 缓存演示

执行缓存写入 + 读取操作。

```json
// 输入
{ "input": {"name": "cache_key", "count": 42}, "context": {} }
// 输出
{ "message": "缓存操作成功: ...", "total": 42 }
```

#### 1.4 demo_database — 数据库演示

执行一条 SELECT 查询。

```json
// 输入
{ "input": {"name": "test_name", "count": 1}, "context": {} }
// 输出
{ "message": "数据库查询成功: ...", "total": 1 }
```

#### 1.5 demo_call_plugin — 插件间调用

通过宿主调用另一个指定插件的函数。

```json
// 输入
{ "input": {"name": "some_data", "count": 10}, "context": {} }
// 输出
{ "message": "调用成功: ...", "total": 10 }
```

#### 1.6 demo_call_service_by_key — 服务编排调用

通过服务键调用服务编排接口。

```json
// 输入
{ "input": {"name": "some_data", "count": 5}, "context": {} }
// 输出
{ "message": "服务执行成功: ...", "total": 5 }
```

#### 1.7 run_all_demos — 综合测试

依次执行日志、缓存、数据库等功能测试。

```json
// 输入
{ "input": {"name": "test", "count": 1}, "context": {} }
// 输出
["日志测试: 成功", "缓存写入测试: 成功", "缓存读取测试: ...", "数据库测试: ..."]
```

### 二、服务编排函数

#### 2.1 route_check — 路由判断

根据输入的 `route` 字段返回分支标识 `"1"` / `"2"` / `"3"` / `"4"`，用于 switch 节点。

```json
// 输入
{ "input": {"route": "2"}, "context": {} }
// 输出
"2"
```

#### 2.2 branch_1/2/3_process — 分支处理

各分支处理函数，返回包含分支标识和处理结果的 JSON。

```json
// 输出
{ "branch": "1", "message": "分支1处理完成", "input": ..., "initial_input": ... }
```

#### 2.3 merge_result — 结果合并

合并各分支的处理结果，从上下文 `step_outputs` 获取各分支输出。

```json
// 输出
{ "merged": true, "branch_output": ..., "message": "结果合并完成" }
```

### 三、事务处理函数

所有事务函数通过上下文 `txn_id` 确保在同一事务中执行。

| 函数 | 操作 | 输入数据结构 |
|------|------|-------------|
| `tx_insert` | INSERT | `InsertData { table, name, value }` |
| `tx_update` | UPDATE | `UpdateData { table, name, value }` |
| `tx_query` | SELECT | `QueryData { table, name }` |
| `tx_delete` | DELETE | `DeleteData { table, name }` |

### 四、final_process — 最终处理

整合所有步骤的输出（merge_result、tx_insert、tx_update、tx_query、tx_delete），同时演示缓存写入和插件调用。

---

## 宿主函数依赖

本插件依赖以下宿主函数（由宿主端通过 cmx-plugin-sdk 的 `HostCaller` 提供）：

| 命名空间 | 函数名 | 用途 |
|----------|--------|------|
| `cmx:log` | `log_info`, `log_error`, `log_debug`, `log_warn` | 日志记录 |
| `cmx:database` | `db_query`, `db_execute` | 数据库操作 |
| `cmx:buffer` | `cache_get`, `cache_set`, `cache_delete` | 缓存操作 |
| `cmx:plugin` | `call_plugin`, `call_service_by_key` | 插件间调用 / 服务编排调用 |
| `cmx:iam` | `iam_query` | 身份与权限查询（本插件未使用，SDK 已提供） |

---

## 开发指南

### 添加新的导出函数

1. 在 `core.rs` 的 `PluginCore<H>` 中实现业务逻辑。
2. 在 `extism_layer.rs` 中添加 `#[plugin_fn]` 入口函数，委托给 `PluginCore`。
3. 如需新的数据结构，在 `models.rs` 中定义。

```rust
// core.rs
impl<H: HostFunctions> PluginCore<H> {
    pub fn my_function(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("my_function called")?;
        Ok(FunctionOutput::from_json(serde_json::json!({"result": "ok"})))
    }
}

// extism_layer.rs
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.my_function(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

### 构建 API 文档

```bash
# 安装 cmx-cli 工具
cargo install --registry nora cmx-cli

# 生成文档
cmx-cli doc
```

---

## 故障排查

### 编译错误: `can't find crate for std`

确保已添加对应的 wasm32 目标：

```bash
rustup target add wasm32-unknown-unknown
rustup target add wasm32-wasip1
```

### 运行时错误: `Host function not found`

检查：
1. 宿主端是否正确注册了 HostCaller 提供者
2. 函数名和命名空间是否匹配

## 相关链接

- [Extism 官方文档](https://extism.org/)
- [Extism Rust PDK](https://github.com/extism/rust-pdk)
- [cmx-plugin-sdk](../cmx-plugin-sdk/)
