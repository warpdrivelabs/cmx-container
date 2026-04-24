# 服务编排调试模式 (Debug Mode) 实现方案

## 一、需求概述

在 cmx-service 的 orchestrator 服务编排执行引擎中增加**调试模式**功能：
- 调用 cmx-api 的服务执行接口时，支持传入 `debug` 开关和 `debug_node_id` 参数
- 当编排执行到达 `debug_node_id` 指定的节点时，**暂停执行**，不再调用该节点的 WASM 函数
- 转而调用 `debug_prepare.rs` 进行调试准备工作
- 准备工作完成后，构造一个包含上一步执行结果、服务 `initial_input`、以及调试准备新增返回值的响应返回给前端

## 二、整体架构设计

### 2.1 数据流

```
前端请求 (debug=true, debug_node_id="node-3")
    │
    ▼
cmx-api handler.rs (execute_service_inner)
    │  解析 debug 参数，传递给 Orchestrator
    ▼
Orchestrator.execute_service()
    │  主循环执行节点
    │  node-1 (start) → 跳转
    │  node-2 (func)  → 正常执行 WASM 函数
    │  node-3 (func)  → 检测到 debug_node_id 匹配！
    │     │
    │     ▼
    │  DebugPrepare::prepare()
    │     ├── 通过 PluginQuery 获取插件详情 (PluginSnapshot)
    │     ├── 通过 cmx-debug 获取 code-server URL
    │     ├── 组装调试信息 (插件名称/版本/函数名/WASM路径/源码路径等)
    │     └── 返回 DebugPrepareResult
    │
    ▼
构造 OrchestrationResult (success=true, output=调试信息JSON)
    │
    ▼
前端收到响应：
{
    "success": true,
    "debug_triggered": true,
    "output": {
        "previous_output": { ... },      // 上一步执行结果
        "initial_input": { ... },        // 服务初始输入
        "code_server_url": "https://...", // coder 网页地址
        "plugin": { ... },               // 插件详细信息
        "node_info": { ... }             // 当前调试节点信息
    },
    "steps": [ ... ]                     // 已执行的步骤
}
```

### 2.2 涉及模块及文件变更

| 模块 | 文件 | 变更类型 | 说明 |
|------|------|----------|------|
| cmx-service (types) | `orchestrator/types.rs` | **修改** | 新增 `DebugOptions`、`DebugPrepareResult`、`StepStatus::DebugPaused` 等类型 |
| cmx-service (executor) | `orchestrator/executor.rs` | **修改** | 主循环中增加 debug 节点拦截逻辑 |
| cmx-service (新增) | `orchestrator/debug_prepare.rs` | **新增** | 调试准备模块，通过 PluginQuery 获取插件详情 + code-server URL |
| cmx-service (mod) | `orchestrator/mod.rs` | **修改** | 注册 debug_prepare 子模块 |
| cmx-api (models) | `handlers/service/models.rs` | **修改** | `ServiceExecuteRequest` 增加 debug 字段；新增/修改响应类型 |
| cmx-api (handler) | `handlers/service/handler.rs` | **修改** | `execute_service_inner` 传递 debug 参数 |
| cmx-traits | `src/plugin_query.rs` | **修改** | `PluginSnapshot` 新增 `source_path` 字段 |
| cmx-plugin | 实现层 | **修改** | 构建 PluginSnapshot 时填充 `source_path` |
| cmx-service (Cargo) | `Cargo.toml` | **修改** | 添加 `cmx-debug` 依赖（仅用于 `get_code_server_url_async`） |

## 三、详细设计

### 3.1 类型定义变更 (`cmx-service/orchestrator/types.rs`)

#### 3.1.1 新增 `DebugOptions`

```rust
#[derive(Debug, Clone, Default)]
pub struct DebugOptions {
    pub debug: bool,
    pub debug_node_id: Option<String>,
}

impl DebugOptions {
    pub fn new(debug: bool, debug_node_id: Option<String>) -> Self {
        Self { debug, debug_node_id }
    }

    pub fn is_debug_enabled(&self) -> bool {
        self.debug && self.debug_node_id.is_some()
    }

    pub fn is_debug_node(&self, node_id: &str) -> bool {
        self.is_debug_enabled() && self.debug_node_id.as_deref() == Some(node_id)
    }
}
```

#### 3.1.2 修改 `ExecuteOptions`

在现有 `ExecuteOptions` 中增加 `debug_options` 字段：

```rust
#[derive(Debug, Clone, Default)]
pub struct ExecuteOptions {
    pub include_steps: bool,
    pub debug_options: DebugOptions,
}

impl ExecuteOptions {
    pub fn new(include_steps: bool) -> Self {
        Self {
            include_steps,
            debug_options: DebugOptions::default(),
        }
    }

    pub fn with_debug(mut self, debug: bool, debug_node_id: Option<String>) -> Self {
        self.debug_options = DebugOptions::new(debug, debug_node_id);
        self
    }
}
```

#### 3.1.3 新增 `DebugPrepareResult`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugPrepareResult {
    pub code_server_url: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub plugin_status: String,
    pub plugin_install_path: String,
    pub plugin_wasm_path: Option<String>,
    pub plugin_type: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub function_name: String,
    pub source_path: Option<String>,
    pub node_id: String,
    pub node_name: String,
}
```

#### 3.1.4 修改 `StepStatus`

在现有枚举中增加 `DebugPaused` 变体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    DebugPaused,
}
```

#### 3.1.5 修改 `OrchestrationResult`

增加 debug 相关字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationResult {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub steps: Vec<ExecutionStep>,
    pub total_elapsed_us: u64,
    pub error: Option<OrchestrationError>,
    // 新增 debug 字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_triggered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_prepare_result: Option<DebugPrepareResult>,
}
```

### 3.2 新增 `debug_prepare.rs` (`cmx-service/orchestrator/debug_prepare.rs`)

这是调试准备模块，负责在调试节点处进行准备工作。**插件详细信息通过 `PluginQuery` trait（cmx-traits 封装，cmx-plugin 实现）获取**，code-server URL 通过 `cmx_debug::get_code_server_url_async()` 获取：

```rust
use std::sync::Arc;

use cmx_core::model::service::ServiceNode;
use cmx_traits::PluginQuery;
use tracing::debug;

use crate::error::ServiceError;
use super::types::DebugPrepareResult;

pub struct DebugPrepare<'a> {
    plugin_query: &'a Arc<dyn PluginQuery>,
}

impl<'a> DebugPrepare<'a> {
    pub fn new(plugin_query: &'a Arc<dyn PluginQuery>) -> Self {
        Self { plugin_query }
    }

    /// 执行调试准备工作
    ///
    /// 1. 从节点元信息获取 plugin_id + function_name
    /// 2. 通过 PluginQuery.get_plugin() 获取插件详细信息（PluginSnapshot，由 cmx-plugin 提供）
    /// 3. 通过 cmx_debug::get_code_server_url_async() 获取 code-server URL
    /// 4. 组装返回结果
    pub async fn prepare(
        &self,
        node: &ServiceNode,
    ) -> Result<DebugPrepareResult, ServiceError> {
        let node_data = node.data.as_ref()
            .ok_or_else(|| ServiceError::InternalError(
                format!("调试节点 {} 缺少 data", node.id)
            ))?;

        let node_meta = node_data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError(
                format!("调试节点 {} 缺少 nodeMeta", node.id)
            ))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;

        debug!(
            "[debug-prepare] 准备调试: node_id={}, plugin_id={}, function={}",
            node.id, plugin_id, function_name
        );

        // 通过 PluginQuery (cmx-traits trait, cmx-plugin 实现) 获取插件详细信息
        let plugin_snapshot = self.plugin_query.get_plugin(plugin_id).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(
                format!("插件 {} 未找到", plugin_id)
            ))?;

        // 通过 cmx-debug 获取 code-server URL
        let code_server_url = cmx_debug::get_code_server_url_async().await;

        debug!(
            "[debug-prepare] 调试准备完成: code_server_url={}, source_path={:?}",
            code_server_url, plugin_snapshot.source_path
        );

        Ok(DebugPrepareResult {
            code_server_url,
            plugin_id: plugin_snapshot.plugin_id,
            plugin_name: plugin_snapshot.name,
            plugin_version: plugin_snapshot.version,
            plugin_status: plugin_snapshot.status,
            plugin_install_path: plugin_snapshot.install_path,
            plugin_wasm_path: plugin_snapshot.wasm_path,
            plugin_type: plugin_snapshot.plugin_type,
            domain_code: plugin_snapshot.domain_code,
            application_code: plugin_snapshot.application_code,
            module_code: plugin_snapshot.module_code,
            source_path: plugin_snapshot.source_path,
            function_name: function_name.clone(),
            node_id: node.id.clone(),
            node_name: node_data.name.clone(),
        })
    }
}
```

### 3.3 修改 `executor.rs` (`cmx-service/orchestrator/executor.rs`)

在主执行循环中增加 debug 拦截逻辑。核心变更点：

1. 在 `Orchestrator` 中增加对 `DebugOptions` 的检查
2. 在 `skylake-func` 和 `skylake-switch` 节点执行前，判断当前节点是否是 debug 目标节点
3. 如果是，则调用 `DebugPrepare::prepare()`，然后中断循环并返回调试结果

主循环中 `skylake-func` 分支的修改示意：

```rust
"skylake-func" => {
    // ===== 新增：调试拦截 =====
    if options.debug_options.is_debug_node(&current_node_id) {
        let previous_output = exec_context.current_output.clone();
        let debug_prepare = DebugPrepare::new(&self.plugin_query);
        let prepare_result = debug_prepare.prepare(node).await?;

        // 记录 debug 暂停步骤
        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node.data.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
            node_type: node.node_type.clone(),
            status: StepStatus::DebugPaused,
            output: None,
            elapsed_us: 0,
            error: None,
            previous_output: Some(previous_output),
        });

        // 构建调试输出（包含 previous_output + initial_input + debug 信息）
        let debug_output = serde_json::json!({
            "previous_output": exec_context.current_output,
            "initial_input": exec_context.svr_context.initial_input,
            "debug_info": prepare_result,
        });

        return Ok(OrchestrationResult {
            success: true,
            output: Some(debug_output),
            steps,
            total_elapsed_us: start_time.elapsed().as_micros() as u64,
            error: None,
            debug_triggered: Some(true),
            debug_prepare_result: Some(prepare_result),
        });
    }
    // ===== 原有逻辑 =====
    let previous_output = exec_context.current_output.clone();
    result = node_handler.execute_node(
        node, &mut exec_context, &mut steps, options.include_steps
    ).await;
    // ... 后续不变
}
```

同样的逻辑也需要应用到 `skylake-switch` 分支。

### 3.4 修改 `mod.rs` (`cmx-service/orchestrator/mod.rs`)

新增 debug_prepare 模块声明：

```rust
mod debug_prepare;
```

### 3.5 修改 `Cargo.toml` (`cmx-service/Cargo.toml`)

新增 `cmx-debug` 依赖：

```toml
cmx-debug = { workspace = true }
```

### 3.6 修改 `cmx-api/models.rs`

#### 3.6.1 修改 `ServiceExecuteRequest`

增加 debug 相关字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceExecuteRequest {
    pub service_key: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub include_steps: Option<bool>,
    // 新增
    #[serde(default)]
    pub debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_node_id: Option<String>,
}
```

#### 3.6.2 修改 `ServiceExecuteResponse`

增加 debug 相关字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceExecuteResponse {
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub steps: Vec<ServiceExecutionStep>,
    pub total_elapsed_us: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceOrchestrationError>,
    // 新增
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_triggered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_prepare_result: Option<ServiceDebugPrepareResult>,
}
```

#### 3.6.3 新增 `ServiceDebugPrepareResult`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceDebugPrepareResult {
    pub code_server_url: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub plugin_status: String,
    pub plugin_install_path: String,
    pub plugin_wasm_path: Option<String>,
    pub plugin_type: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub function_name: String,
    pub source_path: Option<String>,
    pub node_id: String,
    pub node_name: String,
}
```

### 3.7 修改 `cmx-api/handler.rs`

修改 `execute_service_inner` 函数签名和实现，传递 debug 参数：

```rust
async fn execute_service_inner(
    state: &CmxAppState,
    service_key: &str,
    svr_context: SVRContext,
    include_steps: bool,
    debug: bool,                    // 新增
    debug_node_id: Option<String>,  // 新增
) -> Result<ServiceExecuteResponse, Error> {
    // ... 获取依赖不变 ...

    let mut options = cmx_service::ExecuteOptions::new(include_steps)
        .with_debug(debug, debug_node_id);

    let result = orchestrator.execute_service(
        service_key,
        svr_context,
        options,
    ).await
    // ... 错误处理不变 ...

    // 构建响应时增加 debug 字段
    let response = ServiceExecuteResponse {
        success: result.success,
        output: result.output,
        steps: result.steps.into_iter().map(|s| ServiceExecutionStep {
            // ... 不变 ...
            status: match s.status {
                cmx_service::StepStatus::Success => "Success".to_string(),
                cmx_service::StepStatus::Failed => "Failed".to_string(),
                cmx_service::StepStatus::Skipped => "Skipped".to_string(),
                cmx_service::StepStatus::DebugPaused => "DebugPaused".to_string(),
            },
            // ...
        }).collect(),
        total_elapsed_us: result.total_elapsed_us,
        error: result.error.map(|e| ServiceOrchestrationError {
            message: e.message,
        }),
        debug_triggered: result.debug_triggered,
        debug_prepare_result: result.debug_prepare_result.map(|d| ServiceDebugPrepareResult {
            code_server_url: d.code_server_url,
            plugin_id: d.plugin_id,
            plugin_name: d.plugin_name,
            plugin_version: d.plugin_version,
            plugin_status: d.plugin_status,
            plugin_install_path: d.plugin_install_path,
            plugin_wasm_path: d.plugin_wasm_path,
            plugin_type: d.plugin_type,
            domain_code: d.domain_code,
            application_code: d.application_code,
            module_code: d.module_code,
            function_name: d.function_name,
            source_path: d.source_path,
            node_id: d.node_id,
            node_name: d.node_name,
        }),
    };

    Ok(response)
}
```

同时修改 `execute_service` 和 `execute_service_by_key` 两个 handler，从请求中提取 debug 参数并传递。

## 四、API 接口变更

### 4.1 请求示例

```json
POST /api/service/execute
{
    "service_key": "order-process",
    "input": {"order_id": "12345"},
    "include_steps": true,
    "debug": true,
    "debug_node_id": "node-3"
}
```

### 4.2 响应示例（触发调试）

```json
{
    "code": 0,
    "msg": "success",
    "data": {
        "success": true,
        "debug_triggered": true,
        "output": {
            "previous_output": {"processed": true, "order_id": "12345"},
            "initial_input": {"order_id": "12345"},
            "debug_info": {
                "code_server_url": "https://dev.cloudmatrix.one:18080",
                "plugin_id": "order-plugin",
                "plugin_name": "订单处理插件",
                "plugin_version": "1.0.0",
                "plugin_status": "activated",
                "plugin_install_path": "/path/to/plugins/order-plugin",
                "plugin_wasm_path": "main.wasm",
                "plugin_type": "wasm",
                "domain_code": "ecommerce",
                "application_code": "order",
                "module_code": "process",
                "function_name": "process_order",
                "source_path": "/path/to/source",
                "node_id": "node-3",
                "node_name": "处理订单"
            }
        },
        "steps": [
            {
                "node_id": "node-1",
                "node_name": "开始",
                "node_type": "skylake-start",
                "status": "Success",
                "output": null,
                "elapsed_us": 100
            },
            {
                "node_id": "node-2",
                "node_name": "验证数据",
                "node_type": "skylake-func",
                "status": "Success",
                "output": {"processed": true, "order_id": "12345"},
                "elapsed_us": 5000
            },
            {
                "node_id": "node-3",
                "node_name": "处理订单",
                "node_type": "skylake-func",
                "status": "DebugPaused",
                "output": null,
                "elapsed_us": 0,
                "previous_output": {"processed": true, "order_id": "12345"}
            }
        ],
        "total_elapsed_us": 12345,
        "debug_prepare_result": {
            "code_server_url": "https://dev.cloudmatrix.one:18080",
            "plugin_id": "order-plugin",
            "plugin_name": "订单处理插件",
            "plugin_version": "1.0.0",
            "plugin_status": "activated",
            "plugin_install_path": "/path/to/plugins/order-plugin",
            "plugin_wasm_path": "main.wasm",
            "plugin_type": "wasm",
            "domain_code": "ecommerce",
            "application_code": "order",
            "module_code": "process",
            "function_name": "process_order",
            "source_path": "/path/to/source",
            "node_id": "node-3",
            "node_name": "处理订单"
        }
    }
}
```

## 五、实施步骤

按以下顺序实施，每步完成后执行 `cargo check` 验证编译：

| 步骤 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `cmx-traits/src/plugin_query.rs` | 修改 | `PluginSnapshot` 新增 `source_path` 字段 |
| 2 | `cmx-plugin` 实现层 | 修改 | 构建 PluginSnapshot 时从 manifest.json 填充 `source_path` |
| 3 | `cmx-service/Cargo.toml` | 修改 | 添加 `cmx-debug = { workspace = true }` 依赖 |
| 4 | `cmx-service/orchestrator/types.rs` | 修改 | 新增 `DebugOptions`、`DebugPrepareResult`；修改 `ExecuteOptions`、`StepStatus`、`OrchestrationResult` |
| 5 | `cmx-service/orchestrator/debug_prepare.rs` | 新增 | 实现 `DebugPrepare` 结构体和 `prepare()` 方法 |
| 6 | `cmx-service/orchestrator/mod.rs` | 修改 | 注册 `debug_prepare` 子模块 |
| 7 | `cmx-service/orchestrator/executor.rs` | 修改 | 主循环中 skylake-func 和 skylake-switch 分支增加 debug 拦截逻辑 |
| 8 | `cmx-service/src/lib.rs` | 修改 | 导出新增类型 (`DebugOptions`, `DebugPrepareResult`) |
| 9 | `cmx-api/handlers/service/models.rs` | 修改 | 请求/响应类型增加 debug 字段；新增 `ServiceDebugPrepareResult` |
| 10 | `cmx-api/handlers/service/handler.rs` | 修改 | handler 函数传递 debug 参数；响应转换增加 debug 字段 |
| 11 | 验证 | 执行 | `cargo check -p cmx-service -p cmx-api` 确认编译通过 |
| 12 | 验证 | 执行 | `cargo clippy -p cmx-service -p cmx-api` 确认无 lint 警告 |

## 六、设计要点与注意事项

### 6.1 debug 请求时自动开启 include_steps

当 `debug=true` 时，应自动将 `include_steps` 设为 `true`，确保前端能看到已执行的步骤。这在 `execute_service_inner` 中处理：

```rust
let include_steps = include_steps || debug;
```

### 6.2 事务处理

调试暂停时如果当前处于事务框内，需要**回滚活跃事务**，避免数据库状态不一致。在 debug 拦截点返回前：

```rust
if txn_manager.has_active() {
    txn_manager.rollback_active().await;
}
```

### 6.3 debug_node_id 不存在的情况

如果传入的 `debug_node_id` 在流程中不存在（比如 ID 拼写错误），编排会正常执行到结束，忽略 debug 参数。这是合理的行为，不需要报错。

### 6.4 DebugPaused 状态的步骤不计入 previous_output

被 debug 暂停的节点本身没有执行，其 `ExecutionStep` 的 `previous_output` 记录的是上一步的输出，方便前端理解数据上下文。

### 6.5 debug 与 cmx-debug 的关系

`debug_prepare.rs` 的依赖划分：
- **`cmx_traits::PluginQuery`**（由 cmx-plugin 实现）— 获取插件详细信息（PluginSnapshot 包含 plugin_id/name/version/status/install_path/wasm_path/plugin_type/domain_code/application_code/module_code/source_path）
- **`cmx_debug::get_code_server_url_async()`** — 获取 code-server URL

不使用 `cmx_debug::start_debug_session` 系列函数，因为那些是用于创建完整的调试会话（包含 WASM 字节加载等），这里只需要获取调试准备信息。也不直接使用 `cmx_debug::plugin::find_plugin_dir_by_id()` 等工具函数，统一通过 `PluginQuery` trait 获取插件信息，保持架构解耦。

### 6.6 PluginSnapshot 新增 source_path

为支持调试功能获取源码路径，需要在 `cmx-traits` 的 `PluginSnapshot` 中新增 `source_path` 字段：

```rust
// cmx-traits/src/plugin_query.rs
pub struct PluginSnapshot {
    // ... 现有字段 ...
    /// 源码路径（从 manifest.json 读取）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}
```

cmx-plugin 的 PluginManager 在构建 PluginSnapshot 时，从 manifest.json 中读取 `plugin.source_path` 并填充该字段。这样所有通过 `PluginQuery` 获取插件信息的地方都能拿到 source_path，不需要额外调用 cmx-debug 的工具函数。
