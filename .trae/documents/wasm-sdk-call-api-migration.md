# 计划：WASM SDK 调用指定插件/服务接口改造

## 问题分析

当前 `cmx-plugin-sdk` 中的 `call_service` 函数存在以下问题：

1. **WASM SDK 侧**（`host_calls.rs`）：
   ```rust
   pub fn call_service(target_plugin_id: &str, function_name: &str, input: &str)
   ```
   - `input: &str` 无法传递复杂的 `FunctionInput` 结构体
   - 只能传递字符串，API 接口使用 `serde_json::Value` 支持任意 JSON

2. **宿主侧**（`cmx-plugin/src/host_functions.rs`）：
   ```rust
   fn do_call_service(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
       let req: ServiceCallRequest = ...;
       let input_bytes = req.input.as_bytes();  // 直接当字符串传递
       runtime.invoke(&req.target_plugin_id, &req.function_name, input_bytes).await
   }
   ```
   - 传给 `runtime.invoke` 的 `input_bytes` 是字符串字节，而非 `FunctionInput` 的 MsgPack 序列化
   - API 接口传给 runtime.invoke 的是 `FunctionInput` 序列化的 MsgPack

## 解决方案

需要新增**两个**宿主函数和一个**服务编排调用**函数：

### 1. 调用指定插件的指定函数
类似 API `/api/service/call`，传递 `plugin_id`, `function_name`, `input`

### 2. 调用指定服务（服务编排）
类似 API `/api/service/execute`，传递 `service_key`, `input`

---

## 实施步骤

### 步骤 1：修改 cmx-core 的请求结构体

**文件**: `crates/libs/cmx-core/src/wasm_types/plugin.rs`

新增两个请求结构体（带完整注释）：

```rust
/// 调用指定插件的指定函数请求
///
/// 用于 WASM 插件通过宿主函数调用另一个插件的指定函数。
/// 类似于 API `/api/service/call` 的功能，但运行在 WASM 插件上下文中。
///
/// # 字段说明
/// - `plugin_id`: 目标插件的唯一标识
/// - `function_name`: 目标插件中要调用的函数名
/// - `input`: 传递给函数的输入数据（JSON 格式，支持任意结构）
/// - `initial_input`: 初始输入数据（可选，用于调试场景）
/// - `debug`: 是否启用调试模式（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFunRequest {
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 传递给函数的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 初始输入数据（调试时传递服务最开始的入参，可选）
    pub initial_input: Option<serde_json::Value>,
    /// 是否启用调试模式（可选，默认 false）
    pub debug: Option<bool>,
}

/// 调用指定服务的请求
///
/// 用于 WASM 插件通过宿主函数执行一个完整的服务编排。
/// 类似于 API `/api/service/execute` 的功能，但运行在 WASM 插件上下文中。
///
/// # 字段说明
/// - `service_key`: 服务的唯一标识（对应服务.json 中的 code 字段）
/// - `input`: 传递给第一个函数节点的输入数据（JSON 格式）
/// - `include_steps`: 是否返回各步骤的执行详情（可选，默认 false）
/// - `debug`: 是否启用调试模式（可选，默认 false）
/// - `debug_node_id`: 调试目标节点ID（启用 debug 时必填）
/// - `debug_params`: 调试参数（可选，HashMap 形式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallServiceRequest {
    /// 服务唯一标识（对应服务.json 中的 code 字段）
    pub service_key: String,
    /// 传递给第一个函数节点的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 是否返回各步骤的执行详情（可选，默认 false）
    pub include_steps: Option<bool>,
    /// 是否启用调试模式（可选，默认 false）
    pub debug: Option<bool>,
    /// 调试目标节点ID（启用 debug 时必填）
    pub debug_node_id: Option<String>,
    /// 调试参数（可选，用于传递额外的调试配置）
    pub debug_params: Option<HashMap<String, String>>,
}
```

**文件**: `crates/libs/cmx-core/src/lib.rs`

导出新结构体（带注释）：
```rust
/// WASM 类型定义
pub use wasm_types::{
    plugin::{ServiceCallRequest, ServiceCallResponse, PluginInfoResponse, PluginFunRequest, CallServiceRequest},
    // ... 其他导出
};
```

### 步骤 2：修改 cmx-traits 的 HostFunctionProvider

**文件**: `crates/libs/cmx-traits/src/host_func.rs`

新增 `call_plugin` 和 `call_service_by_key` 函数定义到 `HostFunctionDef`（带注释）：

```rust
/// 向插件宿主函数提供者注册新的函数
///
/// # 新增函数说明
/// - `call_plugin`: 调用指定插件的指定函数，类似 /api/service/call
/// - `call_service_by_key`: 调用指定服务编排，类似 /api/service/execute
fn functions(&self) -> Vec<HostFunctionDef> {
    vec![
        // ... 原有函数
        /// 调用指定插件的指定函数（MsgPack 编码）
        HostFunctionDef::msgpack_fn("call_plugin", "cmx:plugin"),
        /// 调用指定服务编排（MsgPack 编码）
        HostFunctionDef::msgpack_fn("call_service_by_key", "cmx:plugin"),
    ]
}
```

### 步骤 3：修改 cmx-plugin 宿主函数实现

**文件**: `crates/libs/cmx-plugin/src/host_functions.rs`

新增 `call_plugin` 和 `call_service_by_key` 两个宿主函数的实现（带完整注释）：

```rust
/// 执行调用指定插件的指定函数
///
/// # 功能说明
/// 接收 WASM 插件的调用请求，通过 GlobalRuntime 加载目标插件并执行指定函数。
/// 输入输出均使用 MsgPack 编码。
///
/// # 参数
/// - `self`: 宿主函数提供者实例
/// - `input`: MsgPack 编码的 PluginFunRequest 请求
///
/// # 返回值
/// - `Ok(Vec<u8>)`: 包含 CallServiceResponse 的 MsgPack 编码
/// - `Err(HostFuncError)`: 函数执行失败
fn do_call_plugin(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
    // 1. 反序列化请求
    let req: PluginFunRequest = match rmp_serde::from_slice(&input) {
        Ok(r) => r,
        Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
    };
    info!("[call_plugin] 目标插件: {}, 函数: {}", req.plugin_id, req.function_name);

    // 2. 获取运行时实例
    let runtime = GlobalRuntime::get();

    // 3. 构建 FunctionInput（与 API handler 保持一致）
    let mut svr_ctx = SVRContext::new(
        req.initial_input.clone().unwrap_or(req.input.clone()), // 优先使用 initial_input
        HashMap::new(),
        Utc::now(),
        generate_request_id(), // 生成请求ID
    );
    let func_input = FunctionInput::from_value(req.input.clone(), svr_ctx);

    // 4. 序列化输入
    let input_bytes = rmp_serde::to_vec(&func_input)
        .map_err(|e| HostFuncError::internal(e.to_string()))?;

    // 5. 构建调用选项
    let invoke_options = InvokeOptions {
        debug: req.debug.unwrap_or(false),
        ..Default::default()
    };

    // 6. 异步调用目标插件函数
    let rt = tokio::runtime::Handle::current();
    let result: Result<WasmInvokeResult, _> = rt.block_on(async {
        runtime.invoke_with_options(&req.plugin_id, &req.function_name, &input_bytes, &invoke_options).await
    });

    // 7. 处理调用结果
    match result {
        Ok(invoke_result) => {
            // 成功：解析输出并返回
            let output = if invoke_result.output.is_empty() {
                serde_json::Value::Null
            } else {
                rmp_serde::from_slice(&invoke_result.output)
                    .unwrap_or_else(|_| serde_json::Value::Null)
            };
            Ok(Self::ok_response(Some(output), Some(invoke_result.elapsed_us)))
        }
        Err(e) => {
            // 失败：返回错误响应
            warn!("[call_plugin] 调用失败: {}", e);
            Ok(Self::err_response(e.to_string()))
        }
    }
}

/// 执行调用指定服务编排
///
/// # 功能说明
/// 接收 WASM 插件的服务调用请求，通过 GlobalRuntime 执行完整的服务编排。
/// 输入输出均使用 MsgPack 编码。
///
/// # 参数
/// - `self`: 宿主函数提供者实例
/// - `input`: MsgPack 编码的 CallServiceRequest 请求
///
/// # 返回值
/// - `Ok(Vec<u8>)`: 包含 CallServiceResponse 的 MsgPack 编码
/// - `Err(HostFuncError)`: 函数执行失败
fn do_call_service_by_key(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
    // 1. 反序列化请求
    let req: CallServiceRequest = match rmp_serde::from_slice(&input) {
        Ok(r) => r,
        Err(e) => return Ok(Self::err_response(format!("解析请求失败: {}", e))),
    };
    info!("[call_service_by_key] 服务: {}", req.service_key);

    // 2. 获取运行时实例
    let runtime = GlobalRuntime::get();

    // 3. 获取服务编排执行器（通过 GlobalRuntime 获取）
    // 注意：这里通过 runtime 获取 orchestrator，与 API handler 保持一致
    let orchestrator = runtime.get_orchestrator()
        .ok_or_else(|| HostFuncError::internal("无法获取服务编排器"))?;

    // 4. 构建服务上下文
    let svr_ctx = SVRContext::new(
        req.input.clone(),
        HashMap::new(),
        Utc::now(),
        generate_request_id(),
    );

    // 5. 构建执行选项
    let options = ExecuteOptions::new(req.include_steps.unwrap_or(false))
        .with_debug(
            req.debug.unwrap_or(false),
            req.debug_node_id.clone(),
            req.debug_params.clone(),
        );

    // 6. 异步执行服务编排
    let rt = tokio::runtime::Handle::current();
    let result = rt.block_on(async {
        orchestrator.execute_service(&req.service_key, svr_ctx, options).await
    });

    // 7. 处理执行结果
    match result {
        Ok(exec_result) => {
            if exec_result.success {
                Ok(Self::ok_response(exec_result.output, Some(exec_result.total_elapsed_us)))
            } else {
                let error_msg = exec_result.error.map(|e| e.message).unwrap_or_default();
                Ok(Self::err_response(error_msg))
            }
        }
        Err(e) => {
            warn!("[call_service_by_key] 执行失败: {}", e);
            Ok(Self::err_response(e.to_string()))
        }
    }
}

/// 构建成功响应（MsgPack 编码）
///
/// # 参数说明
/// - `output`: 函数执行结果（JSON 格式）
/// - `elapsed_us`: 执行耗时（微秒）
///
/// # 返回值
/// 包含 CallServiceResponse 的 MsgPack 编码字节
fn ok_response(output: Option<serde_json::Value>, elapsed_us: Option<u64>) -> Vec<u8> {
    rmp_serde::to_vec(&CallServiceResponse {
        success: true,
        output,
        elapsed_us,
        error: None,
    })
    .unwrap_or_default()
}

/// 构建错误响应（MsgPack 编码）
///
/// # 参数说明
/// - `msg`: 错误信息
///
/// # 返回值
/// 包含 CallServiceResponse 的 MsgPack 编码字节
fn err_response(msg: String) -> Vec<u8> {
    rmp_serde::to_vec(&CallServiceResponse {
        success: false,
        output: None,
        elapsed_us: None,
        error: Some(msg),
    })
    .unwrap_or_default()
}
```

更新 `HostFunctionProvider` 实现中的函数列表和 `call` 方法：
```rust
impl HostFunctionProvider for PluginHostFunctions {
    fn namespace(&self) -> &str {
        "cmx:plugin"
    }

    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            /// 调用指定插件的指定函数
            HostFunctionDef::msgpack_fn("call_plugin", "cmx:plugin"),
            /// 调用指定服务编排
            HostFunctionDef::msgpack_fn("call_service_by_key", "cmx:plugin"),
        ]
    }

    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        match name {
            /// 处理 call_plugin 请求
            "call_plugin" => self.do_call_plugin(input),
            /// 处理 call_service_by_key 请求
            "call_service_by_key" => self.do_call_service_by_key(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    fn provided_functions(&self) -> Vec<&str> {
        vec!["call_plugin", "call_service_by_key"]
    }
}
```

### 步骤 4：修改 cmx-plugin-sdk 的 host_calls.rs

**文件**: `crates/libs/cmx-plugin-sdk/src/host_calls.rs`

#### 4.1 新增结构体定义（带完整注释）：

```rust
/// 调用指定插件的指定函数请求
///
/// 传递给宿主函数 `call_plugin`，用于在 WASM 插件中调用另一个插件的函数。
///
/// # 字段说明
/// - `plugin_id`: 目标插件的唯一标识
/// - `function_name`: 目标插件中要调用的函数名
/// - `input`: 传递给函数的输入数据（JSON 格式）
/// - `initial_input`: 初始输入数据（可选，用于调试场景）
/// - `debug`: 是否启用调试模式（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFunRequest {
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 传递给函数的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 初始输入数据（可选，用于调试场景）
    pub initial_input: Option<serde_json::Value>,
    /// 是否启用调试模式（可选）
    pub debug: Option<bool>,
}

/// 调用指定服务的请求
///
/// 传递给宿主函数 `call_service_by_key`，用于在 WASM 插件中执行一个完整的服务编排。
///
/// # 字段说明
/// - `service_key`: 服务的唯一标识
/// - `input`: 传递给第一个函数节点的输入数据（JSON 格式）
/// - `include_steps`: 是否返回各步骤的执行详情（可选）
/// - `debug`: 是否启用调试模式（可选）
/// - `debug_node_id`: 调试目标节点ID（可选）
/// - `debug_params`: 调试参数（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallServiceRequest {
    /// 服务唯一标识
    pub service_key: String,
    /// 传递给第一个函数节点的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 是否返回各步骤的执行详情（可选）
    pub include_steps: Option<bool>,
    /// 是否启用调试模式（可选）
    pub debug: Option<bool>,
    /// 调试目标节点ID（可选）
    pub debug_node_id: Option<String>,
    /// 调试参数（可选）
    pub debug_params: Option<HashMap<String, String>>,
}

/// 服务调用响应
///
/// 宿主函数返回给 WASM 插件的调用结果。
///
/// # 字段说明
/// - `success`: 是否执行成功
/// - `output`: 执行结果（JSON 格式，失败时为 None）
/// - `error`: 错误信息（成功时为 None）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallServiceResponse {
    /// 是否执行成功
    pub success: bool,
    /// 执行结果（JSON 格式）
    pub output: Option<serde_json::Value>,
    /// 错误信息（成功时为 None）
    pub error: Option<String>,
}
```

#### 4.2 更新 extern 声明块（新增函数，带注释）：

```rust
// 声明插件间调用宿主函数（MsgPack 编码）
#[host_fn("cmx:plugin")]
extern "ExtismHost" {
    /// 调用指定插件的指定函数
    ///
    /// # 参数
    /// - `request`: PluginFunRequest 的 MsgPack 编码
    ///
    /// # 返回值
    /// CallServiceResponse 的 MsgPack 编码
    fn call_plugin(request: Vec<u8>) -> Vec<u8>;

    /// 调用指定服务编排
    ///
    /// # 参数
    /// - `request`: CallServiceRequest 的 MsgPack 编码
    ///
    /// # 返回值
    /// CallServiceResponse 的 MsgPack 编码
    fn call_service_by_key(request: Vec<u8>) -> Vec<u8>;
}
```

#### 4.3 新增 HostCaller 方法（带完整注释）：

```rust
/// 调用指定插件的指定函数
///
/// 类似于 API `/api/service/call`，在 WASM 插件上下文中调用另一个插件的函数。
///
/// # 参数说明
/// - `request`: PluginFunRequest 请求结构体，包含目标插件ID、函数名和输入数据
///
/// # 返回值说明
/// - `Ok(serde_json::Value)`: 函数执行结果
/// - `Err(Error)`: 调用失败，包含错误信息
///
/// # 示例
/// ```rust,ignore
/// let request = PluginFunRequest {
///     plugin_id: "my-plugin".to_string(),
///     function_name: "handle_data".to_string(),
///     input: serde_json::json!({"key": "value"}),
///     initial_input: None,
///     debug: Some(false),
/// };
/// let result = HostCaller::call_plugin(request)?;
/// ```
pub fn call_plugin(request: PluginFunRequest) -> Result<serde_json::Value, Error> {
    // 1. 序列化请求为 MsgPack 格式
    let bytes = rmp_serde::to_vec(&request)?;

    // 2. 调用宿主函数获取原始响应
    let result = unsafe { call_plugin(bytes)? };

    // 3. 反序列化响应
    let response: CallServiceResponse = rmp_serde::from_slice(&result)?;

    // 4. 检查是否成功，失败则返回错误
    if !response.success {
        return Err(Error::from(response.error.unwrap_or_default()));
    }

    // 5. 返回执行结果
    Ok(response.output.unwrap_or(serde_json::Value::Null))
}

/// 调用指定服务编排
///
/// 类似于 API `/api/service/execute`，在 WASM 插件上下文中执行一个完整的服务编排。
///
/// # 参数说明
/// - `request`: CallServiceRequest 请求结构体，包含服务标识、输入数据和执行选项
///
/// # 返回值说明
/// - `Ok(serde_json::Value)`: 服务执行的最终输出
/// - `Err(Error)`: 执行失败，包含错误信息
///
/// # 示例
/// ```rust,ignore
/// let request = CallServiceRequest {
///     service_key: "my-service".to_string(),
///     input: serde_json::json!({"data": "test"}),
///     include_steps: Some(false),
///     debug: Some(false),
///     debug_node_id: None,
///     debug_params: None,
/// };
/// let result = HostCaller::call_service_by_key(request)?;
/// ```
pub fn call_service_by_key(request: CallServiceRequest) -> Result<serde_json::Value, Error> {
    // 1. 序列化请求为 MsgPack 格式
    let bytes = rmp_serde::to_vec(&request)?;

    // 2. 调用宿主函数获取原始响应
    let result = unsafe { call_service_by_key(bytes)? };

    // 3. 反序列化响应
    let response: CallServiceResponse = rmp_serde::from_slice(&result)?;

    // 4. 检查是否成功，失败则返回错误
    if !response.success {
        return Err(Error::from(response.error.unwrap_or_default()));
    }

    // 5. 返回执行结果
    Ok(response.output.unwrap_or(serde_json::Value::Null))
}
```

### 步骤 5：更新 wasmdemo 中的调用示例

**文件**: `crates/libs/cmx-wasmdemo/src/lib.rs`

根据新的函数签名更新调用代码（添加注释）：

```rust
/// 调用指定插件的指定函数示例
///
/// 展示如何使用 HostCaller::call_plugin 调用其他插件的函数
fn example_call_plugin() {
    let request = PluginFunRequest {
        plugin_id: "target-plugin".to_string(),     // 目标插件ID
        function_name: "process".to_string(),        // 目标函数名
        input: serde_json::json!({"data": "value"}), // 函数输入
        initial_input: None,                         // 初始输入（可选）
        debug: Some(false),                          // 调试模式（可选）
    };

    match HostCaller::call_plugin(request) {
        Ok(result) => tracing::info!("调用成功: {:?}", result),
        Err(e) => tracing::error!("调用失败: {}", e),
    }
}

/// 调用指定服务编排示例
///
/// 展示如何使用 HostCaller::call_service_by_key 执行服务编排
fn example_call_service_by_key() {
    let request = CallServiceRequest {
        service_key: "my-domain/my-service".to_string(), // 服务标识
        input: serde_json::json!({"input": "data"}),       // 服务输入
        include_steps: Some(false),                        // 返回步骤详情（可选）
        debug: Some(false),                               // 调试模式（可选）
        debug_node_id: None,                              // 调试节点ID（可选）
        debug_params: None,                                // 调试参数（可选）
    };

    match HostCaller::call_service_by_key(request) {
        Ok(result) => tracing::info!("服务执行成功: {:?}", result),
        Err(e) => tracing::error!("服务执行失败: {}", e),
    }
}
```

### 步骤 6：编译检查

运行 `cargo check` 确认修改正确

---

## 涉及文件清单

| 文件 | 修改内容 |
|------|---------|
| `cmx-core/src/wasm_types/plugin.rs` | 新增 `PluginFunRequest`, `CallServiceRequest` 结构体（含注释） |
| `cmx-core/src/lib.rs` | 导出新结构体 |
| `cmx-traits/src/host_func.rs` | 新增函数定义（含注释） |
| `cmx-plugin/src/host_functions.rs` | 实现 `do_call_plugin`, `do_call_service_by_key` 宿主函数（含注释） |
| `cmx-plugin-sdk/src/host_calls.rs` | 新增结构体、extern 声明和 `HostCaller` 方法（含注释） |
| `cmx-wasmdemo/src/lib.rs` | 更新调用示例（含注释） |

---

## 依赖关系

```
cmx-core (定义结构体)
    ↓
cmx-traits (定义函数签名)
    ↓
cmx-plugin (宿主侧实现)
    ↑
    └── cmx-plugin-sdk (WASM 侧调用，通过 extern "ExtismHost" 链接)
```

注意：`cmx-plugin-sdk` 不能依赖 `cmx-service`（避免循环依赖或 WASM 体积过大），所以 `call_service_by_key` 的宿主实现在 `cmx-plugin` 中通过 `GlobalRuntime::get_orchestrator()` 获取编排器来执行。