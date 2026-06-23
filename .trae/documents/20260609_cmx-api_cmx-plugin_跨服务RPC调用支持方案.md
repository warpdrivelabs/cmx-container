# 跨服务 RPC 调用支持方案

## 一、概述

为三个 HTTP API 接口和 WASM 插件 SDK 增加 `server_name` 参数支持，实现跨服务的 RPC 调用能力。当请求中指定了 `server_name` 时，系统自动通过 gRPC 将请求路由到目标远程服务执行；未指定时保持现有的本地执行逻辑不变。

### 涉及接口

| 接口 | 路由 | 功能 |
|------|------|------|
| `service_call` | `POST /api/service/call` | 直接调用插件函数 |
| `execute_service` | `POST /api/service/execute` | 执行服务编排 |
| `execute_service_by_key` | `POST /api/service/execute/{service-key}` | 执行服务编排（路径参数版） |

## 二、现状分析

### 2.1 调用链路

```
HTTP API 层:  handler.rs → RuntimeInvoker / Orchestrator → WASM 函数
WASM 插件层:  HostCaller → extism host_fn → PluginHostFunctions → GlobalRuntime / GlobalServiceInvoker
RPC 层:       VoloGrpcClient → volo-grpc → CmxOrchestratorServiceImpl → ServiceInvoker / RuntimeInvoker
```

### 2.2 现有 RPC 基础设施

- `RpcClient` trait（`cmx-traits`）：定义 `call_service` 和 `call_function` 两个方法
- `VoloGrpcClient`（`cmx-rpc`）：基于 volo-grpc 的 RPC 客户端实现，支持指数退避重试
- `GlobalRpcClient`（`cmx-rpc`）：全局单例，应用启动时在 `init_rpc()` 中初始化
- gRPC Proto 已定义 `ExecuteService` 和 `CallFunction` 两个 RPC 方法

### 2.3 依赖关系

- `cmx-api` 当前**不依赖** `cmx-rpc`（需新增）
- `cmx-plugin` 当前**不依赖** `cmx-rpc`（需新增）
- `cmx-plugin-sdk` 是纯 WASM 侧 SDK，通过 extism-pdk 声明宿主函数，**不需要修改 extern 声明**（`Vec<u8>` 参数透传）

## 三、设计方案

### 3.1 核心设计思路

**极简改动原则**：SDK 侧的 extism `#[host_fn]` extern 声明（`call_plugin(request: Vec<u8>)` 和 `call_service_by_key(request: Vec<u8>)`）使用 MsgPack 编码的 `Vec<u8>` 透传。只需在 `PluginFunRequest` 和 `CallServiceRequest` 两个请求结构体中增加 `server_name: Option<String>` 字段，序列化/反序列化自动处理，SDK 的 extern 声明和 `HostCaller` 现有方法签名**完全不需要改动**。

路由决策全部在**宿主函数实现侧**（`cmx-plugin/host_functions.rs`）完成：反序列化请求后检查 `server_name`，有值走 RPC，无值走本地。

### 3.2 数据流

**本地调用（无 `server_name`）**：保持不变
```
HTTP Request → handler → 本地 Orchestrator/RuntimeInvoker → WASM 函数
```

**HTTP API 远程调用（有 `server_name`）**：
```
HTTP Request → handler → GlobalRpcClient.call_service/call_function
  → volo-grpc → 远程 gRPC Server → 远端 Orchestrator/RuntimeInvoker → WASM 函数
```

**WASM 插件远程调用（有 `server_name`）**：
```
WASM 插件代码:
  let mut req = PluginFunRequest { ..., server_name: Some("remote-svc".into()), .. };
  HostCaller::call_plugin(req)  // 现有方法，无需改动
    → extism host_fn (Vec<u8> 透传)
    → PluginHostFunctions 反序列化，检测到 server_name
    → GlobalRpcClient.call_function()
    → 远程 gRPC Server
```

### 3.3 SDK 便捷方法（可选增强）

在 `HostCaller` 上增加 `call_remote_plugin(server_name, request)` 和 `call_remote_service(server_name, request)` 便捷方法，自动填充 `server_name` 字段后调用现有方法。这是对现有 API 的轻量封装，提供更明确的语义。

## 四、具体修改清单

### 4.1 cmx-core — 请求类型增加 `server_name` 字段

**文件**: `crates/libs/cmx-core/src/wasm_types/plugin.rs`

- `PluginFunRequest` 增加 `server_name: Option<String>` 字段
- `CallServiceRequest` 增加 `server_name: Option<String>` 字段

```rust
// PluginFunRequest 新增字段（在现有字段之后）
/// 目标服务名称（跨服务调用时指定，不指定则本地调用）
#[serde(skip_serializing_if = "Option::is_none")]
pub server_name: Option<String>,

// CallServiceRequest 新增字段（在现有字段之后）
/// 目标服务名称（跨服务调用时指定，不指定则本地调用）
#[serde(skip_serializing_if = "Option::is_none")]
pub server_name: Option<String>,
```

### 4.2 cmx-api — HTTP API 层支持 RPC 路由

#### 4.2.1 新增依赖

**文件**: `crates/libs/cmx-api/Cargo.toml`

```toml
# 内部依赖 - RPC 通信
cmx-rpc = { workspace = true }
```

#### 4.2.2 请求模型增加 `server_name`

**文件**: `crates/libs/cmx-api/src/handlers/service/models.rs`

- `FunctionCallRequest` 增加 `server_name: Option<String>`
- `ServiceExecuteRequest` 增加 `server_name: Option<String>`

```rust
// FunctionCallRequest 新增
/// 目标服务名称（跨服务调用时指定，不指定则本地调用）
#[serde(skip_serializing_if = "Option::is_none")]
pub server_name: Option<String>,

// ServiceExecuteRequest 新增
/// 目标服务名称（跨服务调用时指定，不指定则本地调用）
#[serde(skip_serializing_if = "Option::is_none")]
pub server_name: Option<String>,
```

#### 4.2.3 Handler 实现 RPC 路由

**文件**: `crates/libs/cmx-api/src/handlers/service/handler.rs`

**`service_call` 函数**（第 169 行）：

在函数开头（获取依赖组件之前）增加 RPC 路由判断：

```rust
pub async fn service_call(
    State(state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<FunctionCallRequest>,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return call_function_via_rpc(server_name, &req).await;
    }

    // ... 原有本地执行逻辑不变
}
```

**`execute_service` 函数**（第 434 行）：

在 `execute_service_inner` 调用前增加 RPC 路由判断：

```rust
pub async fn execute_service(...) -> Result<...> {
    // ... 现有参数处理（include_steps, debug 等）

    if req.service_key.clone().is_none() {
        return Ok(Json(ApiResp::fail(1, "service_key 不能为空")));
    }
    let service_key = req.service_key.clone().unwrap();

    // 跨服务 RPC 调用（在本地执行之前判断）
    if let Some(ref server_name) = req.server_name {
        return execute_service_via_rpc(server_name, &service_key, &req).await;
    }

    // ... 原有 execute_service_inner 本地执行逻辑不变
}
```

**`execute_service_by_key` 函数**（第 512 行）：

同理，在本地执行前增加 RPC 路由：

```rust
pub async fn execute_service_by_key(...) -> Result<...> {
    // ... 现有参数处理

    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return execute_service_via_rpc(server_name, &service_key, &req).await;
    }

    // ... 原有 execute_service_inner 本地执行逻辑不变
}
```

**新增辅助函数**（在 handler.rs 文件底部）：

```rust
/// 通过 RPC 调用远程插件函数
async fn call_function_via_rpc(
    server_name: &str,
    req: &FunctionCallRequest,
) -> Result<Json<ApiResp<FunctionCallResponse>>, Error> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Err(Error::business_error("RPC 服务未启用，无法进行跨服务调用"));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let result = rpc_client
        .call_function(server_name, &req.plugin_id, &req.function_name, req.input.clone())
        .await
        .map_err(|e| Error::business_error(format!("RPC 调用失败: {}", e)))?;

    Ok(Json(ApiResp::ok(FunctionCallResponse {
        success: result.success,
        result: result.result,
        elapsed_us: result.elapsed_us,
        error: result.error,
    })))
}

/// 通过 RPC 调用远程服务编排
async fn execute_service_via_rpc(
    server_name: &str,
    service_key: &str,
    req: &ServiceExecuteRequest,
) -> Result<Json<ApiResp<ServiceExecuteResponse>>, Error> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Err(Error::business_error("RPC 服务未启用，无法进行跨服务调用"));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let options = cmx_traits::ServiceInvokeOptions {
        include_steps: req.include_steps.unwrap_or(false),
        debug: req.debug.unwrap_or(false),
        debug_node_id: req.debug_node_id.clone(),
        debug_params: req.debug_params.clone(),
    };
    let result = rpc_client
        .call_service(server_name, service_key, req.input.clone(), options)
        .await
        .map_err(|e| Error::business_error(format!("RPC 调用失败: {}", e)))?;

    // 将 CallServiceResponse 转换为 ServiceExecuteResponse
    let response = ServiceExecuteResponse {
        success: result.success,
        output: result.output,
        steps: result.steps.into_iter().map(|s| ServiceExecutionStep {
            node_id: s.node_id,
            node_name: s.node_name,
            node_type: s.node_type,
            status: match s.status {
                cmx_service::StepStatus::Success => "Success".to_string(),
                cmx_service::StepStatus::Failed => "Failed".to_string(),
                cmx_service::StepStatus::Skipped => "Skipped".to_string(),
                cmx_service::StepStatus::DebugPaused => "DebugPaused".to_string(),
            },
            output: s.output,
            elapsed_us: s.elapsed_us,
            error: s.error,
            previous_output: s.previous_output,
        }).collect(),
        total_elapsed_us: result.total_elapsed_us.unwrap_or(0),
        error: result.error.map(|e| ServiceOrchestrationError { message: e.message }),
        debug_triggered: None,
        debug_prepare_result: None,
    };
    Ok(Json(ApiResp::ok(response)))
}
```

### 4.3 cmx-plugin — 宿主函数支持 RPC 路由

#### 4.3.1 新增依赖

**文件**: `crates/libs/cmx-plugin/Cargo.toml`

```toml
# 内部依赖 - RPC 通信
cmx-rpc = { workspace = true }
```

#### 4.3.2 宿主函数实现 RPC 路由

**文件**: `crates/libs/cmx-plugin/src/host_functions.rs`

**`do_call_plugin` 方法**：在反序列化请求后，检查 `server_name` 字段：

```rust
fn do_call_plugin(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
    let req: PluginFunRequest = match rmp_serde::from_slice(&input) {
        Ok(r) => r,
        Err(e) => return Ok(Self::err_plugin_response_msgpack(format!("解析请求失败: {}", e))),
    };
    info!("[call_plugin] 目标插件: {}, 函数: {}", req.plugin_id, req.function_name);

    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return self.do_call_plugin_via_rpc(server_name, &req);
    }

    // ... 原有本地执行逻辑不变
}
```

**`do_call_service_by_key` 方法**：同样增加 RPC 路由：

```rust
fn do_call_service_by_key(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
    let req: CallServiceRequest = match rmp_serde::from_slice(&input) {
        Ok(r) => r,
        Err(e) => return Ok(Self::err_service_response_msgpack(format!("解析请求失败: {}", e))),
    };
    info!("[call_service_by_key] 服务: {}", req.service_key);

    // 跨服务 RPC 调用
    if let Some(ref server_name) = req.server_name {
        return self.do_call_service_via_rpc(server_name, &req);
    }

    // ... 原有本地执行逻辑不变
}
```

**新增 RPC 调用辅助方法**（在 `PluginHostFunctions` impl 中）：

```rust
/// 通过 RPC 调用远程插件函数
fn do_call_plugin_via_rpc(&self, server_name: &str, req: &PluginFunRequest) -> Result<Vec<u8>, HostFuncError> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Ok(Self::err_plugin_response_msgpack("RPC 服务未启用，无法进行跨服务调用".to_string()));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        rpc_client.call_function(server_name, &req.plugin_id, &req.function_name, req.input.clone()).await
    });

    match result {
        Ok(call_result) => {
            Ok(rmp_serde::to_vec(&PluginFunCallResponse {
                success: call_result.success,
                result: call_result.result,
                elapsed_us: Some(call_result.elapsed_us),
                error: call_result.error,
            }).unwrap_or_default())
        }
        Err(e) => {
            warn!("[call_plugin:rpc] RPC 调用失败: {}", e);
            Ok(Self::err_plugin_response_msgpack(format!("RPC 调用失败: {}", e)))
        }
    }
}

/// 通过 RPC 调用远程服务编排
fn do_call_service_via_rpc(&self, server_name: &str, req: &CallServiceRequest) -> Result<Vec<u8>, HostFuncError> {
    if !cmx_rpc::GlobalRpcClient::is_initialized() {
        return Ok(Self::err_service_response_msgpack("RPC 服务未启用，无法进行跨服务调用".to_string()));
    }
    let rpc_client = cmx_rpc::GlobalRpcClient::get();
    let options = ServiceInvokeOptions {
        include_steps: req.include_steps.unwrap_or(false),
        debug: req.debug.unwrap_or(false),
        debug_node_id: req.debug_node_id.clone(),
        debug_params: req.debug_params.clone(),
    };
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        rpc_client.call_service(server_name, &req.service_key, req.input.clone(), options).await
    });

    match result {
        Ok(response) => Ok(rmp_serde::to_vec(&response).unwrap_or_default()),
        Err(e) => {
            warn!("[call_service_by_key:rpc] RPC 调用失败: {}", e);
            Ok(Self::err_service_response_msgpack(format!("RPC 调用失败: {}", e)))
        }
    }
}
```

### 4.4 cmx-plugin-sdk — WASM 插件 SDK 便捷方法（可选增强）

**文件**: `crates/libs/cmx-plugin-sdk/src/host_calls.rs`

SDK 侧的 extern 声明和 `call_plugin` / `call_service_by_key` 方法**不需要任何改动**。插件开发者可以直接在 `PluginFunRequest` / `CallServiceRequest` 中设置 `server_name` 字段。

为提供更好的开发者体验，**可选**增加便捷方法：

```rust
impl HostCaller {
    // ... 现有方法不变

    /// 调用远程服务的插件函数
    ///
    /// 通过 RPC 方式调用指定远程服务上的插件函数。
    /// 本质上是 `call_plugin` 的便捷封装，自动设置 `server_name`。
    ///
    /// # 参数
    /// - `server_name`: 目标服务名称（注册中心中的服务标识）
    /// - `request`: 插件函数调用请求（会自动覆盖 server_name）
    pub fn call_remote_plugin(server_name: &str, mut request: PluginFunRequest) -> Result<PluginFunCallResponse, PluginError> {
        request.server_name = Some(server_name.to_string());
        Self::call_plugin(request)
    }

    /// 调用远程服务编排
    ///
    /// 通过 RPC 方式执行指定远程服务上的服务编排。
    /// 本质上是 `call_service_by_key` 的便捷封装，自动设置 `server_name`。
    ///
    /// # 参数
    /// - `server_name`: 目标服务名称（注册中心中的服务标识）
    /// - `request`: 服务调用请求（会自动覆盖 server_name）
    pub fn call_remote_service(server_name: &str, mut request: CallServiceRequest) -> Result<CallServiceResponse, PluginError> {
        request.server_name = Some(server_name.to_string());
        Self::call_service_by_key(request)
    }
}
```

**插件开发者使用方式**：

```rust
// 方式一：直接设置 server_name（推荐，语义清晰）
let request = PluginFunRequest {
    plugin_id: "target-plugin".into(),
    function_name: "process".into(),
    input: json!({"key": "value"}),
    server_name: Some("remote-service".into()),  // 指定远程服务
    ..Default::default()
};
HostCaller::call_plugin(request)?;

// 方式二：使用便捷方法
let request = PluginFunRequest {
    plugin_id: "target-plugin".into(),
    function_name: "process".into(),
    input: json!({"key": "value"}),
    ..Default::default()
};
HostCaller::call_remote_plugin("remote-service", request)?;
```

## 五、修改文件清单

| 序号 | 文件路径 | 修改内容 |
|------|----------|----------|
| 1 | `crates/libs/cmx-core/src/wasm_types/plugin.rs` | `PluginFunRequest`、`CallServiceRequest` 增加 `server_name` 字段 |
| 2 | `crates/libs/cmx-api/Cargo.toml` | 增加 `cmx-rpc` 依赖 |
| 3 | `crates/libs/cmx-api/src/handlers/service/models.rs` | `FunctionCallRequest`、`ServiceExecuteRequest` 增加 `server_name` 字段 |
| 4 | `crates/libs/cmx-api/src/handlers/service/handler.rs` | 三个 handler 增加 RPC 路由逻辑 + 新增两个 RPC 辅助函数 |
| 5 | `crates/libs/cmx-plugin/Cargo.toml` | 增加 `cmx-rpc` 依赖 |
| 6 | `crates/libs/cmx-plugin/src/host_functions.rs` | 两个宿主函数增加 RPC 路由 + 新增两个 RPC 辅助方法 |
| 7 | `crates/libs/cmx-plugin-sdk/src/host_calls.rs` | 可选：增加 `call_remote_plugin`、`call_remote_service` 便捷方法 |

## 六、注意事项

1. **GlobalRpcClient 未初始化防护**：RPC 路由前必须检查 `GlobalRpcClient::is_initialized()`，未初始化时返回友好错误而非 panic。
2. **向后兼容**：所有新增字段均为 `Option<String>` + `#[serde(skip_serializing_if = "Option::is_none")]`，不影响现有 API 和 SDK 调用。
3. **错误处理统一**：RPC 调用失败时，统一使用对应响应类型的 `success: false` + 错误消息格式。
4. **调试模式**：远程 RPC 调用不支持本地调试模式（debug 参数在远程服务上无效），RPC 请求中 debug 参数正常传递但由远端决定是否生效。

## 七、验证步骤

1. 编译检查：`rtk cargo check` 确保无编译错误
2. 不传 `server_name` 时，三个 HTTP API 接口行为与修改前完全一致
3. 传 `server_name` 时，请求通过 RPC 路由到远程服务
4. WASM 插件设置 `server_name` 后通过现有 `HostCaller::call_plugin` / `HostCaller::call_service_by_key` 能正确发起 RPC 调用
5. RPC 未启用时，传 `server_name` 返回友好错误而非 panic
