# WASM 类型重构计划

## 一、现状分析

### 1. 当前文件位置

| 文件 | 路径 | 内容 |
|------|------|------|
| wasm_types.rs | `crates/libs/cmx-core/src/wasm_types.rs` | 宿主端类型定义 |
| host_calls.rs | `crates/libs/cmx-plugin-sdk/src/host_calls.rs` | WASM 端类型定义（重复） |

### 2. 重复定义的 struct

| struct | cmx-core (wasm_types.rs) | cmx-plugin-sdk (host_calls.rs) | 差异 |
|--------|-------------------------|-------------------------------|------|
| DbQueryRequest | ✅ | ✅ | 完全相同 |
| DbResponse | ✅ | ✅ | 完全相同 |
| CacheGetRequest | ✅ | ✅ | 完全相同 |
| CacheSetRequest | ✅ | ✅ | 完全相同 |
| CacheResponse | ✅ | ✅ | 完全相同 |
| PluginCallRequest | ✅ | - | - |
| PluginCallResponse | ✅ | - | - |
| ServiceCallRequest | - | ✅ | 与 PluginCallRequest 相同 |
| ServiceCallResponse | - | ✅ | 与 PluginCallResponse 相同 |
| PluginInfoResponse | ✅ | - | 仅 cmx-core |
| WasmFunctionRequest<T> | ✅ | - | 仅 cmx-core |
| WasmFunctionResponse<T> | ✅ | - | 仅 cmx-core |
| WasmContext | ✅ | - | 仅 cmx-core |

### 3. 命名不一致问题

- `PluginCallRequest` (cmx-core) vs `ServiceCallRequest` (cmx-plugin-sdk)
- `PluginCallResponse` (cmx-core) vs `ServiceCallResponse` (cmx-plugin-sdk)

### 4. 引用位置

| 文件 | 使用方式 |
|------|----------|
| `cmx-database/host_functions.rs` | `cmx_core::wasm_types::{DbQueryRequest, DbResponse}` |
| `cmx-plugin/host_functions.rs` | `cmx_core::wasm_types::{PluginCallRequest, PluginCallResponse, PluginInfoResponse}` |
| `cmx-buffer/host_functions.rs` | 内部定义匿名 struct（未使用 wasm_types） |
| `cmx-wasmdemo/src/lib.rs` | 通过 `cmx_plugin_sdk::DbQueryRequest` 使用 |
| `cmx-plugin-sdk/src/lib.rs` | 重新导出 host_calls 中的 struct |

---

## 二、重构方案

### 方案：在 cmx-core 创建 wasm_types/ 目录，按功能分类解耦

#### 目录结构

```
crates/libs/cmx-core/src/
├── wasm_types/
│   ├── mod.rs           # 模块导出
│   ├── database.rs      # 数据库相关类型
│   ├── cache.rs         # 缓存相关类型
│   ├── plugin.rs        # 插件调用相关类型
│   ├── context.rs       # WASM 上下文类型
│   └── common.rs        # 通用包装类型
└── lib.rs               # 更新导出
```

#### 分类详情

| 文件 | 包含 struct |
|------|-------------|
| database.rs | `DbQueryRequest`, `DbResponse` |
| cache.rs | `CacheGetRequest`, `CacheSetRequest`, `CacheResponse` |
| plugin.rs | `PluginCallRequest`, `PluginCallResponse`, `PluginInfoResponse` |
| context.rs | `WasmContext` |
| common.rs | `WasmFunctionRequest<T>`, `WasmFunctionResponse<T>` |

---

## 三、待确认问题

### 问题 1：命名统一

`PluginCallRequest/PluginCallResponse` vs `ServiceCallRequest/ServiceCallResponse`

- **PluginCallRequest**：强调"插件间调用"
- **ServiceCallRequest**：强调"服务调用"

**建议**：统一使用 `PluginCallRequest/PluginCallResponse`，因为：
1. 与 `PluginInfoResponse` 命名风格一致
2. 更准确地描述了"插件间调用"的语义

### 问题 2：cmx-buffer/host_functions.rs 的处理

当前 `cmx-buffer/host_functions.rs` 内部定义了匿名 struct 而非使用 wasm_types：

```rust
#[derive(serde::Deserialize)]
struct CacheRequest {
    key: String,
}
```

**建议**：重构时统一使用 `cmx_core::wasm_types` 中的类型

### 问题 3：cmx-plugin-sdk/host_calls.rs 的 HostCaller

`HostCaller` 是宿主函数调用封装，包含：
- 日志方法：`log_info`, `log_error`, `log_debug`, `log_warn`
- 数据库方法：`db_query`, `db_execute`
- 缓存方法：`cache_get`, `cache_set`, `cache_delete`
- 插件调用方法：`call_service`

**建议**：保留在 `cmx-plugin-sdk/host_calls.rs`，仅删除重复的 struct 定义，改为从 `cmx_core::wasm_types` 导入

---

## 四、修改清单

### 1. 新建文件（cmx-core）

| 文件 | 操作 |
|------|------|
| `wasm_types/mod.rs` | 新建 |
| `wasm_types/database.rs` | 新建 |
| `wasm_types/cache.rs` | 新建 |
| `wasm_types/plugin.rs` | 新建 |
| `wasm_types/context.rs` | 新建 |
| `wasm_types/common.rs` | 新建 |

### 2. 删除文件

| 文件 | 操作 |
|------|------|
| `cmx-core/src/wasm_types.rs` | 删除（替换为目录） |

### 3. 修改文件

| 文件 | 修改内容 |
|------|----------|
| `cmx-core/src/lib.rs` | 更新模块导出 |
| `cmx-plugin-sdk/src/host_calls.rs` | 删除重复 struct，从 cmx_core 导入 |
| `cmx-plugin-sdk/src/lib.rs` | 更新导出路径 |
| `cmx-buffer/src/host_functions.rs` | 使用 wasm_types 中的类型 |
| `cmx-wasmdemo/src/lib.rs` | 如需调整导入路径 |

---

## 五、执行步骤

1. 在 `cmx-core/src/` 创建 `wasm_types/` 目录
2. 创建 6 个子文件，按分类迁移 struct
3. 更新 `cmx-core/src/lib.rs` 导出
4. 修改 `cmx-plugin-sdk/src/host_calls.rs`，删除重复定义
5. 更新 `cmx-plugin-sdk/src/lib.rs` 导出
6. 修改 `cmx-buffer/src/host_functions.rs` 使用统一类型
7. 编译验证
8. 运行测试

---

## 六、向后兼容

为确保现有代码兼容，在 `wasm_types/mod.rs` 中重新导出所有类型：

```rust
pub mod database;
pub mod cache;
pub mod plugin;
pub mod context;
pub mod common;

pub use database::{DbQueryRequest, DbResponse};
pub use cache::{CacheGetRequest, CacheSetRequest, CacheResponse};
pub use plugin::{PluginCallRequest, PluginCallResponse, PluginInfoResponse};
pub use context::WasmContext;
pub use common::{WasmFunctionRequest, WasmFunctionResponse};
```

同时为 `ServiceCallRequest/ServiceCallResponse` 提供别名（如果需要兼容）：

```rust
/// 服务调用请求（PluginCallRequest 的别名）
pub type ServiceCallRequest = PluginCallRequest;

/// 服务调用响应（PluginCallResponse 的别名）
pub type ServiceCallResponse = PluginCallResponse;
```
