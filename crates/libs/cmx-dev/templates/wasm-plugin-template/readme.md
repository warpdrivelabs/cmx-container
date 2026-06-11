# WASM Plugin 开发模板

基于 Extism PDK + cmx-plugin-sdk 的 WASM 插件项目模板，用于在 CMX 平台中开发可插拔的业务功能模块。

## 项目结构

```
<plugin_id>/
├── .cargo/
│   └── config.toml              # Cargo 配置（镜像源、私有 registry）
├── .vscode/
│   └── launch.json              # VS Code 调试配置（lldb 调试测试）
├── .editorconfig                # 编辑器格式配置
├── Cargo.toml                   # Rust 项目配置
├── manifest.json                # 插件清单（核心配置文件）
├── readme.md                    # 项目说明
├── config/
│   └── domain_app_module_config.json  # 领域-应用-模块配置
├── metadata/                    # 元数据目录
├── seeddata/                    # 初始化数据目录
├── servicedata/                 # 服务数据目录
├── menudata/
│   └── sample-menu.json         # 菜单配置示例
├── formdata/
│   └── sample-form.json         # 表单配置示例
├── mcpdata/
│   └── sample-skill.json        # MCP/Skills 配置示例
└── src/
    ├── lib.rs                   # 模块入口
    ├── models.rs                # 数据模型定义
    ├── host_traits.rs           # 宿主功能 trait 定义
    ├── core.rs                  # 核心业务逻辑
    ├── extism_layer.rs          # Extism PDK 适配层
    └── tests.rs                 # 单元测试
```

---

## 架构设计

模板采用**三层分离**架构，将业务逻辑与 WASM 运行时解耦，确保核心逻辑可独立测试：

```
┌──────────────────────────────────────────────┐
│              extism_layer.rs                  │  ← Extism PDK 适配层
│  #[plugin_fn] + Msgpack 编解码               │     （编译为 WASM 时启用）
│  实现 HostFunctions trait → 委托 HostCaller  │
├──────────────────────────────────────────────┤
│                 core.rs                       │  ← 核心业务逻辑层
│  PluginCore<H: HostFunctions>                │     （纯逻辑，不依赖 Extism）
│  所有业务函数在此实现                         │
├──────────────────────────────────────────────┤
│             host_traits.rs                    │  ← 宿主功能抽象层
│  trait HostFunctions                          │     （可 mock 测试）
│  日志 / 缓存 / 数据库 / 插件调用 / 服务编排  │
├──────────────────────────────────────────────┤
│               models.rs                       │  ← 数据模型层
│  FunctionInput / FunctionOutput / 自定义模型  │
└──────────────────────────────────────────────┘
```

**设计优势：**

- `core.rs` 中的业务逻辑通过泛型 `H: HostFunctions` 依赖抽象，不直接依赖 Extism PDK
- 测试时使用 `MockHostFunctions`（由 `mockall` 自动生成），无需真实宿主环境
- `extism_layer.rs` 仅负责将 `PluginCore` 的方法暴露为 Extism 插件函数，是薄适配层

---

## 编写自定义函数

### 步骤一：定义数据模型

在 `src/models.rs` 中定义函数的输入/输出结构体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyRequest {
    pub field_a: String,
    pub field_b: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyResponse {
    pub result: String,
    pub count: i32,
}
```

### 步骤二：实现核心逻辑

在 `src/core.rs` 的 `PluginCore` 中添加业务方法：

```rust
impl<H: HostFunctions> PluginCore<H> {
    pub fn my_function(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: MyRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| e.to_string())?;

        self.host.log_info(&format!("处理请求: {:?}", request))?;

        let response = MyResponse {
            result: format!("已处理: {}", request.field_a),
            count: request.field_b,
        };

        Ok(FunctionOutput::from_json(
            serde_json::to_value(&response).map_err(|e| e.to_string())?
        ))
    }
}
```

### 步骤三：暴露插件函数

在 `src/extism_layer.rs` 中添加 Extism 入口函数：

```rust
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.my_function(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

### 步骤四：编写单元测试

在 `src/tests.rs` 中添加 mock 测试：

```rust
#[test]
fn test_my_function() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"field_a": "test", "field_b": 42}));
    let result = core.my_function(&input).unwrap();
    let parsed: MyResponse = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed.count, 42);
}
```

---

## 宿主功能 API（HostFunctions Trait）

通过 `self.host` 在 `PluginCore` 中调用以下宿主能力：

### 日志

| 方法 | 说明 |
|------|------|
| `host.log_info(msg: &str)` | 记录 INFO 级别日志 |
| `host.log_error(msg: &str)` | 记录 ERROR 级别日志 |
| `host.log_debug(msg: &str)` | 记录 DEBUG 级别日志 |
| `host.log_warn(msg: &str)` | 记录 WARN 级别日志 |

### 缓存

| 方法 | 说明 |
|------|------|
| `host.cache_get(key: &str)` | 获取缓存值，返回 `CacheResponse` |
| `host.cache_set(key: &str, value: Value, ttl: Option<u64>)` | 设置缓存，`ttl` 为过期秒数 |
| `host.cache_delete(key: &str)` | 删除缓存 |

### 数据库

| 方法 | 说明 |
|------|------|
| `host.db_query(request: DbRequest)` | 执行 SELECT 查询，返回 `DbResponse` |
| `host.db_execute(request: DbRequest)` | 执行 INSERT/UPDATE/DELETE，返回 `DbResponse` |

`DbRequest` 字段：

```rust
DbRequest {
    sql: "SELECT * FROM table".to_string(),  // SQL 语句
    params: None,                              // SQL 参数（JSON 数组）
    dataset_id: None,                          // 数据集 ID
    db_id: None,                               // 数据库 ID（不指定用默认）
    txn_id: None,                              // 事务 ID（事务操作必填）
}
```

### 插件调用

| 方法 | 说明 |
|------|------|
| `host.call_plugin(request: PluginFunRequest)` | 调用另一个插件的函数 |
| `host.call_service_by_key(request: CallServiceRequest)` | 调用服务编排 |

`PluginFunRequest` 字段：

```rust
PluginFunRequest {
    plugin_id: "target-plugin".to_string(),     // 目标插件 ID
    function_name: "some_function".to_string(), // 目标函数名
    input: serde_json::json!({...}),             // 输入数据
    initial_input: None,                         // 初始输入（调试用）
    debug: Some(false),                          // 调试模式
}
```

`CallServiceRequest` 字段：

```rust
CallServiceRequest {
    service_key: "bm".to_string(),              // 服务标识
    input: serde_json::json!({...}),             // 输入数据
    include_steps: Some(true),                   // 是否返回步骤详情
    debug: Some(false),                          // 调试模式
    debug_node_id: None,                         // 调试目标节点 ID
    debug_params: None,                          // 调试参数
}
```

---

## FunctionInput 上下文使用

`FunctionInput` 包含 `input`（当前输入）和 `context`（SVRContext 上下文）：

```rust
pub struct FunctionInput {
    pub input: serde_json::Value,            // 当前步骤输入
    pub context: SVRContext,                  // 服务调用上下文
    pub binary_data: HashMap<String, Vec<u8>>, // 二进制数据
}
```

### SVRContext 常用操作

```rust
let initial_input = &input.context.initial_input;

let prev_output = input.context.get_step_output("previous_node_id");

let txn_id = input.context.txn_id.clone();

let request_id = &input.context.request_id;
```

| 字段/方法 | 说明 |
|-----------|------|
| `initial_input` | 服务调用的初始输入参数 |
| `step_outputs` | 各步骤的输出缓存 |
| `get_step_output(node_id)` | 获取指定步骤的输出 |
| `txn_id` | 事务 ID（事务操作时由宿主注入） |
| `request_id` | 请求唯一标识 |
| `headers` | 请求头信息 |
| `time_in` | 请求进入时间 |

---

## 服务编排场景

模板中包含了服务编排的典型场景示例：

### 路由分支

`route_check` 根据输入返回分支标识，宿主根据返回值决定执行路径：

```rust
pub fn route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let route_input: RouteInput = serde_json::from_value(input.input.clone())?;
    let result = match route_input.route.trim() {
        "1" => "1",
        "2" => "2",
        _ => "1",
    };
    Ok(FunctionOutput::from_json(serde_json::to_value(result)?))
}
```

### 事务操作

在事务中执行多个数据库操作时，通过 `input.context.txn_id` 确保同一事务：

```rust
pub fn tx_insert(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let txn_id = input.context.txn_id.clone();
    let query_request = DbRequest {
        sql: "INSERT INTO ...".to_string(),
        txn_id,
        ..Default::default()
    };
    self.host.db_execute(query_request)?;
    // ...
}
```

### 结果合并

`merge_result` 从上下文中获取各分支输出进行合并：

```rust
pub fn merge_result(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let branch_output = input.context.get_step_output("branch_1_func")
        .or_else(|| input.context.get_step_output("branch_2_func"))
        .cloned();
    // ...
}
```

### 数据流设计

在服务编排中，当前节点从哪里获取数据，取决于**前序节点的输出是否包含所需字段**：

| 前序节点输出 | 数据获取方式 | 说明 |
|-------------|-------------|------|
| 包含所需字段 | `input.input` | 直接使用前序节点的输出 |
| 不含所需字段 | `input.context.initial_input` | 从原始输入获取业务参数 |
| 需要特定步骤 | `input.context.get_step_output("node_id")` | 按节点ID获取指定步骤输出 |

**switch 节点特殊行为**：switch 节点的返回值仅用于路由判断，不会传递给下一个节点。switch 后的节点收到的 `input.input` 与 switch 执行前相同。

**典型场景**：当流程为 `start → switch → branch → merge → 数据库操作` 时，需要分析 merge 的输出结构：
- 如果 merge 输出包含业务字段 → 数据库操作使用 `input.input`
- 如果 merge 输出不含业务字段 → 数据库操作使用 `input.context.initial_input`

```rust
// 前序节点输出不含业务字段时：从 initial_input 获取
pub fn tx_create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: CreateOrderRequest = serde_json::from_value(
        input.context.initial_input.clone()
    ).map_err(|e| format!("参数解析失败: {}", e))?;
    // ...
}

// 前序节点输出包含业务字段时：直接使用 input.input
pub fn tx_create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
        .map_err(|e| format!("参数解析失败: {}", e))?;
    // ...
}
```

---

## manifest.json 配置说明

`manifest.json` 是插件的核心清单文件，定义了插件的元数据和打包范围：

| 字段 | 说明 |
|------|------|
| `manifest_version` | 清单版本，当前为 `1.0` |
| `plugin.id` | 插件唯一标识 |
| `plugin.name` | 插件显示名称 |
| `plugin.version` | 插件版本号 |
| `plugin.main_file` | WASM 文件名 |
| `plugin.datasource_id` | 关联数据源 ID |
| `plugin.domain_code` | 领域编码 |
| `plugin.application_code` | 应用编码 |
| `plugin.module_code` | 模块编码 |
| `plugin.table_config_files` | 表配置文件列表 |
| `plugin.supported_databases` | 支持的数据库类型 |
| `plugin.dependencies` | 依赖的其他插件 |
| `plugin.extra_files` | 额外打包的文件 |
| `entries` | 打包时包含的文件 glob 列表 |

### 数据目录说明

| 目录 | 用途 |
|------|------|
| `config/` | 表结构配置（domain_app_module_config 等） |
| `metadata/` | 插件元数据 |
| `seeddata/` | 初始化数据 |
| `servicedata/` | 服务数据 |
| `menudata/` | 菜单配置（JSON 格式） |
| `formdata/` | 表单配置（JSON 格式） |
| `mcpdata/` | MCP/Skills 配置（JSON 格式） |

---

## 编译与测试

### 编译 WASM

```bash
# 安装 WASM 目标（首次）
rustup target add wasm32-wasip1

# 编译 release 版本
cargo build --release --target wasm32-wasip1 --features extism
```

编译产物位于 `target/wasm32-wasip1/release/<plugin_id>.wasm`。

### 运行测试

```bash
# 运行单元测试（不需要 extism feature，使用 mock）
cargo test
```

---

## 调试

### 单元测试调试（推荐）

由于模板采用三层分离架构，`core.rs` 中的业务逻辑与 Extism PDK 完全解耦，因此推荐通过**单元测试 + Mock** 进行调试，这是最高效的方式。

#### 命令行调试

```bash
# 运行所有测试，显示打印输出
cargo test -- --nocapture

# 运行指定测试
cargo test test_count_vowels -- --nocapture

# 运行单个测试并显示详细输出
cargo test test_my_function -- --nocapture --exact
```

#### VS Code 断点调试

项目已内置 `.vscode/launch.json` 调试配置，支持 lldb 断点调试：

1. 确保安装 VS Code 扩展：**CodeLLDB**（`vadimcn.vscode-lldb`）
2. 在 `src/core.rs` 或 `src/tests.rs` 中设置断点
3. 按 `F5` 或从调试面板选择配置启动：
   - **Debug All Tests** — 调试所有测试
   - **Debug Specific Test** — 调试指定测试（输入测试函数名，如 `test_count_vowels`）

#### Mock 调试技巧

通过配置 `MockHostFunctions` 的返回值，模拟不同场景进行调试：

```rust
#[test]
fn test_with_custom_mock() {
    let mut mock = MockHostFunctions::new();

    // 模拟日志调用
    mock.expect_log_info()
        .returning(|msg| {
            println!("[MOCK log_info] {}", msg);
            Ok(())
        });

    // 模拟数据库查询返回指定结果
    mock.expect_db_query()
        .returning(|_| Ok(DbResponse {
            success: true,
            dataset: Some(/* 构造测试数据集 */),
            affected_rows: None,
            txn_id: None,
            error: None,
        }));

    // 模拟缓存操作
    mock.expect_cache_get()
        .returning(|key| Ok(CacheResponse {
            success: true,
            value: Some(serde_json::json!("mock_value")),
            exists: Some(true),
            error: None,
        }));

    let core = PluginCore::new(mock);
    // 断点在此，逐步调试业务逻辑
    let result = core.my_function(&input);
}
```

#### 验证 Mock 调用次数

使用 `mockall` 的 `times()` 验证宿主函数是否被按预期调用：

```rust
#[test]
fn test_verify_host_calls() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .times(2)
        .returning(|_| Ok(()));
    mock.expect_db_query()
        .times(1)
        .returning(|_| Ok(DbResponse { success: true, ..Default::default() }));

    let core = PluginCore::new(mock);
    let _ = core.my_function(&input);
    // 测试结束时自动验证调用次数是否符合预期
}
```

### WASM 运行时调试

当需要在真实宿主环境中调试 WASM 插件时：

1. **使用 `debug` 参数**：调用插件或服务编排时设置 `debug: true`，宿主会记录详细的执行日志

```rust
let request = PluginFunRequest {
    plugin_id: "target-plugin".to_string(),
    function_name: "some_function".to_string(),
    input: serde_json::json!({...}),
    initial_input: None,
    debug: Some(true),  // 开启调试模式
};
```

2. **使用宿主日志排查**：在业务逻辑中添加 `host.log_debug()` 输出关键变量值，编译部署后在宿主日志中查看

3. **分步验证**：在服务编排中设置 `debug_node_id`，可在指定节点暂停执行并查看中间状态

```rust
let request = CallServiceRequest {
    service_key: "bm".to_string(),
    input: serde_json::json!({...}),
    include_steps: Some(true),       // 返回每步详情
    debug: Some(true),                // 开启调试模式
    debug_node_id: Some("my_node".to_string()),  // 在指定节点暂停
    debug_params: None,
};
```

---

## 开发注意事项

1. **新增函数必须同时修改三个文件**：`models.rs`（模型）→ `core.rs`（逻辑）→ `extism_layer.rs`（暴露），缺一不可
2. **业务逻辑只写在 `core.rs` 中**，不要在 `extism_layer.rs` 中写业务代码
3. **编译 WASM 必须启用 `extism` feature**：`cargo build --features extism`
4. **单元测试不需要 `extism` feature**，使用 `MockHostFunctions` 即可隔离宿主依赖
5. **事务操作必须传递 `txn_id`**，从 `input.context.txn_id` 获取
6. **获取前序步骤输出**使用 `input.context.get_step_output("node_id")`
7. **SQL 参数化查询**建议使用 `DbRequest.params` 字段传递参数，避免 SQL 注入
