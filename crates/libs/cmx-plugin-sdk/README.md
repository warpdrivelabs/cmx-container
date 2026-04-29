# cmx-plugin-sdk

> 基于 Extism PDK 的插件开发 SDK，提供宿主函数调用封装、插件函数导出宏、错误类型定义和标准入参出参类型。

## 项目简介

本 SDK 用于开发 CMX 平台的 WASM 插件，所有服务编排中的函数都应使用统一的入参和出参格式。

## 快速开始

### 安装

```toml
[dependencies]
cmx-plugin-sdk = "0.1.0"
```

### 核心示例

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
use extism_pdk::*;

#[plugin_fn]
pub fn my_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let current_input = input.as_str();
    let json_value = input.as_json_value();
    let initial_input = &input.context.initial_input;
    let headers = &input.context.headers;

    if let Some(prev_output) = input.context.get_step_output("previous_node_id") {
        // 使用前序步骤输出
    }

    Ok(Json(FunctionOutput::from_json(serde_json::json!({
        "status": "success",
        "data": "处理结果"
    }))))
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 标准入参出参格式 | 所有函数使用统一的 `FunctionInput` 和 `FunctionOutput` |
| 上下文管理 | 包含初始输入、请求头、步骤输出等完整上下文信息 |
| 宿主函数调用封装 | `HostCaller` 结构体用于调用宿主函数 |
| 错误处理 | 自定义 `PluginError` 错误类型 |
| 二进制数据支持 | 支持在函数间传递二进制数据 |

## 模块结构

```
cmx-plugin-sdk
├── src/
│   ├── lib.rs             # 主模块入口
│   ├── host_calls.rs      # 宿主函数调用封装
│   └── error.rs           # 自定义错误类型
└── Cargo.toml
```

## 主要类型说明

### `FunctionInput`

所有服务编排中的函数都应该使用此结构体作为入参。

- `input`: 当前步骤输入数据（JSON 字符串或纯文本）
- `context`: 服务调用上下文
- `binary_data`: 二进制数据

### `FunctionOutput`

所有服务编排中的函数都应该使用此结构体作为出参。

- `result`: 函数执行结果
- `binary_data`: 二进制数据

### `SVRContext`

包含服务调用的完整上下文信息。

- `initial_input`: 初始调用入参
- `headers`: HTTP 请求头信息
- `step_outputs`: 各步骤执行结果的缓存

## 使用指南

### 一、函数签名规范

#### 1.1 标准函数签名

所有服务编排函数必须使用以下签名格式：

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput};
use extism_pdk::*;

// JSON 格式输入输出
#[plugin_fn]
pub fn my_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 函数逻辑
    Ok(Json(FunctionOutput::success("result")))
}

// MessagePack 格式输入输出
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // 函数逻辑
    Ok(Msgpack(FunctionOutput::success("result")))
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
pub fn process_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 方式一：获取纯文本输入
    let text_input = input.as_str();
    println!("Received text: {}", text_input);

    // 方式二：解析为 JSON Value（宽松模式）
    let json_value = input.as_json_value();
    if let Some(value) = json_value {
        println!("JSON input: {:?}", value);
    }

    // 方式三：解析为强类型结构体
    #[derive(serde::Deserialize)]
    struct MyInput {
        name: String,
        age: u32,
    }

    match input.parse_json::<MyInput>() {
        Ok(parsed) => {
            println!("Name: {}, Age: {}", parsed.name, parsed.age);
        }
        Err(e) => {
            return Err(PluginError::InvalidInput(format!("Failed to parse input: {}", e)).into());
        }
    }

    Ok(Json(FunctionOutput::success("processed")))
}
```

#### 2.2 访问上下文数据

```rust
#[plugin_fn]
pub fn process_with_context(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let ctx = &input.context;

    // 获取初始入参
    let initial = &ctx.initial_input;
    println!("Initial input: {:?}", initial);

    // 获取 HTTP 请求头
    let headers = &ctx.headers;
    if let Some(auth) = headers.get("Authorization") {
        println!("Authorization: {}", auth);
    }

    // 获取前序步骤的输出
    // 在服务编排中，每个步骤可以通过 context 获取前序步骤的结果
    if let Some(prev_result) = ctx.get_step_output("previous_node_id") {
        println!("Previous step result: {:?}", prev_result);
    }

    // 获取事务 ID（如果有）
    if let Some(txn_id) = &ctx.txn_id {
        println!("Transaction ID: {}", txn_id);
    }

    Ok(Json(FunctionOutput::success("done")))
}
```

#### 2.3 处理二进制数据

```rust
use cmx_plugin_sdk::BinaryData;

#[plugin_fn]
pub fn handle_binary(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 检查是否有二进制数据
    if !input.binary_data.is_empty() {
        for (key, data) in &input.binary_data {
            println!("Binary key: {}, size: {} bytes", key, data.len());

            // 处理二进制数据
            match key.as_str() {
                "image" => process_image(data)?,
                "document" => process_document(data)?,
                _ => {}
            }
        }
    }

    Ok(Json(FunctionOutput::success("binary processed")))
}

fn process_image(data: &[u8]) -> PluginResult<()> {
    // 图像处理逻辑
    Ok(())
}
```

### 三、输出构造

#### 3.1 构造成功输出

```rust
#[plugin_fn]
pub fn success_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 方式一：使用 from_json 创建 JSON 输出
    let output = FunctionOutput::from_json(serde_json::json!({
        "status": "success",
        "data": {
            "result": "processed",
            "count": 42
        }
    }));

    // 方式二：使用辅助方法
    let output = FunctionOutput::success("simple result");

    // 方式三：构建复杂输出
    let mut output = FunctionOutput::default();
    output.set_result(serde_json::json!({
        "message": "操作成功",
        "id": "12345"
    }));

    Ok(Json(output))
}
```

#### 3.2 构造错误输出

```rust
#[plugin_fn]
pub fn error_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 返回业务错误
    let error_output = FunctionOutput::error("INVALID_INPUT", "用户名不能为空");

    // 返回系统错误
    let system_error = FunctionOutput::error("SYSTEM_ERROR", "数据库连接失败");

    Ok(Json(error_output))
}
```

#### 3.3 返回二进制数据

```rust
#[plugin_fn]
pub fn binary_output(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let mut output = FunctionOutput::success("file processed");

    // 添加二进制数据到输出
    let image_data = generate_thumbnail()?;
    output.add_binary("thumbnail", image_data);

    Ok(Json(output))
}
```

### 四、宿主函数调用

#### 4.1 调用日志函数

```rust
use cmx_plugin_sdk::HostCaller;

#[plugin_fn]
pub fn logging_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 记录 Info 日志
    HostCaller::log_info("Starting processing")?;

    // 记录 Debug 日志
    HostCaller::log_debug("Debug information")?;

    // 记录 Warning 日志
    HostCaller::log_warn("Warning: low memory")?;

    // 记录 Error 日志
    HostCaller::log_error("An error occurred")?;

    Ok(Json(FunctionOutput::success("logged")))
}
```

#### 4.2 调用缓存函数

```rust
use cmx_plugin_sdk::{HostCaller, CacheRequest, CacheResponse};

#[plugin_fn]
pub fn cache_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 设置缓存
    let set_req = CacheRequest {
        key: "user:001".to_string(),
        value: r#"{"name":"张三","age":30}"#.to_string(),
        ttl_seconds: Some(3600),
    };
    let _: CacheResponse = HostCaller::cache_set(set_req)?;

    // 获取缓存
    let get_req = CacheRequest {
        key: "user:001".to_string(),
        ..Default::default()
    };
    let cache_response: CacheResponse = HostCaller::cache_get(get_req)?;
    println!("Cached value: {}", cache_response.value);

    // 删除缓存
    let del_req = CacheRequest {
        key: "user:001".to_string(),
        ..Default::default()
    };
    let _: CacheResponse = HostCaller::cache_delete(del_req)?;

    Ok(Json(FunctionOutput::success("cache operations done")))
}
```

#### 4.3 调用数据库函数

```rust
use cmx_plugin_sdk::{HostCaller, DbRequest, DbResponse, ParamValue};

#[plugin_fn]
pub fn database_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 执行查询
    let query_req = DbRequest {
        sql: "SELECT id, name FROM users WHERE id = $1".to_string(),
        params: Some(vec![
            ParamValue::Int64(1)
        ]),
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    let query_resp: DbResponse = HostCaller::db_query(query_req)?;
    println!("Query result: {:?}", query_resp.rows);

    // 执行插入
    let insert_req = DbRequest {
        sql: "INSERT INTO logs (level, message, created_at) VALUES ($1, $2, NOW())".to_string(),
        params: Some(vec![
            ParamValue::String("INFO".to_string()),
            ParamValue::String("User logged in".to_string()),
        ]),
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    let insert_resp: DbResponse = HostCaller::db_execute(insert_req)?;
    println!("Inserted rows: {}", insert_resp.rows_affected);

    Ok(Json(FunctionOutput::success("database operations done")))
}
```

#### 4.4 调用服务编排函数

```rust
use cmx_plugin_sdk::{HostCaller, ServiceCallRequest, ServiceCallResponse};

#[plugin_fn]
pub fn service_call_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let call_req = ServiceCallRequest {
        service_id: "user-service".to_string(),
        function_name: "get_user".to_string(),
        input: serde_json::json!({"user_id": 123}),
        trace_id: None,
        timeout_ms: Some(5000),
    };

    let call_resp: ServiceCallResponse = HostCaller::call_service(call_req)?;

    if call_resp.success {
        println!("Service result: {:?}", call_resp.output);
    } else {
        eprintln!("Service error: {:?}", call_resp.error);
    }

    Ok(Json(FunctionOutput::from_json(serde_json::json!({
        "result": call_resp.output
    }))))
}
```

### 五、插件数据管理

#### 5.1 存储插件数据

```rust
use cmx_plugin_sdk::{HostCaller, PluginDataRequest, PluginDataResponse};

#[plugin_fn]
pub fn store_data_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let store_req = PluginDataRequest {
        plugin_id: "my-plugin".to_string(),
        key: "config".to_string(),
        value: serde_json::json!({
            "setting1": "value1",
            "setting2": 42
        }),
        encrypted: Some(true),
    };

    let _: PluginDataResponse = HostCaller::plugin_data_set(store_req)?;

    Ok(Json(FunctionOutput::success("data stored")))
}
```

#### 5.2 读取插件数据

```rust
#[plugin_fn]
pub fn read_data_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let read_req = PluginDataRequest {
        plugin_id: "my-plugin".to_string(),
        key: "config".to_string(),
        value: serde_json::Value::Null,
        encrypted: Some(true),
    };

    let data_resp: PluginDataResponse = HostCaller::plugin_data_get(read_req)?;

    if let Some(value) = data_resp.value {
        println!("Retrieved config: {:?}", value);
    }

    Ok(Json(FunctionOutput::from_json(data_resp.value.unwrap_or_default())))
}
```

### 六、错误处理

#### 6.1 使用 PluginError

```rust
use cmx_plugin_sdk::PluginError;

#[plugin_fn]
pub fn error_handling_example(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 验证输入
    if input.as_str().is_empty() {
        return Err(PluginError::InvalidInput("Input cannot be empty".to_string()).into());
    }

    // 业务逻辑错误
    let valid_users = vec!["alice", "bob", "charlie"];
    if !valid_users.contains(&input.as_str()) {
        return Err(PluginError::Unauthorized("User not found".to_string()).into());
    }

    // 系统错误
    let result = some_operation()?;
    if result.is_err() {
        return Err(PluginError::System("Operation failed".to_string()).into());
    }

    Ok(Json(FunctionOutput::success("success")))
}
```

#### 6.2 错误类型说明

| 错误类型 | 说明 |
|----------|------|
| `PluginError::InvalidInput` | 输入参数无效 |
| `PluginError::Unauthorized` | 未授权访问 |
| `PluginError::NotFound` | 资源不存在 |
| `PluginError::Conflict` | 资源冲突 |
| `PluginError::Internal` | 内部错误 |
| `PluginError::System` | 系统错误 |

### 七、完整示例

#### 7.1 用户服务插件

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
pub fn get_user(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 1. 解析输入
    let query = input.parse_json::<UserQuery>()?;

    // 2. 记录日志
    HostCaller::log_info(&format!("Fetching user: {}", query.user_id))?;

    // 3. 查询缓存
    let cache_key = format!("user:{}", query.user_id);
    let cached = check_cache(&cache_key)?;

    if let Some(cached_user) = cached {
        return Ok(Json(FunctionOutput::from_json(serde_json::json!({
            "user": cached_user,
            "source": "cache"
        }))));
    }

    // 4. 查询数据库
    let user = fetch_user_from_db(query.user_id)?;

    // 5. 写入缓存
    set_cache(&cache_key, &user)?;

    // 6. 返回结果
    Ok(Json(FunctionOutput::from_json(serde_json::json!({
        "user": user,
        "source": "database"
    }))))
}

fn check_cache(key: &str) -> PluginResult<Option<UserResponse>> {
    // 实现缓存检查逻辑
    Ok(None)
}

fn set_cache(key: &str, user: &UserResponse) -> PluginResult<()> {
    // 实现缓存设置逻辑
    Ok(())
}

fn fetch_user_from_db(user_id: i64) -> PluginResult<UserResponse> {
    // 实现数据库查询逻辑
    Ok(UserResponse {
        id: user_id,
        name: "张三".to_string(),
        email: "zhangsan@example.com".to_string(),
    })
}
```

#### 7.2 图像处理插件

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, BinaryData};
use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct ImageProcessInput {
    action: String,
    format: Option<String>,
    quality: Option<u32>,
}

#[plugin_fn]
pub fn process_image(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 1. 解析输入
    let process_input = input.parse_json::<ImageProcessInput>()?;
    let image_data = input.binary_data.get("image")
        .ok_or_else(|| PluginError::InvalidInput("No image data provided".to_string()))?;

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
        _ => return Err(PluginError::InvalidInput(format!(
            "Unknown action: {}",
            process_input.action
        )).into()),
    };

    // 4. 构造输出
    let mut output = FunctionOutput::success("Image processed");
    output.add_binary("result", result);

    Ok(Json(output))
}

fn resize_image(data: &[u8], format: Option<&str>) -> PluginResult<Vec<u8>> {
    // 图像缩放逻辑
    Ok(data.to_vec())
}

fn compress_image(data: &[u8], quality: u32) -> PluginResult<Vec<u8>> {
    // 图像压缩逻辑
    Ok(data.to_vec())
}

fn create_thumbnail(data: &[u8]) -> PluginResult<Vec<u8>> {
    // 生成缩略图逻辑
    Ok(data.to_vec())
}
```

### 八、编译与部署

#### 8.1 编译为 WASM

```bash
# 安装 wasm32-unknown-unknown 目标
rustup target add wasm32-unknown-unknown

# 编译 release 版本
cargo build --release --target wasm32-unknown-unknown

# 编译 debug 版本（用于调试）
cargo build --target wasm32-unknown-unknown
```

#### 8.2 插件包结构

```
my-plugin/
├── plugin.wasm          # 编译后的 WASM 文件
├── manifest.json       # 插件清单
└── config/
    └── settings.json   # 插件配置
```

#### 8.3 manifest.json 示例

```json
{
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "A sample plugin",
  "main": "plugin.wasm",
  "functions": [
    "get_user",
    "process_image"
  ],
  "permissions": [
    "database:read",
    "database:write",
    "cache:read",
    "cache:write"
  ]
}
```
