# cmx-wasmdemo

> WASM 插件演示模块，用于验证 Extism 插件功能，提供各种演示函数。

## 项目简介

本模块用于验证 Extism 插件功能，提供元音字母统计、日志演示、缓存演示、数据库演示、插件调用演示等示例函数，以及服务编排和事务处理功能测试。

## 快速开始

### 编译

```bash
# 编译为 WASM 目标
cargo build --release --target wasm32-unknown-unknown
```

### 核心示例

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
use extism_pdk::*;

#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let current_input = &input.input;
    let initial_input = &input.context.initial_input;

    Ok(Msgpack(FunctionOutput::new("处理结果")))
}
```

## 导出函数列表

| 函数名 | 功能 | 适用场景 |
|--------|------|----------|
| `count_vowels` | 统计元音字母 | 简单字符串处理 |
| `demo_log` | 日志演示 | 调试日志功能 |
| `demo_cache` | 缓存演示 | 缓存读写操作 |
| `demo_database` | 数据库演示 | 数据库查询 |
| `demo_plugin_call` | 插件调用演示 | 插件间调用 |
| `run_all_demos` | 综合测试 | 功能验证 |
| `route_check` | 路由判断 | 服务编排分支 |
| `branch_1/2/3_process` | 分支处理 | 服务编排分支 |
| `merge_result` | 合并结果 | 服务编排合并 |
| `tx_insert/update/query/delete` | 事务操作 | 事务框测试 |
| `final_process` | 最终处理 | 服务编排结束 |

## 模块结构

```
cmx-wasmdemo
├── src/
│   └── lib.rs
│       ├── 业务数据结构 (DemoRequest, DemoResponse)
│       ├── 演示函数 (count_vowels, demo_log, demo_cache, demo_database, demo_plugin_call, run_all_demos)
│       ├── 服务编排函数 (route_check, branch_*_process, merge_result)
│       └── 事务处理函数 (tx_*, final_process)
└── Cargo.toml
```

## 使用指南

### 一、基础函数

#### 1.1 count_vowels - 元音字母统计

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput};
use extism_pdk::*;

/// 统计字符串中的元音字母数量
///
/// # 参数
/// - input: 输入字符串
///
/// # 返回值
/// - result: 元音字母数量
///
/// # 示例
/// ```json
/// {
///   "input": "Hello World",
///   "context": {...}
/// }
/// ```
/// 返回: `{"result": 3}` (e, o, o)
#[plugin_fn]
pub fn count_vowels(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let text = input.as_str();
    let vowels = text.matches(|c| "aeiouAEIOU".contains(c)).count();

    Ok(Json(FunctionOutput::from_json(serde_json::json!({
        "vowels": vowels,
        "input": text
    }))))
}
```

#### 1.2 demo_log - 日志演示

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput};
use extism_pdk::*;

/// 演示日志功能，支持多种日志级别
///
/// # 日志级别
/// - Info: 信息日志
/// - Error: 错误日志
/// - Debug: 调试日志
/// - Warn: 警告日志
///
/// # 示例
/// ```json
/// {
///   "input": "Test message",
///   "context": {...}
/// }
/// ```
#[plugin_fn]
pub fn demo_log(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let message = input.as_str();

    // 记录各级别日志
    HostCaller::log_info("Info: Processing started")?;
    HostCaller::log_debug(&format!("Debug: Input data: {}", message))?;
    HostCaller::log_warn("Warn: Low memory warning")?;
    HostCaller::log_error("Error: Operation failed")?;

    Ok(Json(FunctionOutput::success("Logs written")))
}
```

### 二、缓存操作

#### 2.1 demo_cache - 缓存演示

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, CacheRequest};
use extism_pdk::*;

/// 演示缓存的写入、读取和删除操作
///
/// # 操作流程
/// 1. 写入缓存 (cache_set)
/// 2. 读取缓存 (cache_get)
/// 3. 删除缓存 (cache_delete)
#[plugin_fn]
pub fn demo_cache(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let cache_key = "demo_cache_key";
    let cache_value = r#"{"data": "test_value", "timestamp": 1234567890}"#;

    // 1. 写入缓存，设置 TTL 为 3600 秒
    let set_req = CacheRequest {
        key: cache_key.to_string(),
        value: cache_value.to_string(),
        ttl_seconds: Some(3600),
    };
    let _: CacheResponse = HostCaller::cache_set(set_req)?;
    tracing::info!("Cache set: {} = {}", cache_key, cache_value);

    // 2. 读取缓存
    let get_req = CacheRequest {
        key: cache_key.to_string(),
        ..Default::default()
    };
    let cached: CacheResponse = HostCaller::cache_get(get_req)?;
    tracing::info!("Cache get: {}", cached.value);

    // 3. 删除缓存
    let del_req = CacheRequest {
        key: cache_key.to_string(),
        ..Default::default()
    };
    let _: CacheResponse = HostCaller::cache_delete(del_req)?;
    tracing::info!("Cache deleted: {}", cache_key);

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "set": cache_value,
        "get": cached.value,
        "deleted": true
    }))))
}
```

### 三、数据库操作

#### 3.1 demo_database - 数据库演示

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, DbRequest, ParamValue};
use extism_pdk::*;

/// 演示数据库查询功能
///
/// # 支持的操作
/// - SELECT 查询
/// - INSERT 插入
/// - UPDATE 更新
/// - DELETE 删除
#[plugin_fn]
pub fn demo_database(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 执行 SELECT 查询
    let query_req = DbRequest {
        sql: "SELECT id, name, email FROM users WHERE id = $1".to_string(),
        params: Some(vec![ParamValue::Int64(1)]),
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    let query_result: DbResponse = HostCaller::db_query(query_req)?;
    tracing::info!("Query result: {:?}", query_result.rows);

    // 执行 INSERT 操作
    let insert_req = DbRequest {
        sql: "INSERT INTO logs (level, message, created_at) VALUES ($1, $2, NOW())".to_string(),
        params: Some(vec![
            ParamValue::String("INFO".to_string()),
            ParamValue::String("Demo log message".to_string()),
        ]),
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };

    let insert_result: DbResponse = HostCaller::db_execute(insert_req)?;
    tracing::info!("Insert result: {} rows affected", insert_result.rows_affected);

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "query": query_result.rows,
        "insert": insert_result.rows_affected
    }))))
}
```

### 四、服务编排函数

#### 4.1 route_check - 路由判断

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput};
use extism_pdk::*;

/// 根据输入的 route 字段决定返回哪个分支标识
/// 用于 skylake-switch 节点
///
/// # 输入格式
/// ```json
/// {
///   "input": "branch_1",
///   "context": {...}
/// }
/// ```
///
/// # 返回值
/// - 返回对应的分支标识: "branch_1", "branch_2" 或 "branch_3"
#[plugin_fn]
pub fn route_check(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let route_value = input.as_str().trim().to_lowercase();

    let branch = match route_value.as_str() {
        "a" | "1" | "first" | "branch_1" => "branch_1",
        "b" | "2" | "second" | "branch_2" => "branch_2",
        "c" | "3" | "third" | "branch_3" => "branch_3",
        _ => "branch_1", // 默认分支
    };

    tracing::info!("Route decision: {} -> {}", route_value, branch);

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "selected_branch": branch,
        "route_value": route_value
    }))))
}
```

#### 4.2 branch_*_process - 分支处理

```rust
/// 分支1处理函数
#[plugin_fn]
pub fn branch_1_process(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let input_data = input.as_str();

    tracing::info!("Branch 1 processing: {}", input_data);

    // 执行业务逻辑 A
    let result = process_branch_1_logic(input_data)?;

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "branch": "branch_1",
        "result": result,
        "processed_at": chrono::Utc::now().to_rfc3339()
    }))))
}

/// 分支2处理函数
#[plugin_fn]
pub fn branch_2_process(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let input_data = input.as_str();

    tracing::info!("Branch 2 processing: {}", input_data);

    // 执行业务逻辑 B
    let result = process_branch_2_logic(input_data)?;

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "branch": "branch_2",
        "result": result,
        "processed_at": chrono::Utc::now().to_rfc3339()
    }))))
}

/// 分支3处理函数
#[plugin_fn]
pub fn branch_3_process(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let input_data = input.as_str();

    tracing::info!("Branch 3 processing: {}", input_data);

    // 执行业务逻辑 C
    let result = process_branch_3_logic(input_data)?;

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "branch": "branch_3",
        "result": result,
        "processed_at": chrono::Utc::now().to_rfc3339()
    }))))
}
```

#### 4.3 merge_result - 结果合并

```rust
/// 合并各分支的处理结果
///
/// # 输入格式
/// ```json
/// {
///   "input": "combined_result",
///   "context": {
///     "step_outputs": {
///       "branch_1": {"result": "A"},
///       "branch_2": {"result": "B"},
///       "branch_3": {"result": "C"}
///     }
///   }
/// }
/// ```
#[plugin_fn]
pub fn merge_result(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let ctx = &input.context;

    // 获取各分支的输出
    let branch_1_result = ctx.get_step_output("branch_1")
        .unwrap_or(serde_json::Value::Null);
    let branch_2_result = ctx.get_step_output("branch_2")
        .unwrap_or(serde_json::Value::Null);
    let branch_3_result = ctx.get_step_output("branch_3")
        .unwrap_or(serde_json::Value::Null);

    // 合并结果
    let merged = serde_json::json!({
        "branch_1": branch_1_result,
        "branch_2": branch_2_result,
        "branch_3": branch_3_result,
        "total_branches": 3
    });

    tracing::info!("Merged result: {:?}", merged);

    Ok(Json(FunctionOutput::success(merged)))
}
```

### 五、事务处理函数

#### 5.1 事务插入

```rust
/// 在事务中执行插入操作
///
/// # 输入格式
/// ```json
/// {
///   "input": {"table": "users", "data": {"name": "张三", "email": "zhangsan@example.com"}},
///   "context": {"txn_id": "tx_12345"}
/// }
/// ```
#[plugin_fn]
pub fn tx_insert(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    #[derive(serde::Deserialize)]
    struct InsertInput {
        table: String,
        data: serde_json::Value,
    }

    let insert_input = input.parse_json::<InsertInput>()?;
    let txn_id = input.context.txn_id.as_ref()
        .ok_or_else(|| PluginError::InvalidInput("No transaction ID".to_string()))?;

    // 构建 INSERT SQL
    if let Some(obj) = insert_input.data.as_object() {
        let columns: Vec<String> = obj.keys().map(|k| k.clone()).collect();
        let values: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            insert_input.table,
            columns.join(", "),
            values.join(", ")
        );

        let params: Vec<ParamValue> = obj.values()
            .map(|v| ParamValue::from(v.clone()))
            .collect();

        let req = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: Some(txn_id.clone()),
        };

        let result: DbResponse = HostCaller::db_execute(req)?;

        Ok(Json(FunctionOutput::success(serde_json::json!({
            "txn_id": txn_id,
            "table": insert_input.table,
            "rows_affected": result.rows_affected
        }))))
    } else {
        Err(PluginError::InvalidInput("Invalid data format".to_string()).into())
    }
}
```

#### 5.2 事务查询

```rust
/// 在事务中执行查询操作
#[plugin_fn]
pub fn tx_query(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    #[derive(serde::Deserialize)]
    struct QueryInput {
        sql: String,
        params: Option<Vec<ParamValue>>,
    }

    let query_input = input.parse_json::<QueryInput>()?;
    let txn_id = input.context.txn_id.clone();

    let req = DbRequest {
        sql: query_input.sql,
        params: query_input.params,
        dataset_id: None,
        db_id: None,
        txn_id,
    };

    let result: DbResponse = HostCaller::db_query(req)?;

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "rows": result.rows,
        "row_count": result.rows.len()
    }))))
}
```

#### 5.3 最终处理

```rust
/// 服务编排的最终处理函数
///
/// # 功能
/// - 汇总所有步骤的结果
/// - 生成最终响应
/// - 记录执行日志
#[plugin_fn]
pub fn final_process(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let ctx = &input.context;

    // 收集所有步骤的输出
    let mut step_results = serde_json::Map::new();
    for (step_id, output) in &ctx.step_outputs {
        step_results.insert(step_id.clone(), output.clone());
    }

    // 生成最终结果
    let final_result = serde_json::json!({
        "status": "completed",
        "initial_input": ctx.initial_input,
        "step_count": ctx.step_outputs.len(),
        "step_results": step_results,
        "completed_at": chrono::Utc::now().to_rfc3339()
    });

    tracing::info!("Final result: {:?}", final_result);

    Ok(Json(FunctionOutput::success(final_result)))
}
```

### 六、综合测试

#### 6.1 run_all_demos - 运行所有演示

```rust
/// 执行所有演示功能，用于验证插件环境是否正常
///
/// # 测试内容
/// 1. 日志功能
/// 2. 缓存操作
/// 3. 数据库操作
/// 4. 上下文传递
#[plugin_fn]
pub fn run_all_demos(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    let mut results = serde_json::Map::new();

    // 1. 测试日志
    HostCaller::log_info("Demo: Testing log functionality")?;
    results.insert("log".to_string(), serde_json::json!({"status": "ok"}));

    // 2. 测试缓存
    let cache_key = "demo_test_key";
    let cache_req = CacheRequest {
        key: cache_key.to_string(),
        value: "test_value".to_string(),
        ttl_seconds: Some(60),
    };
    let _: CacheResponse = HostCaller::cache_set(cache_req)?;
    results.insert("cache".to_string(), serde_json::json!({"status": "ok"}));

    // 3. 测试数据库
    let db_req = DbRequest {
        sql: "SELECT 1 as test".to_string(),
        params: None,
        dataset_id: None,
        db_id: None,
        txn_id: None,
    };
    let _: DbResponse = HostCaller::db_query(db_req)?;
    results.insert("database".to_string(), serde_json::json!({"status": "ok"}));

    // 4. 验证上下文
    results.insert("context".to_string(), serde_json::json!({
        "initial_input": input.context.initial_input,
        "has_headers": !input.context.headers.is_empty()
    }));

    Ok(Json(FunctionOutput::success(serde_json::json!({
        "all_demos_passed": true,
        "results": results,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}
```

### 七、插件调用

#### 7.1 demo_plugin_call - 插件间调用

```rust
use cmx_plugin_sdk::{FunctionInput, FunctionOutput, HostCaller, ServiceCallRequest};
use extism_pdk::*;

/// 演示调用其他插件的服务
///
/// # 输入格式
/// ```json
/// {
///   "input": "data to process",
///   "context": {...}
/// }
/// ```
///
/// # 调用方式
/// 使用 HostCaller::call_service 调用其他插件的服务函数
#[plugin_fn]
pub fn demo_plugin_call(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
    // 准备调用请求
    let call_req = ServiceCallRequest {
        service_id: "target-service".to_string(),
        function_name: "process".to_string(),
        input: serde_json::json!({
            "data": input.as_str(),
            "source": "demo_plugin_call"
        }),
        trace_id: None,
        timeout_ms: Some(5000),
    };

    // 调用其他插件的服务
    let call_resp: ServiceCallResponse = HostCaller::call_service(call_req)?;

    if call_resp.success {
        Ok(Json(FunctionOutput::success(serde_json::json!({
            "status": "success",
            "plugin_response": call_resp.output,
            "source": "demo_plugin_call"
        }))))
    } else {
        Ok(Json(FunctionOutput::error(serde_json::json!({
            "status": "failed",
            "error": call_resp.error,
            "source": "demo_plugin_call"
        }))))
    }
}
```

### 八、编译与部署

#### 8.1 编译

```bash
# 安装 wasm32 目标
rustup target add wasm32-unknown-unknown

# Debug 构建
cargo build --target wasm32-unknown-unknown

# Release 构建
cargo build --release --target wasm32-unknown-unknown

# 查看输出
ls -la target/wasm32-unknown-unknown/debug/
# 或
ls -la target/wasm32-unknown-unknown/release/
```

#### 8.2 部署

```bash
# 复制 WASM 文件到插件目录
cp target/wasm32-unknown-unknown/release/plugin.wasm /path/to/plugins/my-demo/

# 创建 manifest.json
cat > /path/to/plugins/my-demo/manifest.json << 'EOF'
{
  "id": "cmx-wasmdemo",
  "name": "WASM Demo Plugin",
  "version": "0.1.0",
  "functions": [
    "count_vowels",
    "demo_log",
    "demo_cache",
    "demo_database",
    "run_all_demos"
  ]
}
EOF
```
