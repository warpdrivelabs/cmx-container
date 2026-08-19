# cmx-plugin-sdk

> 基于 Extism PDK 的插件开发 SDK，提供宿主函数调用封装、插件函数导出宏、错误类型定义和标准入参出参类型。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

本 SDK 用于开发 CMX 平台的 WASM 插件，所有服务编排中的函数都应使用统一的入参和出参格式。类型本体（`FunctionInput` / `FunctionOutput` / `SVRContext` / 各类请求响应结构）定义在 `cmx-core` 并由本 crate re-export，插件只需依赖本 SDK。

## 快速开始

### 安装

```toml
[dependencies]
cmx-plugin-sdk = { version = "0.1.12", registry = "nora", default-features = false }

[features]
default = []
# 启用 Extism PDK 集成（宿主函数调用 + 插件导出宏）
extism = ["extism-pdk", "cmx-plugin-sdk/extism"]
```

> `extism` 为 SDK 默认 feature（引入 extism-pdk 与 rmp-serde）；关闭后仅提供类型定义，适用于纯业务逻辑的单测与调试。workspace 内插件（如 cmx-plugin-demo）通常以 `default-features = false` + 自身 `extism` feature 的方式引入。

### 核心示例

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
use extism_pdk::*;

#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let current_input = input.as_str();
    let json_value = input.as_json_value();
    let initial_input = &input.context.initial_input;
    let headers = &input.context.headers;

    if let Some(prev_output) = input.context.get_step_output("previous_node_id") {
        // 使用前序步骤输出
    }

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
        "status": "success",
        "data": "处理结果"
    }))))
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 标准入参出参格式 | 所有函数使用统一的 `FunctionInput` 和 `FunctionOutput` |
| 上下文管理 | `SVRContext` 包含初始输入、请求头、步骤输出、事务 ID、认证上下文等完整上下文信息 |
| 宿主函数调用封装 | `HostCaller` 结构体封装日志 / 数据库 / 缓存 / 插件间调用 / 服务编排 / IAM 查询 |
| 错误处理 | 自定义 `PluginError` 错误类型 |
| 二进制数据支持 | `binary_data: HashMap<String, Vec<u8>>` 支持在函数间传递二进制数据 |

## 模块结构

```
cmx-plugin-sdk
├── src/
│   ├── lib.rs             # 主模块入口（re-export extism_pdk、HostCaller、cmx-core 类型）
│   ├── host_calls.rs      # 宿主函数调用封装（需启用 extism feature）
│   └── error.rs           # 自定义错误类型
└── Cargo.toml             # crate-type = ["cdylib", "rlib"]；features: extism（默认）
```

## 主要类型说明

### `FunctionInput`

所有服务编排中的函数都应该使用此结构体作为入参。

- `input`: 当前步骤输入数据（`serde_json::Value`）
- `context`: 服务调用上下文（`SVRContext`）
- `binary_data`: 二进制数据（`HashMap<String, Vec<u8>>`）

常用方法：

| 方法 | 说明 |
|------|------|
| `as_str()` | 输入为 JSON 字符串时返回 `&str`，否则返回空串 |
| `as_json_value()` | 返回 `&serde_json::Value`（即 `input` 字段引用） |
| `from_value(value, context)` | 从 `Value` 构造 |
| `from_input<T: Serialize>(input, context)` | 从任意可序列化类型构造 |

### `FunctionOutput`

所有服务编排中的函数都应该使用此结构体作为出参。

- `result`: 函数执行结果（`serde_json::Value`）
- `binary_data`: 二进制数据（`HashMap<String, Vec<u8>>`）

常用方法：

| 方法 | 说明 |
|------|------|
| `new(result)` / `from_json(value)` / `from_value(value)` | 从 `Value` 创建输出 |
| `from_result<T: Serialize>(result)` | 从任意可序列化类型创建输出 |
| `with_binary(key, data)` | 链式添加二进制数据 |

### `SVRContext`

包含服务调用的完整上下文信息（与 `cmx_core::SVRContext` 同一类型）。

- `initial_input`: 初始调用入参
- `headers`: HTTP 请求头信息
- `step_outputs`: 各步骤执行结果的缓存
- `txn_id`: 事务 ID（仅在事务框内执行时设置）
- `time_in` / `request_id`: 请求进入时间与请求 ID
- `auth_context`: 认证上下文（由 mw_auth 中间件或 gRPC interceptor 注入，含 user_id/username/roles/permissions）

## 使用指南

### 一、函数签名规范

#### 1.1 标准函数签名

所有服务编排函数必须使用以下签名格式：

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput};
use extism_pdk::*;

// MessagePack 格式输入输出（推荐，数据类宿主函数同用 MsgPack 编码）
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 函数逻辑
    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"ok": true}))))
}

// JSON 格式输入输出
#[plugin_fn]
pub fn my_function_json(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 函数逻辑
    Ok(Json(FunctionOutput::from_json(serde_json::json!({"ok": true}))))
}
```

#### 1.2 函数命名规范

- 插件导出函数名：`lower_snake_case`
- 内部函数：`pub fn` 标记
- 入口函数：使用 `#[plugin_fn]` 宏

### 二、输入处理

#### 2.1 获取输入数据

```rust
#[plugin_fn]
pub fn process_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 方式一：获取纯文本输入（input 为 JSON 字符串时返回其内容，否则空串）
    let text_input = input.as_str();

    // 方式二：获取 JSON Value 引用
    let json_value = input.as_json_value();

    // 方式三：解析为强类型结构体（serde_json::from_value）
    #[derive(serde::Deserialize)]
    struct MyInput {
        name: String,
        age: u32,
    }

    let parsed: MyInput = match serde_json::from_value(input.as_json_value().clone()) {
        Ok(parsed) => parsed,
        Err(e) => {
            return Err(Error::msg(format!("Failed to parse input: {}", e)));
        }
    };

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"name": parsed.name}))))
}
```

#### 2.2 访问上下文数据

```rust
#[plugin_fn]
pub fn process_with_context(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let ctx = &input.context;

    // 获取初始入参
    let initial = &ctx.initial_input;

    // 获取 HTTP 请求头
    let headers = &ctx.headers;
    if let Some(auth) = headers.get("Authorization") {
        HostCaller::log_debug(&format!("Authorization: {}", auth))?;
    }

    // 获取前序步骤的输出
    // 在服务编排中，每个步骤可以通过 context 获取前序步骤的结果
    if let Some(prev_result) = ctx.get_step_output("previous_node_id") {
        HostCaller::log_debug(&format!("Previous step result: {:?}", prev_result))?;
    }

    // 获取事务 ID（如果有）
    if let Some(txn_id) = &ctx.txn_id {
        HostCaller::log_debug(&format!("Transaction ID: {}", txn_id))?;
    }

    // 获取当前调用者身份（由宿主中间件注入）
    if let Some(auth) = &ctx.auth_context {
        HostCaller::log_debug(&format!("Caller: {}", auth.username))?;
    }

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"done": true}))))
}
```

#### 2.3 处理二进制数据

```rust
#[plugin_fn]
pub fn handle_binary(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // binary_data 就是 HashMap<String, Vec<u8>>，无需额外类型包装
    if !input.binary_data.is_empty() {
        for (key, data) in &input.binary_data {
            HostCaller::log_info(&format!("Binary key: {}, size: {} bytes", key, data.len()))?;

            // 处理二进制数据
            match key.as_str() {
                "image" => process_image(data)?,
                "document" => process_document(data)?,
                _ => {}
            }
        }
    }

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"processed": true}))))
}

fn process_image(data: &[u8]) -> Result<(), Error> {
    // 图像处理逻辑
    Ok(())
}

fn process_document(data: &[u8]) -> Result<(), Error> {
    Ok(())
}
```

### 三、输出构造

#### 3.1 构造成功输出

```rust
#[plugin_fn]
pub fn success_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 方式一：使用 from_json 创建 JSON 输出
    let output = FunctionOutput::from_json(serde_json::json!({
        "status": "success",
        "data": {
            "result": "processed",
            "count": 42
        }
    }));

    // 方式二：从任意可序列化类型创建
    #[derive(serde::Serialize)]
    struct MyResult { message: String, id: String }
    let output = FunctionOutput::from_result(MyResult {
        message: "操作成功".to_string(),
        id: "12345".to_string(),
    });

    Ok(Msgpack(output))
}
```

#### 3.2 构造失败输出

`FunctionOutput` 没有 error 辅助构造器；执行失败时直接返回 `Err`（携带错误消息），由宿主转为编排失败：

```rust
#[plugin_fn]
pub fn error_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    if input.as_str().is_empty() {
        // 返回业务错误（字符串消息会体现在编排执行记录中）
        return Err(Error::msg("INVALID_INPUT: 用户名不能为空"));
    }

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"ok": true}))))
}
```

#### 3.3 返回二进制数据

```rust
#[plugin_fn]
pub fn binary_output(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 生成缩略图
    let image_data = generate_thumbnail()?;

    // with_binary 链式添加二进制数据
    let output = FunctionOutput::from_json(serde_json::json!({"status": "file processed"}))
        .with_binary("thumbnail", image_data);

    Ok(Msgpack(output))
}

fn generate_thumbnail() -> Result<Vec<u8>, Error> {
    Ok(vec![])
}
```

### 四、宿主函数调用

#### 4.1 调用日志函数

```rust
use cmx_plugin_sdk::HostCaller;

#[plugin_fn]
pub fn logging_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 记录 Info 日志
    HostCaller::log_info("Starting processing")?;

    // 记录 Debug 日志
    HostCaller::log_debug("Debug information")?;

    // 记录 Warning 日志
    HostCaller::log_warn("Warning: low memory")?;

    // 记录 Error 日志
    HostCaller::log_error("An error occurred")?;

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"logged": true}))))
}
```

#### 4.2 调用缓存函数

缓存宿主函数（`cmx:buffer`）提供三个方法，均为「传参调用」风格而非请求结构体：

```rust
use cmx_plugin_sdk::{HostCaller, CacheResponse};

#[plugin_fn]
pub fn cache_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 设置缓存（key, JSON value, 可选 TTL 秒数）
    let set_resp: CacheResponse = HostCaller::cache_set(
        "user:001",
        serde_json::json!({"name": "张三", "age": 30}),
        Some(3600),
    )?;

    // 获取缓存（返回 CacheResponse { success, value, exists, error }）
    let get_resp: CacheResponse = HostCaller::cache_get("user:001")?;
    if let Some(value) = get_resp.value {
        HostCaller::log_info(&format!("Cached value: {}", value))?;
    }

    // 删除缓存
    let del_resp: CacheResponse = HostCaller::cache_delete("user:001")?;

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
        "set_ok": set_resp.success,
        "del_ok": del_resp.success,
    }))))
}
```

#### 4.3 调用数据库函数

`DbRequest` 字段：`sql` / `params`（旧 JSON 参数，向后兼容）/ `data_values`（带类型的 `DataValue` 参数，**优先于 params**，传类型化 NULL 必须用它）/ `dataset_id` / `db_id` / `txn_id`。`DbResponse` 字段：`success` / `affected_rows` / `dataset` / `txn_id`。

```rust
use cmx_plugin_sdk::{HostCaller, DbRequest, DbResponse};

#[plugin_fn]
pub fn database_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 执行查询（参数用 data_values 传带类型的 DataValue）
    let query_req = DbRequest {
        sql: "SELECT id, name FROM users WHERE id = $1".to_string(),
        data_values: Some(vec![1i64.into()]),
        ..Default::default()
    };

    let query_resp: DbResponse = HostCaller::db_query(query_req)?;
    if let Some(dataset) = query_resp.dataset {
        HostCaller::log_info(&format!("Query dataset: {} rows", dataset.rows.len()))?;
    }

    // 执行插入
    let insert_req = DbRequest {
        sql: "INSERT INTO logs (level, message, created_at) VALUES ($1, $2, NOW())".to_string(),
        data_values: Some(vec!["INFO".into(), "User logged in".into()]),
        ..Default::default()
    };

    let insert_resp: DbResponse = HostCaller::db_execute(insert_req)?;
    HostCaller::log_info(&format!("Inserted rows: {:?}", insert_resp.affected_rows))?;

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({"done": true}))))
}
```

#### 4.4 调用服务编排函数

`HostCaller::call_service_by_key` 在插件上下文中执行一个完整的服务编排（类似 API `/api/service/execute`）：

```rust
use cmx_plugin_sdk::{HostCaller, CallServiceRequest, CallServiceResponse};

#[plugin_fn]
pub fn service_call_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let call_req = CallServiceRequest {
        service_key: "user-service".to_string(),
        input: serde_json::json!({"user_id": 123}),
        include_steps: Some(true),
        ..Default::default()
    };

    let call_resp: CallServiceResponse = HostCaller::call_service_by_key(call_req)?;

    if call_resp.success {
        HostCaller::log_info(&format!("Service result: {:?}", call_resp.output))?;
    } else if let Some(err) = call_resp.error {
        HostCaller::log_error(&format!("Service error: {}", err.message))?;
    }

    Ok(Msgpack(FunctionOutput::from_json(call_resp.output.unwrap_or_default())))
}
```

#### 4.5 插件间调用与远程调用

```rust
use cmx_plugin_sdk::{HostCaller, PluginFunRequest, PluginFunCallResponse, CallServiceRequest};

#[plugin_fn]
pub fn plugin_call_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 调用本地另一个插件的函数（类似 API /api/service/call）
    let req = PluginFunRequest {
        plugin_id: "cmx_account".to_string(),
        function_name: "get_user".to_string(),
        input: serde_json::json!({"user_id": 123}),
        ..Default::default()
    };
    let resp: PluginFunCallResponse = HostCaller::call_plugin(req)?;

    // 远程变体：自动设置 server_name，经 RPC 调用指定服务上的插件/编排
    // HostCaller::call_remote_plugin(server_name, req)
    // HostCaller::call_remote_service(server_name, call_service_request)

    Ok(Msgpack(FunctionOutput::from_json(resp.result.unwrap_or_default())))
}
```

#### 4.6 IAM 用户/权限查询

`HostCaller` 提供一组 IAM 查询方法（宿主侧带缓存与熔断，走 `cmx:iam` 单一入口 `iam_query`）：

| 方法 | 说明 |
|------|------|
| `get_user_details(user_id)` | 单个用户详情（脱敏，无 password_hash） |
| `get_users_details(user_ids)` | 批量用户详情（`WHERE id = ANY($1)`，无 N+1） |
| `get_user_effective_permissions(user_id)` | 用户有效权限聚合（roles + permissions code） |
| `has_permission(user_id, code)` | 是否拥有指定权限码 |
| `has_role(user_id, code)` | 是否拥有指定角色码 |
| `has_permissions(user_id, codes)` | 批量权限校验（一次往返，按入参顺序返回 `WasmCheckResult`） |
| `has_roles(user_id, codes)` | 批量角色判断（同上） |

```rust
use cmx_plugin_sdk::HostCaller;

#[plugin_fn]
pub fn iam_example(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let allowed = HostCaller::has_permission("u-001", "order:read")?;
    let is_admin = HostCaller::has_role("u-001", "admin")?;
    let details = HostCaller::get_user_details("u-001")?;

    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
        "allowed": allowed,
        "is_admin": is_admin,
        "username": details.map(|u| u.username),
    }))))
}
```

### 五、错误处理

#### 5.1 使用 PluginError

`PluginError` 主要由 SDK 内部宿主调用封装使用（序列化/反序列化/宿主调用失败）；插件函数体中通常直接返回 `extism_pdk::Error`（如 `Error::msg(...)`）：

```rust
use cmx_plugin_sdk::PluginError;

fn validate_owner(owner: Option<&str>) -> Result<(), PluginError> {
    match owner {
        Some(o) if !o.is_empty() => Ok(()),
        _ => Err(PluginError::ArgumentError("owner 不能为空".to_string())),
    }
}
```

#### 5.2 错误类型说明

| 错误类型 | 说明 |
|----------|------|
| `PluginError::HostCallFailed` | 宿主函数调用失败（含宿主侧返回的失败信息） |
| `PluginError::SerializationError` | MsgPack 序列化失败 |
| `PluginError::DeserializationError` | MsgPack 反序列化失败 |
| `PluginError::ArgumentError` | 参数错误 |
| `PluginError::InternalError` | 内部错误 |

### 六、完整示例

#### 6.1 用户服务插件

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller};
use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct UserQuery {
    user_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserResponse {
    id: i64,
    name: String,
    email: String,
}

#[plugin_fn]
pub fn get_user(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 1. 解析输入
    let query: UserQuery = serde_json::from_value(input.as_json_value().clone())
        .map_err(Error::msg)?;

    // 2. 记录日志
    HostCaller::log_info(&format!("Fetching user: {}", query.user_id))?;

    // 3. 查询缓存
    let cache_key = format!("user:{}", query.user_id);
    let cached: Option<UserResponse> = match HostCaller::cache_get(&cache_key)?.value {
        Some(v) => serde_json::from_value(v).ok(),
        None => None,
    };

    if let Some(cached_user) = cached {
        return Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
            "user": cached_user,
            "source": "cache"
        }))));
    }

    // 4. 查询数据库（fetch_user_from_db 内部使用 HostCaller::db_query）
    let user = fetch_user_from_db(query.user_id)?;

    // 5. 写入缓存
    HostCaller::cache_set(&cache_key, serde_json::to_value(&user).unwrap_or_default(), Some(300))?;

    // 6. 返回结果
    Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
        "user": user,
        "source": "database"
    }))))
}

fn fetch_user_from_db(user_id: i64) -> Result<UserResponse, Error> {
    // 实现数据库查询逻辑（HostCaller::db_query）
    Ok(UserResponse {
        id: user_id,
        name: "张三".to_string(),
        email: "zhangsan@example.com".to_string(),
    })
}
```

#### 6.2 图像处理插件

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller};
use extism_pdk::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ImageProcessInput {
    action: String,
    format: Option<String>,
    quality: Option<u32>,
}

#[plugin_fn]
pub fn process_image(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 1. 解析输入
    let process_input: ImageProcessInput = serde_json::from_value(input.as_json_value().clone())
        .map_err(Error::msg)?;
    let image_data = input.binary_data.get("image")
        .ok_or_else(|| Error::msg("No image data provided"))?;

    // 2. 记录日志
    HostCaller::log_info(&format!(
        "Processing image: {} ({} bytes)",
        process_input.action,
        image_data.len()
    ))?;

    // 3. 处理图像
    let result = match process_input.action.as_str() {
        "resize" => resize_image(image_data, process_input.format.as_deref())?,
        "compress" => compress_image(image_data, process_input.quality.unwrap_or(80))?,
        "thumbnail" => create_thumbnail(image_data)?,
        _ => return Err(Error::msg(format!("Unknown action: {}", process_input.action))),
    };

    // 4. 构造输出（with_binary 添加二进制数据）
    let output = FunctionOutput::from_json(serde_json::json!({"status": "Image processed"}))
        .with_binary("result", result);

    Ok(Msgpack(output))
}

fn resize_image(data: &[u8], format: Option<&str>) -> Result<Vec<u8>, Error> {
    // 图像缩放逻辑
    Ok(data.to_vec())
}

fn compress_image(data: &[u8], quality: u32) -> Result<Vec<u8>, Error> {
    // 图像压缩逻辑
    Ok(data.to_vec())
}

fn create_thumbnail(data: &[u8]) -> Result<Vec<u8>, Error> {
    // 生成缩略图逻辑
    Ok(data.to_vec())
}
```

### 七、编译与部署

#### 7.1 编译为 WASM

```bash
# 安装 wasm32-unknown-unknown 目标
rustup target add wasm32-unknown-unknown

# 编译 release 版本
cargo build --release --target wasm32-unknown-unknown

# 编译 debug 版本（用于调试）
cargo build --target wasm32-unknown-unknown
```

#### 7.2 插件包结构

完整插件包除 WASM 产物外还包含清单、表定义与种子数据（标准范例见 `crates/libs/cmx-plugin-demo`）：

```
my-plugin/
├── manifest.json        # 插件清单（manifest_version + plugin{...} 元数据）
├── target/wasm32-unknown-unknown/release/my_plugin.wasm
├── config/              # 表定义配置入口（*_config.json → metadata/*_tables.json）
├── metadata/            # 表定义
├── seeddata/            # 种子数据
└── src/                 # 插件源码
```

#### 7.3 manifest.json 要点

清单为 `{ "manifest_version": "1.0", "plugin": { ... } }` 结构，`plugin` 内核心字段：`type: "wasm-plugin"`、`id`（下划线命名，如 `cmx_plugin_demo`）、`name` / `version` / `description`、`main_file`（编译产物文件名）、`table_config_files`（表定义入口）、`supported_databases`、`domain_code` / `application_code` / `module_code`、`vendor_*` 等。完整结构以 cmx-plugin-demo 的 `manifest.json` 为准（加载链：manifest.json → table_config_files → config → 建表 + 插数据）。
