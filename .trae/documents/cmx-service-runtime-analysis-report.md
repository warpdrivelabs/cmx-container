# cmx-service 与 cmx-runtime 模块全面分析报告

## 一、现状评估

### 1.1 模块架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                         web-server                               │
│  (应用入口，组装各组件)                                            │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  cmx-service  │       │  cmx-plugin   │       │  cmx-runtime  │
│  (服务编排)    │◄──────│  (插件管理)    │       │  (WASM运行时) │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │  cmx-traits   │
                        │  (trait 抽象)  │
                        └───────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│ cmx-database  │       │  cmx-buffer   │       │  cmx-utils    │
│ (数据库操作)   │       │  (缓存操作)    │       │  (日志等)      │
└───────────────┘       └───────────────┘       └───────────────┘
                                │
                                ▼
                        ┌───────────────┐
                        │   cmx-core    │
                        │  (基础数据类型) │
                        │  (WASM共享类型) │  ← 支持 wasm32-wasip1
                        └───────────────┘
                                ▲
                                │
                        ┌───────────────┐
                        │ cmx-wasmdemo  │
                        │ (WASM 模块)    │
                        │ target:       │
                        │ wasm32-wasip1 │
                        └───────────────┘
```

### 1.2 模块职责分析

#### 1.2.1 cmx-service 模块

**核心职责：**

* 作为插件编排的执行引擎

* 协调 PluginQuery 和 RuntimeInvoker 完成请求处理

* 提供服务编排功能（Orchestrator）

**主要组件：**

| 组件                     | 文件              | 职责                                |
| ---------------------- | --------------- | --------------------------------- |
| CmxService             | service.rs      | 核心服务结构，实现 PluginLifecycleListener |
| Orchestrator           | orchestrator.rs | 编排执行器，解析和执行编排定义                   |
| ServiceHandler         | handler.rs      | HTTP 处理器，封装服务层逻辑                  |
| InvokeRequest/Response | request.rs      | 请求/响应类型定义                         |

**依赖关系：**

```toml
[dependencies]
cmx-core = { workspace = true }
cmx-traits = { workspace = true }
cmx-database = { workspace = true }
```

**说明：** cmx-service 依赖 cmx-database 用于编排定义持久化等场景，符合设计预期。

#### 1.2.2 cmx-runtime 模块

**核心职责：**

* 基于 wasmtime 的 WASM 运行时引擎

* 加载、编译和实例化 WASM 模块

* 管理宿主函数注册（通过 HostFunctionProvider trait）

* 调用 WASM 导出函数

**主要组件：**

| 组件                   | 文件                 | 职责                                |
| -------------------- | ------------------ | --------------------------------- |
| WasmEngine           | engine.rs          | WASM 引擎核心，实现 RuntimeInvoker trait |
| WasmInstance         | instance.rs        | WASM 实例包装，封装 Instance 和 Store     |
| RuntimeLinkerAdapter | linker\_adapter.rs | Linker 适配器，实现 WasmLinker trait    |
| GlobalWasmEngine     | lib.rs             | 全局单例访问                            |

**依赖关系：**

```toml
[dependencies]
cmx-core = { workspace = true }
cmx-traits = { workspace = true }
cmx-utils = { workspace = true }
wasmtime = { workspace = true }
```

**优点：** 仅依赖 cmx-core, cmx-traits, cmx-utils，不依赖业务模块，符合设计原则。

### 1.3 服务编排功能评估

#### 1.3.1 已实现功能

| 功能         | 实现状态  | 说明                      |
| ---------- | ----- | ----------------------- |
| 步骤顺序执行     | ✅ 已实现 | 按编排定义顺序执行步骤             |
| 步骤间数据引用    | ✅ 已实现 | 支持 Reference 类型引用前序步骤输出 |
| 静态输入       | ✅ 已实现 | 支持 Static 类型输入          |
| 合并输入       | ✅ 已实现 | 支持 Merge 类型合并多个来源       |
| 插件激活检查     | ✅ 已实现 | 执行前检查插件是否激活             |
| WASM 模块懒加载 | ✅ 已实现 | 未加载时自动加载                |
| 步骤执行耗时统计   | ✅ 已实现 | 记录每个步骤的执行时间             |

#### 1.3.2 未实现/不完善功能

| 功能      | 实现状态  | 问题描述                                    |
| ------- | ----- | --------------------------------------- |
| 编排定义持久化 | ❌ 未实现 | `execute_orchestration` 返回"编排定义加载尚未实现"  |
| 条件执行    | ❌ 未实现 | `condition` 字段定义了但未实际使用                 |
| 并行执行    | ❌ 未实现 | `parallel` 字段定义了但未实际使用                  |
| 错误重试机制  | ❌ 未实现 | 步骤失败直接终止，无重试选项                          |
| 编排版本管理  | ❌ 未实现 | 无法管理编排定义的版本演进                           |
| 输入数据传递  | ❌ 未实现 | invoke 方法忽略 `_input` 参数                 |
| 返回值获取   | ❌ 未实现 | 返回 `output: Vec::new()` 未从 WASM 获取实际返回值 |

### 1.4 Host 与 WASM 交互机制评估

#### 1.4.1 当前数据传递协议

**宿主函数签名：**

```rust
// 当前签名
(input_ptr: i32, input_len: i32) -> i32
// 返回值：输出数据长度，负值表示错误
// 数据存储在 OUTPUT_BUFFER 线程局部变量中
```

**问题分析：**

| 问题      | 严重程度 | 说明                                      |
| ------- | ---- | --------------------------------------- |
| 竞态条件风险  | 🔴 高 | OUTPUT\_BUFFER 是线程局部变量，多实例调用会互相覆盖       |
| 两次调用开销  | 🟡 中 | 需要额外调用 get\_output 获取结果                 |
| 内存不安全   | 🔴 高 | WASM 不知道需要分配多少内存来接收结果                   |
| 类型不安全   | 🟡 中 | 所有数据通过 `&[u8]` 传递，需要手动 JSON 序列化         |
| 输入数据未传递 | 🔴 高 | engine.rs 的 invoke 方法忽略 `_input` 参数     |
| 返回值未获取  | 🔴 高 | 返回 `output: Vec::new()` 未从 WASM 获取实际返回值 |

#### 1.4.2 Host → Guest 调用链路分析

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ CmxService  │────►│RuntimeInvoker│────►│ WasmEngine  │────►│ WasmInstance│
│ .invoke()   │     │ .invoke()    │     │ .invoke()   │     │ .get_func() │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │                   │
       │ 序列化输入         │ _input 被忽略      │ input_ptr=0       │
       │ 为 JSON 字节      │ ❌ 问题点          │ input_len=0       │
       │                   │                   │ ❌ 问题点          │
       ▼                   ▼                   ▼                   ▼
```

**问题定位：**

* [engine.rs:188-189](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-runtime/src/engine.rs#L188-189) 硬编码 `input_ptr = 0` 和 `input_len = 0`

* [engine.rs:199](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-runtime/src/engine.rs#L199) 返回空 `output: Vec::new()`

#### 1.4.3 Guest → Host 调用链路分析

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   WASM      │────►│RuntimeLinker│────►│HostFuncWrapper│───►│HostFunction │
│  调用宿主函数 │     │ Adapter     │     │ 闭包        │     │ Provider    │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │                   │
       │ (ptr, len)        │ 读取 WASM 内存    │ 执行业务逻辑      │
       │                   │ 解析 JSON         │ 返回 Vec<u8>      │
       ▼                   ▼                   ▼                   ▼
```

**当前实现问题：**

* OUTPUT\_BUFFER 线程局部变量存在竞态条件风险

* WASM 端需要两次调用才能获取结果（先调用函数获取长度，再调用 get\_output 获取数据）

### 1.5 Host 函数注册机制评估

#### 1.5.1 当前命名空间

| 命名空间         | 模块           | 提供的函数                                                            |
| ------------ | ------------ | ---------------------------------------------------------------- |
| cmx:log      | cmx-utils    | info, warn, error                                                |
| cmx:database | cmx-database | execute\_sql, query\_sql, txn\_begin, txn\_commit, txn\_rollback |
| cmx:buffer   | cmx-buffer   | cache\_get, cache\_set, cache\_delete                            |
| cmx:plugin   | cmx-plugin   | call\_service, get\_info                                         |

#### 1.5.2 注册流程分析

**优点：**

1. 通过 `HostFunctionProvider` trait 实现解耦
2. 各模块独立实现自己的宿主函数
3. 运行时统一注册，便于管理

**问题：**

| 问题        | 说明                                     |
| --------- | -------------------------------------- |
| 请求/响应结构分散 | 每个 HostFunctionProvider 内部定义自己的请求/响应结构 |
| 缺少类型共享    | 无法在 WASM 端复用宿主端的类型定义                   |
| 缺少权限控制    | 任何 WASM 都可以调用所有宿主函数                    |
| 缺少版本管理    | 宿主函数签名变更无法感知                           |

### 1.6 cmx-core 模块评估

#### 1.6.1 当前内容

| 目录/文件                | 内容                                                     |
| -------------------- | ------------------------------------------------------ |
| model/cell.rs        | DataValue, Field, FieldType, ColumnDefine, TableDefine |
| model/data/dataset/  | Schema, DataSet, Row                                   |
| model/data/response/ | RestResponse, CMXResponse trait                        |
| model/data/request/  | 请求参数类型                                                 |
| model/domain/        | 领域实体                                                   |
| model/meta/          | 元数据定义，包括 PluginDefinition, PluginManifest              |

#### 1.6.2 WASM 兼容性

**当前依赖：**

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
smol_str = { version = "0.3", features = ["serde"] }
rust_decimal = "1"
thiserror = "2"
strum = "0.27"
strum_macros = "0.27"
uuid = { version = "1.21", features = ["v4", "serde"] }
base64 = "0.22"
```

**评估结果：** 所有依赖都支持 `wasm32-wasip1` 目标，cmx-core 可作为 WASM 和宿主的共享类型库。

#### 1.6.3 问题

| 问题            | 说明                             |
| ------------- | ------------------------------ |
| 缺少 WASM 端调用封装 | cmx-core 不包含 WASM 端调用宿主函数的便捷封装 |
| 缺少宿主函数请求/响应类型 | 各模块各自定义，无法共享                   |

***

## 二、问题清单

### 2.1 严重问题（P0）

| 编号   | 问题                  | 影响             | 涉及模块        |
| ---- | ------------------- | -------------- | ----------- |
| P0-1 | invoke 方法未传递输入数据    | WASM 无法接收输入参数  | cmx-runtime |
| P0-2 | invoke 方法未获取返回值     | 无法获取 WASM 执行结果 | cmx-runtime |
| P0-3 | OUTPUT\_BUFFER 竞态条件 | 多线程/多实例数据覆盖    | cmx-runtime |
| P0-4 | 编排定义无法持久化           | 无法保存和加载编排定义    | cmx-service |

### 2.2 重要问题（P1）

| 编号   | 问题              | 影响       | 涉及模块        |
| ---- | --------------- | -------- | ----------- |
| P1-1 | 条件执行未实现         | 无法动态跳过步骤 | cmx-service |
| P1-2 | 并行执行未实现         | 无法优化执行效率 | cmx-service |
| P1-3 | 错误重试未实现         | 可靠性不足    | cmx-service |
| P1-4 | 缺少宿主函数请求/响应共享类型 | 类型重复定义   | cmx-core    |

### 2.3 一般问题（P2）

| 编号   | 问题            | 影响       | 涉及模块        |
| ---- | ------------- | -------- | ----------- |
| P2-1 | 缺少权限控制        | 安全风险     | cmx-runtime |
| P2-2 | 缺少资源限制        | 可能资源滥用   | cmx-runtime |
| P2-3 | 缺少 WASM 端调用封装 | 开发体验差    | cmx-core    |
| P2-4 | 缺少编排版本管理      | 无法演进编排定义 | cmx-service |

***

## 三、功能需求

### 3.1 服务编排功能需求

#### 3.1.1 编排定义持久化

**需求描述：**
支持将编排定义保存到数据库，并支持按 ID 和版本加载。

**数据库表设计：**

```sql
CREATE TABLE orchestration_def (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    version INT NOT NULL DEFAULT 1,
    rkyv_version VARCHAR(32) NOT NULL,  -- rkyv 版本号
    definition BYTEA NOT NULL,          -- rkyv 序列化数据
    is_active BOOLEAN DEFAULT true,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

**API 设计：**

```rust
/// 编排定义注册表
pub trait OrchestrationRegistry {
    /// 保存编排定义
    async fn save(&self, orchestration: &Orchestration) -> Result<(), Error>;
    
    /// 加载编排定义
    async fn load(&self, id: &str, version: Option<u32>) -> Result<Option<Orchestration>, Error>;
    
    /// 列出所有编排
    async fn list(&self, filter: OrchestrationFilter) -> Result<Vec<Orchestration>, Error>;
    
    /// 删除编排
    async fn delete(&self, id: &str) -> Result<(), Error>;
}
```

#### 3.1.2 条件执行

**需求描述：**
支持条件表达式，根据前序步骤的执行结果决定是否执行当前步骤。

**条件表达式语法：**

```json
{
  "step_id": "step2",
  "condition": "$step1.success == true && $step1.output.count > 10",
  "plugin_id": "plugin-b",
  "function_name": "process"
}
```

**支持的表达式：**

* 步骤状态：`$step_id.success`、`$step_id.error`

* 输出字段：`$step_id.output.field`

* 比较运算：`==`、`!=`、`>`、`<`、`>=`、`<=`

* 逻辑运算：`&&`、`||`、`!`

#### 3.1.3 并行执行

**需求描述：**
支持多个步骤并行执行，提升编排执行效率。

**并行组设计：**

```json
{
  "steps": [
    { "step_id": "a", "parallel_group": "group1", ... },
    { "step_id": "b", "parallel_group": "group1", ... },
    { "step_id": "c", "parallel_group": "group1", ... },
    { "step_id": "d", "depends_on": ["group1"], ... }
  ]
}
```

**执行策略：**

* 同一 `parallel_group` 的步骤并行执行

* `depends_on` 指定依赖的步骤或组

* 组内任一步骤失败可选择终止或继续

#### 3.1.4 错误重试

**需求描述：**
支持步骤执行失败时的自动重试机制。

**重试配置：**

```json
{
  "step_id": "step1",
  "retry": {
    "max_attempts": 3,
    "backoff": "exponential",
    "initial_delay_ms": 100,
    "max_delay_ms": 5000,
    "retry_on": ["timeout", "temporary_error"]
  }
}
```

### 3.2 Host 与 WASM 交互需求

#### 3.2.1 数据传递协议优化

**需求描述：**
设计安全、高效的数据传递协议，消除竞态条件和内存安全问题。

**新签名设计：**

```rust
// 新签名：直接写入 WASM 内存
(input_ptr: i32, input_len: i32, output_ptr: i32, output_capacity: i32) -> i32
// 返回值：
// - 正数: 实际写入的字节数
// - 负数: 需要的缓冲区大小（容量不足时）
```

**优点：**

1. 消除竞态条件：数据直接写入 WASM 提供的缓冲区
2. 单次调用：不需要额外的 get\_output 调用
3. 内存安全：WASM 知道自己缓冲区大小

#### 3.2.2 输入输出传递修复

**需求描述：**
修复 WasmEngine 的 invoke 方法，正确传递输入数据并获取返回值。

**修改点：**

1. 将输入数据写入 WASM 线性内存
2. 调用 WASM 函数时传递正确的指针和长度
3. 从 WASM 线性内存读取返回值

#### 3.2.3 wasmtime + rkyv + Arena 技术栈

**需求描述：**
采用 wasmtime + rkyv + Arena 技术栈实现高效数据传递。

**技术方案：**

* **wasmtime**: WASM 运行时引擎

* **rkyv**: 零拷贝序列化框架，比 JSON 快 10x 以上

* **Arena**: 内存分配器，减少内存碎片

**数据传递流程：**

```
┌─────────────┐                    ┌─────────────┐
│   WASM      │                    │    Host     │
│             │                    │             │
│ 1. 准备请求  │                    │             │
│    数据     │                    │             │
│ (cmx-core   │                    │             │
│  类型)      │                    │             │
│             │                    │             │
│ 2. rkyv序列化│                    │             │
│    到Arena  │                    │             │
│             │                    │             │
│ 3. 调用宿主  │ ──────────────────►│ 4. 零拷贝读取│
│    函数     │  (ptr, len,        │    输入数据 │
│             │   out_ptr, cap)    │             │
│             │                    │ 5. 执行逻辑 │
│             │                    │             │
│             │                    │ 6. rkyv序列化│
│             │                    │    到Arena  │
│             │◄────────────────── │             │
│ 7. 零拷贝读取│   返回写入字节数    │             │
│    响应数据 │                    │             │
└─────────────┘                    └─────────────┘
```

### 3.3 Host 函数注册需求

#### 3.3.1 共享类型定义

**需求描述：**
在 cmx-core 中定义所有宿主函数的请求/响应类型，供宿主和 WASM 共同使用。

**类型定义位置：**

```
cmx-core/
├── src/
│   ├── wasm/           # 新增：WASM 共享类型
│   │   ├── mod.rs
│   │   ├── request.rs  # 请求类型定义
│   │   ├── response.rs # 响应类型定义
│   │   └── error.rs    # WASM 错误类型
```

#### 3.3.2 权限控制

**需求描述：**
支持插件级别的宿主函数调用权限控制。

**权限配置：**

```json
{
  "plugin_id": "my-plugin",
  "permissions": {
    "cmx:database": ["query_sql"],
    "cmx:buffer": ["cache_get", "cache_set"],
    "cmx:plugin": []
  }
}
```

#### 3.3.3 命名规范

**需求描述：**
统一宿主函数命名规范，便于管理和文档生成。

**命名规范：**

* 命名空间：`cmx:{模块名}`，如 `cmx:database`、`cmx:buffer`

* 函数名：小写下划线风格，如 `query_sql`、`cache_get`

* 完整名称：`{命名空间}/{函数名}`，如 `cmx:database/query_sql`

***

## 四、技术约束

### 4.1 模块依赖约束

| 模块           | 允许依赖                                      | 禁止依赖                                              |
| ------------ | ----------------------------------------- | ------------------------------------------------- |
| cmx-core     | serde, chrono, uuid 等基础库                  | 业务模块                                              |
| cmx-traits   | cmx-core                                  | 业务模块                                              |
| cmx-runtime  | cmx-core, cmx-traits, cmx-utils, wasmtime | cmx-database, cmx-buffer, cmx-plugin, cmx-service |
| cmx-service  | cmx-core, cmx-traits, cmx-database        | 无                                                 |
| cmx-wasmdemo | cmx-core                                  | 无限制                                               |

### 4.2 WASM 编译约束

**目标平台：** `wasm32-wasip1`（WASI Preview 1）

**约束：**

1. cmx-core 必须支持 `wasm32-wasip1` 目标编译
2. 所有依赖必须支持 WASM 目标
3. 不使用 `no_std`，利用 WASI 的标准库支持

### 4.3 数据传递约束

**技术栈：** wasmtime + rkyv + Arena

**约束：**

1. Host → Guest 和 Guest → Host 均使用此技术栈
2. rkyv 版本必须在 Host 和 Guest 之间保持一致
3. 使用 bytecheck 验证数据对齐
4. Arena 定期重置以避免内存碎片

### 4.4 性能约束

| 指标         | 目标值             |
| ---------- | --------------- |
| 单次宿主函数调用延迟 | < 1ms           |
| 编排执行吞吐量    | > 1000 steps/s  |
| rkyv 序列化性能 | 比 JSON 快 10x 以上 |
| 零拷贝反序列化    | 无额外开销           |

***

## 五、文件修改清单

| 模块           | 文件                     | 修改类型 | 说明             |
| ------------ | ---------------------- | ---- | -------------- |
| cmx-core     | src/wasm/mod.rs        | 新增   | WASM 共享类型模块    |
| cmx-core     | src/wasm/request.rs    | 新增   | 请求类型定义         |
| cmx-core     | src/wasm/response.rs   | 新增   | 响应类型定义         |
| cmx-core     | src/wasm/caller.rs     | 新增   | WASM 端调用封装     |
| cmx-core     | src/wasm/error.rs      | 新增   | WASM 错误类型      |
| cmx-traits   | src/host\_func.rs      | 修改   | 更新宿主函数签名       |
| cmx-runtime  | src/linker\_adapter.rs | 修改   | 适配新签名，支持 rkyv  |
| cmx-runtime  | src/engine.rs          | 修改   | 实现输入输出传递       |
| cmx-runtime  | src/arena.rs           | 新增   | Arena 内存管理     |
| cmx-database | src/host\_functions.rs | 修改   | 使用 cmx-core 类型 |
| cmx-buffer   | src/host\_functions.rs | 修改   | 使用 cmx-core 类型 |
| cmx-utils    | src/host\_functions.rs | 修改   | 使用 cmx-core 类型 |
| cmx-plugin   | src/host\_functions.rs | 修改   | 使用 cmx-core 类型 |
| cmx-service  | src/orchestrator.rs    | 修改   | 实现条件/并行/重试     |
| cmx-service  | src/registry.rs        | 新增   | 编排定义持久化        |
| cmx-wasmdemo | Cargo.toml             | 修改   | 添加 cmx-core 依赖 |
| cmx-wasmdemo | src/lib.rs             | 修改   | 使用 cmx-core 类型 |
| cmx-wasmdemo | src/demo.rs            | 修改   | 使用类型安全调用       |

***

## 六、优先级建议

### P0 - 核心功能（必须实现）

1. **修复 invoke 输入输出传递**：修复 WasmEngine 的 invoke 方法
2. **新数据传递协议**：消除竞态条件，提升安全性
3. **cmx-core 共享类型**：添加 WASM 共享类型

### P1 - 重要功能（建议实现）

1. **编排持久化**：实现编排定义的保存和加载
2. **条件执行**：实现步骤条件跳过
3. **WASM 端调用封装**：简化 WASM 开发体验

### P2 - 增强功能（可选实现）

1. **并行执行**：优化编排执行效率
2. **错误重试**：提升系统可靠性
3. **权限控制**：增强安全性
4. **资源限制**：防止资源滥用

***

## 七、验收标准

### 7.1 数据传递

* [ ] 宿主函数使用新签名，无竞态条件

* [ ] WASM 调用宿主函数可正确传递输入数据

* [ ] 宿主函数返回值可正确传递给 WASM

* [ ] 单元测试覆盖所有数据传递场景

* [ ] rkyv 序列化比 JSON 快 10x 以上

### 7.2 类型共享

* [ ] cmx-core 可编译为 wasm32-wasip1 目标

* [ ] 所有宿主函数使用 cmx-core 中定义的请求/响应类型

* [ ] WASM 端可直接使用 cmx-core 类型，无需重复定义

### 7.3 编排功能

* [ ] 编排定义可持久化到数据库

* [ ] 条件表达式可正确解析和执行

* [ ] 并行步骤可同时执行

* [ ] 错误重试可按配置执行

### 7.4 模块解耦

* [ ] cmx-runtime 不依赖任何业务模块

* [ ] 所有跨模块交互通过 cmx-traits 的 trait 进行

***

## 八、风险与缓解

| 风险             | 影响                      | 缓解措施               |
| -------------- | ----------------------- | ------------------ |
| cmx-core 依赖兼容性 | 某些依赖可能不支持 wasm32-wasip1 | 仔细验证每个依赖，必要时寻找替代方案 |
| rkyv 版本兼容性     | Guest 和 Host 版本必须一致     | 锁定相同版本，添加版本检查      |
| 内存对齐问题         | rkyv 要求数据对齐             | 使用 bytecheck 验证    |
| 并行执行复杂性        | 可能引入新的并发问题              | 充分测试，提供回退选项        |
| Arena 内存碎片     | 长期运行可能产生碎片              | 定期重置 Arena，监控内存使用  |

***

## 九、附录

### 9.1 相关文档

* [wasm-service-orchestration-requirements.md](./wasm-service-orchestration-requirements.md)

* [wasm-host-function-optimization-plan.md](./wasm-host-function-optimization-plan.md)

* [cmx-core/zerocopy.md](../../crates/libs/cmx-core/zerocopy.md)

### 9.2 编译命令参考

```bash
# 安装 WASI 目标
rustup target add wasm32-wasip1

# 编译 cmx-core 为 WASM（验证兼容性）
cd crates/libs/cmx-core
cargo build --target wasm32-wasip1

# 编译 cmx-wasmdemo
cd crates/libs/cmx-wasmdemo
cargo build --release --target wasm32-wasip1

# 输出文件
# target/wasm32-wasip1/release/cmx_wasmdemo.wasm
```

### 9.3 关键代码位置

| 功能                | 文件位置                               | 行号      |
| ----------------- | ---------------------------------- | ------- |
| invoke 输入忽略       | cmx-runtime/src/engine.rs          | 188-189 |
| invoke 返回空值       | cmx-runtime/src/engine.rs          | 199     |
| OUTPUT\_BUFFER 定义 | cmx-runtime/src/linker\_adapter.rs | 34-35   |
| 编排持久化未实现          | cmx-service/src/handler.rs         | 140-145 |
| 条件执行未使用           | cmx-service/src/orchestrator.rs    | 30-31   |
| 并行执行未使用           | cmx-service/src/orchestrator.rs    | 28-29   |

