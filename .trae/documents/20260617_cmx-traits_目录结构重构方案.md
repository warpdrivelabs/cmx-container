# cmx-traits 目录结构重构方案

> 本方案承接上一会话的实施进度，记录已完成的重构工作及剩余收尾任务。

---

## 一、方案摘要

对 cmx-traits crate 进行目录结构重构，将原本单层平铺的功能源码按领域驱动设计重组为 8 个领域子目录，提升工程结构的可维护性与可读性。

**当前进度**：主体重构已完成（含下游 crate 全量适配），仅剩 README.md 中 6 处 import 路径示例需要更新。

---

## 二、当前状态分析

### 2.1 已完成的工作

通过上一会话的实施，cmx-traits 目录结构已按领域重组完成：

```
crates/libs/cmx-traits/src/
├── lib.rs              # 仅保留 pub mod 声明，移除顶层 pub use 重导出
├── error.rs            # 通用错误类型（TraitError, HostFuncError）保留在根目录
├── auth/               # 认证领域
│   ├── mod.rs
│   ├── error.rs        # AuthError
│   ├── policy.rs       # AuthPolicy
│   ├── service.rs      # AuthService
│   ├── storage_query.rs # AuthStorageQuery
│   └── user_query.rs   # UserAuthQuery
├── iam/                # IAM 领域
│   ├── mod.rs
│   ├── data_scope.rs
│   └── permission_checker.rs
├── plugin/             # 插件领域
│   ├── mod.rs
│   ├── query.rs        # PluginQuery
│   └── lifecycle.rs    # PluginLifecycleListener
├── runtime/            # WASM 运行时领域
│   ├── mod.rs
│   ├── invoker.rs      # RuntimeInvoker
│   ├── host_func.rs    # HostFunctionProvider
│   ├── invoke_context.rs # InvokeContext
│   └── global.rs       # GlobalRuntime
├── service/            # 服务领域
│   ├── mod.rs
│   ├── query.rs        # ServiceQuery
│   ├── storage.rs      # ServiceStorage
│   ├── invoker.rs      # ServiceInvoker
│   └── global_invoker.rs # GlobalServiceInvoker
├── rpc/                # RPC 领域
│   ├── mod.rs
│   └── client.rs       # RpcClient
└── event_bus/          # 事件总线
    ├── mod.rs
    ├── bus.rs          # EventBus
    ├── global.rs       # GlobalEventBus
    └── types.rs        # EventHandler, EventTopic, EventPayload
```

### 2.2 已完成的关键决策

| 决策项 | 选择 | 说明 |
|--------|------|------|
| 重构策略 | 清理式重构 | 移除 lib.rs 顶层 pub use 重导出，下游统一使用完整模块路径 |
| error.rs 位置 | 保留在根目录 | TraitError/HostFuncError 作为通用错误类型保留在 src/error.rs |

### 2.3 已完成的下游适配

以下 crate 的 import 路径已全部更新并通过 `cargo check --workspace` 验证：
- cmx-iam（3 文件）、cmx-biz（3 文件）、cmx-auth（16 文件）、cmx-api（7 文件）
- cmx-runtime（5 文件）、cmx-plugin（9 文件）、cmx-service（12 文件）、cmx-rpc（5 文件）
- cmx-utils（1 文件）、cmx-buffer（1 文件）、cmx-database（1 文件）
- web-server（6 文件）

### 2.4 剩余问题

README.md 使用指南章节中仍有 6 处旧的 `use cmx_traits::{...}` 顶层路径未更新为新模块路径：

| 行号 | 当前代码 | 问题 |
|------|----------|------|
| 263 | `use cmx_traits::{PluginLifecycleListener, PluginLifecyclePayload, plugin_events,};` | 应改为 `cmx_traits::plugin::{...}` |
| 323 | `use cmx_traits::{ServiceQuery, ServiceInfo, ServiceDefinition};` | ServiceQuery 应来自 `cmx_traits::service`；ServiceInfo/ServiceDefinition 实际来自 `cmx_core::model::service` |
| 349 | `use cmx_traits::{ServiceStorage, ServiceDefinition};` | ServiceStorage 应来自 `cmx_traits::service`；ServiceDefinition 来自 cmx_core |
| 375 | `use cmx_traits::{GlobalEventBus, plugin_events};` | GlobalEventBus 来自 `cmx_traits::event_bus`；plugin_events 来自 `cmx_traits::plugin` |
| 397 | `use cmx_traits::{GlobalEventBus, plugin_events, EventBus, EventHandler};` | 同上，需拆分为两条 use 语句 |
| 474 | `use cmx_traits::{TraitError, HostFuncError};` | 应改为 `cmx_traits::error::{TraitError, HostFuncError}` |

---

## 三、拟议变更

### 3.1 更新 README.md 第 263-267 行

**文件**：`crates/libs/cmx-traits/README.md`

**变更前**：
```rust
use cmx_traits::{
    PluginLifecycleListener,
    PluginLifecyclePayload,
    plugin_events,
};
```

**变更后**：
```rust
use cmx_traits::plugin::{
    PluginLifecycleListener,
    PluginLifecyclePayload,
    plugin_events,
};
```

### 3.2 更新 README.md 第 323 行

**变更前**：
```rust
use cmx_traits::{ServiceQuery, ServiceInfo, ServiceDefinition};
```

**变更后**：
```rust
use cmx_traits::service::ServiceQuery;
use cmx_core::model::service::ServiceDefinition;
```

**说明**：ServiceInfo 在代码中并非独立类型（query.rs 中实际返回 ServiceDefinition），移除该示例引用以避免误导。

### 3.3 更新 README.md 第 349 行

**变更前**：
```rust
use cmx_traits::{ServiceStorage, ServiceDefinition};
```

**变更后**：
```rust
use cmx_traits::service::ServiceStorage;
use cmx_core::model::service::ServiceDefinition;
```

### 3.4 更新 README.md 第 375 行

**变更前**：
```rust
use cmx_traits::{GlobalEventBus, plugin_events};
```

**变更后**：
```rust
use cmx_traits::event_bus::GlobalEventBus;
use cmx_traits::plugin::plugin_events;
```

### 3.5 更新 README.md 第 397 行

**变更前**：
```rust
use cmx_traits::{GlobalEventBus, plugin_events, EventBus, EventHandler};
```

**变更后**：
```rust
use cmx_traits::event_bus::{GlobalEventBus, EventBus, EventHandler};
use cmx_traits::plugin::plugin_events;
```

### 3.6 更新 README.md 第 474 行

**变更前**：
```rust
use cmx_traits::{TraitError, HostFuncError};
```

**变更后**：
```rust
use cmx_traits::error::{TraitError, HostFuncError};
```

---

## 四、假设与决策

1. **假设**：上一会话的全量编译验证（`cargo check --workspace` exit code 0）结果仍然有效，本次仅修改 README.md 文档，不影响代码编译。
2. **决策**：ServiceInfo 在 cmx-traits 中并非独立导出类型，README 示例中移除该引用，仅保留实际存在的 ServiceDefinition（来自 cmx_core）。
3. **决策**：不修改 README.md 中其他已正确使用新路径的示例（如第 21、156、179-181、217 行已在上一会话更新完成）。

---

## 五、验证步骤

1. **文档一致性检查**：在 README.md 中搜索 `use cmx_traits::{`，确认不再有顶层路径残留（应返回 0 结果）。
2. **编译验证**：运行 `cargo check -p cmx-traits` 确认 crate 本身编译通过（README 修改不影响编译，但作为基线确认）。
3. **Clippy 检查**：运行 `cargo clippy -p cmx-traits` 确认无警告。
4. **测试验证**：运行 `cargo test -p cmx-traits` 确认 doctest 仍通过（2 个标记为 ignored 的 doctest 预期保持忽略状态）。

---

## 六、实施步骤

1. 使用 Edit 工具依次更新 README.md 的 6 处 import 路径（第 263、323、349、375、397、474 行）。
2. 执行验证步骤 1-4。
3. 返回最终完成报告。
