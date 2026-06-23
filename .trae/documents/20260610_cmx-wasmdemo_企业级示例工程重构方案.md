# cmx-wasmdemo 示例工程重构方案

## 摘要

将 cmx-wasmdemo 从当前混乱的"demo"风格重构为企业级业务插件开发的最佳实践示例，以"订单管理"为业务场景，每个宿主函数都有清晰、实用的用例，包含完整的项目目录结构（config/metadata/seeddata/servicedata），支持服务编排，最终可平滑转换为 wasm-plugin-template。

---

## 一、现状分析

### 1.1 当前问题

| 问题 | 说明 |
|------|------|
| **代码组织混乱** | core.rs 将 17 个函数全部平铺，demo 函数与业务逻辑混杂，无分类 |
| **模型不实用** | `DemoRequest { name, count }` 没有业务含义，不适合作为参考 |
| **SQL 注入风险** | 使用 `format!` 拼接 SQL，未使用参数化查询 |
| **HostFunctions trait 不完整** | 缺少 `call_remote_plugin` 和 `call_remote_service` 两个远程调用方法 |
| **缺少项目目录** | 无 config/、metadata/、seeddata/、servicedata/ 目录及示例文件 |
| **错误处理粗糙** | 统一用 `String` 作为错误类型，无结构化错误 |
| **测试覆盖不足** | 仅 8 个基础测试，未覆盖错误场景和远程调用 |
| **函数命名不规范** | `demo_log`、`demo_cache` 等前缀无业务含义 |
| **缓存操作不完整** | 只演示了 set+get，缺少 delete 独立示例 |
| **count_vowels 无实际价值** | 纯字符串处理，不涉及任何宿主函数调用，不适合作为插件示例 |

### 1.2 现有文件清单

```
cmx-wasmdemo/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs          # 模块声明
    ├── models.rs       # 6 个模型（DemoRequest/DemoResponse/RouteInput/InsertData/UpdateData/QueryData/DeleteData）
    ├── host_traits.rs  # 11 个方法的 HostFunctions trait
    ├── core.rs         # 17 个函数的 PluginCore（~300行）
    ├── extism_layer.rs # 17 个 #[plugin_fn] 函数（~250行）
    └── tests.rs        # 8 个测试用例
```

---

## 二、重构方案

### 2.1 业务场景选择：订单管理

选择"订单管理"作为示例业务场景，原因：
- 自然覆盖所有宿主函数（DB 做 CRUD、缓存存状态、日志记审计、插件调用查库存、服务编排处理流程）
- 是常见的企业业务场景，开发者容易理解
- 复杂度适中，既能展示所有能力又不会过于复杂

### 2.2 新目录结构

```
cmx-wasmdemo/
├── manifest.json                          # 插件清单
├── Cargo.toml
├── .cargo/
│   └── config.toml                        # WASM 构建配置
├── config/
│   └── domain_app_module_config.json      # 域/应用/模块配置
├── metadata/
│   └── order_tables.json                  # 订单表结构定义
├── seeddata/
│   └── cmx_order_seed.json                # 订单初始数据
├── servicedata/
│   ├── create_order.json                  # 创建订单流程（含事务）
│   ├── query_order.json                   # 查询订单流程
│   └── process_order.json                 # 订单处理流程（含路由分支+合并+事务）
├── formdata/
│   └── .gitkeep
├── menudata/
│   └── .gitkeep
├── mcpdata/
│   └── .gitkeep
└── src/
    ├── lib.rs
    ├── models.rs                          # 业务模型
    ├── host_traits.rs                     # 13 个方法的 HostFunctions trait
    ├── core.rs                            # 按功能分类的核心逻辑
    ├── extism_layer.rs                    # Extism 适配层
    └── tests.rs                           # 完整测试
```

### 2.3 models.rs 重构

**删除**：`DemoRequest`、`DemoResponse`、`InsertData`、`UpdateData`、`QueryData`、`DeleteData`

**新增**：

```rust
use serde::{Deserialize, Serialize};
pub use cmx_plugin_sdk::{...}; // 保持 SDK 类型重导出

/// 订单状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Processing,
    Shipped,
    Completed,
    Cancelled,
}

/// 订单创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_name: String,
    pub product_name: String,
    pub quantity: u32,
    pub unit_price: f64,
}

/// 订单查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderQueryRequest {
    pub order_id: Option<String>,
    pub customer_name: Option<String>,
    pub status: Option<OrderStatus>,
}

/// 订单更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrderRequest {
    pub order_id: String,
    pub status: Option<OrderStatus>,
    pub quantity: Option<u32>,
}

/// 库存检查请求（用于插件调用示例）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCheckRequest {
    pub product_name: String,
    pub quantity: u32,
}

/// 路由判断输入（服务编排用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInput {
    pub route: String,
}

/// 通用操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
```

### 2.4 host_traits.rs 重构

新增 `call_remote_plugin` 和 `call_remote_service` 两个方法：

```rust
#[cfg_attr(test, automock)]
pub trait HostFunctions {
    // ── 日志 ──
    fn log_info(&self, message: &str) -> Result<(), String>;
    fn log_error(&self, message: &str) -> Result<(), String>;
    fn log_debug(&self, message: &str) -> Result<(), String>;
    fn log_warn(&self, message: &str) -> Result<(), String>;

    // ── 数据库 ──
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String>;
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String>;

    // ── 缓存 ──
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String>;
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String>;
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String>;

    // ── 插件调用 ──
    fn call_plugin(&self, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;
    fn call_remote_plugin(&self, server_name: &str, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;

    // ── 服务编排 ──
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<CallServiceResponse, String>;
    fn call_remote_service(&self, server_name: &str, request: CallServiceRequest) -> Result<CallServiceResponse, String>;
}
```

### 2.5 core.rs 重构

按功能分为 5 大类，每类函数有清晰的注释和业务含义：

#### 类别 1：基础功能（2 个函数）

| 函数 | 宿主函数 | 说明 |
|------|---------|------|
| `greet` | 无 | 简单入参出参示例，返回问候语 |
| `demo_log` | log_info/error/debug/warn | 四级日志使用示例 |

#### 类别 2：缓存操作（3 个函数）

| 函数 | 宿主函数 | 说明 |
|------|---------|------|
| `cache_order_status` | cache_set | 缓存订单状态，演示 cache_set |
| `get_cached_order` | cache_get | 读取缓存中的订单，演示 cache_get |
| `remove_order_cache` | cache_delete | 删除订单缓存，演示 cache_delete |

#### 类别 3：数据库操作（4 个函数）

| 函数 | 宿主函数 | 说明 |
|------|---------|------|
| `query_orders` | db_query | 查询订单列表，演示参数化查询 |
| `create_order` | db_execute | 创建订单，演示 INSERT |
| `update_order` | db_execute | 更新订单状态，演示 UPDATE |
| `delete_order` | db_execute | 删除订单，演示 DELETE |

**关键改进**：所有 SQL 使用参数化查询（`DbRequest.params`），消除 SQL 注入风险。

#### 类别 4：插件间调用（3 个函数）

| 函数 | 宿主函数 | 说明 |
|------|---------|------|
| `check_inventory` | call_plugin | 调用库存插件检查库存 |
| `check_remote_inventory` | call_remote_plugin | 调用远程库存插件 |
| `call_order_service` | call_service_by_key | 调用订单服务编排 |
| `call_remote_order_service` | call_remote_service | 调用远程服务编排 |

#### 类别 5：服务编排函数（7 个函数）

| 函数 | 宿主函数 | 说明 |
|------|---------|------|
| `route_check` | log_info | 路由判断，返回分支标识 |
| `branch_process` | log_info | 分支处理（通用，根据 input.branch 区分） |
| `merge_result` | log_info | 合并各分支结果 |
| `tx_create_order` | db_execute + log_info | 事务内创建订单 |
| `tx_update_stock` | db_execute + log_info | 事务内更新库存 |
| `tx_query_order` | db_query + log_info | 事务内查询订单 |
| `final_process` | cache_set + call_plugin + log_info | 最终处理，整合各步骤输出 |

**关键改进**：
- `branch_process` 合并原来的 branch_1/2/3 为一个通用函数，通过 input 中的 branch 字段区分
- 事务函数使用 `input.context.txn_id` 确保在同一事务中执行
- SQL 使用参数化查询

### 2.6 extism_layer.rs 重构

与 core.rs 对应，每个 `#[plugin_fn]` 函数遵循统一模式：

```rust
#[plugin_fn]
pub fn function_name(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.function_name(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

ExtismHost 实现新增 `call_remote_plugin` 和 `call_remote_service`：

```rust
fn call_remote_plugin(&self, server_name: &str, request: PluginFunRequest) -> Result<PluginFunCallResponse, String> {
    HostCaller::call_remote_plugin(server_name, request).map_err(|e| e.to_string())
}

fn call_remote_service(&self, server_name: &str, request: CallServiceRequest) -> Result<CallServiceResponse, String> {
    HostCaller::call_remote_service(server_name, request).map_err(|e| e.to_string())
}
```

### 2.7 tests.rs 重构

按功能分类组织测试，增加错误场景覆盖：

| 测试 | 覆盖函数 | 说明 |
|------|---------|------|
| `test_greet` | greet | 基础入参出参 |
| `test_demo_log` | demo_log | 日志功能 |
| `test_cache_order_status` | cache_order_status | 缓存写入 |
| `test_get_cached_order` | get_cached_order | 缓存读取 |
| `test_remove_order_cache` | remove_order_cache | 缓存删除 |
| `test_query_orders` | query_orders | 数据库查询 |
| `test_create_order` | create_order | 数据库插入 |
| `test_update_order` | update_order | 数据库更新 |
| `test_delete_order` | delete_order | 数据库删除 |
| `test_check_inventory` | check_inventory | 插件调用 |
| `test_route_check` | route_check | 路由判断 |
| `test_branch_process` | branch_process | 分支处理 |
| `test_merge_result` | merge_result | 结果合并 |
| `test_tx_create_order` | tx_create_order | 事务操作 |
| `test_final_process` | final_process | 最终处理 |

### 2.8 新增项目文件

#### manifest.json

完整的插件清单文件，包含订单管理插件的元数据。

#### config/domain_app_module_config.json

注册表定义和种子数据关系的配置文件。

#### metadata/order_tables.json

订单表结构定义（cmx_order 表），包含列定义、索引、主键等 DDL 元数据。

#### seeddata/cmx_order_seed.json

订单初始数据。

#### servicedata/ 目录

三个服务编排流程定义文件：
- `create_order.json` — 创建订单流程（含事务：创建订单 → 更新库存）
- `query_order.json` — 查询订单流程（查询 → 缓存结果）
- `process_order.json` — 订单处理流程（路由判断 → 分支处理 → 合并 → 事务操作 → 最终处理）

### 2.9 lib.rs 重构

更新模块文档注释，反映新的业务场景和功能分类。

### 2.10 Cargo.toml 更新

- 移除 `path = "../cmx-plugin-sdk"` 本地路径引用，改为与模板一致的 `registry = "nora"` 方式
- 保持其他配置不变

---

## 三、函数对照表（旧 → 新）

| 旧函数 | 新函数 | 变化说明 |
|--------|--------|---------|
| `count_vowels` | `greet` | 替换为有业务含义的简单函数 |
| `demo_log` | `demo_log` | 保留，改进日志消息内容 |
| `demo_cache` | `cache_order_status` + `get_cached_order` + `remove_order_cache` | 拆分为 3 个独立函数，每个演示一个缓存操作 |
| `demo_database` | `query_orders` | 改为有业务含义的查询 |
| — | `create_order` | 新增，演示 INSERT |
| — | `update_order` | 新增，演示 UPDATE |
| — | `delete_order` | 新增，演示 DELETE |
| `demo_call_plugin` | `check_inventory` | 改为有业务含义的插件调用 |
| — | `check_remote_inventory` | 新增，演示远程插件调用 |
| `demo_call_service_by_key` | `call_order_service` | 改为有业务含义的服务编排调用 |
| — | `call_remote_order_service` | 新增，演示远程服务编排调用 |
| `run_all_demos` | 删除 | 无业务价值 |
| `route_check` | `route_check` | 保留 |
| `branch_1_process` | `branch_process` | 合并为通用分支处理函数 |
| `branch_2_process` | `branch_process` | 合并 |
| `branch_3_process` | `branch_process` | 合并 |
| `merge_result` | `merge_result` | 保留，改进实现 |
| `tx_insert` | `tx_create_order` | 改为有业务含义的事务操作 |
| `tx_update` | `tx_update_stock` | 改为有业务含义的事务操作 |
| `tx_query` | `tx_query_order` | 改为有业务含义的事务操作 |
| `tx_delete` | 删除 | 与 delete_order 重复 |
| `final_process` | `final_process` | 保留，简化实现 |

**总计**：17 个旧函数 → 19 个新函数（功能更完整，覆盖所有 13 个宿主方法）

---

## 四、假设与决策

1. **业务场景选择订单管理** — 通用且能覆盖所有宿主函数能力
2. **保持单文件 core.rs 结构** — 与模板兼容，通过注释分区组织
3. **合并 branch_1/2/3 为通用 branch_process** — 减少重复代码，实际业务中分支逻辑通常不同
4. **SQL 使用参数化查询** — 安全最佳实践，通过 `DbRequest.params` 传递参数
5. **HostFunctions trait 增加 2 个远程调用方法** — 与 SDK 的 HostCaller 对齐
6. **Cargo.toml 移除本地 path 引用** — 与模板保持一致
7. **保留 demo_log 命名** — 日志函数本身无业务属性，保留 demo 前缀明确其演示性质

---

## 五、验证步骤

1. `cargo check` — 编译通过
2. `cargo test` — 所有测试通过
3. `cargo build --release --target wasm32-wasip1 --features extism` — WASM 构建通过
4. 检查每个宿主函数至少有一个对应的插件函数用例
5. 检查 servicedata/ 中的流程定义与插件函数对应
6. 检查 SQL 均使用参数化查询
7. 对比模板结构，确认兼容性
