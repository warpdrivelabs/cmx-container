# cmx-plugin-demo 企业级插件示例工程创建方案

## 摘要

新建 `cmx-plugin-demo` crate 作为企业级业务插件开发的最佳实践示例，以"订单管理"为业务场景，每个宿主函数都有清晰、实用的用例，包含完整的项目目录结构（config/metadata/seeddata/servicedata），支持服务编排，最终可平滑转换为 wasm-plugin-template。不修改现有 cmx-wasmdemo。

***

## 一、现状分析

### 1.1 cmx-wasmdemo 存在的问题

| 问题                          | 说明                                                       |
| --------------------------- | -------------------------------------------------------- |
| **代码组织混乱**                  | core.rs 将 17 个函数全部平铺，demo 函数与业务逻辑混杂，无分类                  |
| **模型不实用**                   | `DemoRequest { name, count }` 没有业务含义，不适合作为参考             |
| **SQL 注入风险**                | 使用 `format!` 拼接 SQL，未使用参数化查询                             |
| **HostFunctions trait 不完整** | 缺少 `call_remote_plugin` 和 `call_remote_service` 两个远程调用方法 |
| **缺少项目目录**                  | 无 config/、metadata/、seeddata/、servicedata/ 目录及示例文件       |
| **错误处理粗糙**                  | 统一用 `String` 作为错误类型，无结构化错误                               |
| **测试覆盖不足**                  | 仅 8 个基础测试，未覆盖错误场景和远程调用                                   |
| **函数命名不规范**                 | `demo_log`、`demo_cache` 等前缀无业务含义                         |
| **缓存操作不完整**                 | 只演示了 set+get，缺少 delete 独立示例                              |
| **src/ 结构扁平**               | 所有代码平铺在单文件中，不适合企业级复杂业务插件的扩展                              |
| **文件命名不够语义化**               | `host_traits.rs`、`core.rs`、`extism_layer.rs` 命名不够清晰      |

### 1.2 决策

新建 `cmx-plugin-demo` crate，不修改现有 `cmx-wasmdemo`。

***

## 二、新工程方案

### 2.1 业务场景：订单管理

选择"订单管理"作为示例业务场景，原因：

* 自然覆盖所有宿主函数（DB 做 CRUD、缓存存状态、日志记审计、插件调用查库存、服务编排处理流程）

* 是常见的企业业务场景，开发者容易理解

* 复杂度适中，既能展示所有能力又不会过于复杂

### 2.2 完整目录结构

```
crates/libs/cmx-plugin-demo/
├── manifest.json                              # 插件清单
├── Cargo.toml
├── .cargo/
│   └── config.toml                            # WASM 构建配置
├── config/
│   └── domain_app_module_config.json          # 域/应用/模块配置
├── metadata/
│   └── order_tables.json                      # 订单表结构定义
├── seeddata/
│   └── cmx_order_seed.json                    # 订单初始数据
├── servicedata/
│   ├── create_order.json                      # 创建订单流程（含事务）
│   ├── query_order.json                       # 查询订单流程
│   └── process_order.json                     # 订单处理流程（含路由分支+合并+事务）
├── formdata/
│   └── .gitkeep
├── menudata/
│   └── .gitkeep
├── mcpdata/
│   └── .gitkeep
└── src/
    ├── lib.rs                                 # 模块声明与 crate 文档
    ├── host.rs                                # HostFunctions trait（替代 host_traits.rs）
    ├── models/                                # 业务模型目录
    │   ├── mod.rs                             # 模块导出 + SDK 类型重导出
    │   ├── order.rs                           # 订单相关模型
    │   └── common.rs                          # 通用模型（RouteInput, OperationResult 等）
    ├── handlers/                              # 业务处理逻辑（替代 core.rs）
    │   ├── mod.rs                             # PluginCore 定义 + new()
    │   ├── basic.rs                           # 基础功能（greet, demo_log）
    │   ├── cache.rs                           # 缓存操作（cache_order_status, get_cached_order, remove_order_cache）
    │   ├── database.rs                        # 数据库操作（query_orders, create_order, update_order, delete_order）
    │   ├── plugin_call.rs                     # 插件调用（check_inventory, check_remote_inventory, call_order_service, call_remote_order_service）
    │   └── orchestration.rs                   # 服务编排（route_check, branch_process, merge_result, tx_create_order, tx_update_stock, final_process）
    ├── extism/                                # Extism 适配层（替代 extism_layer.rs）
    │   ├── mod.rs                             # ExtismHost 实现 + 模块导出
    │   ├── basic.rs                           # greet, demo_log
    │   ├── cache.rs                           # 缓存操作 plugin_fn
    │   ├── database.rs                        # 数据库操作 plugin_fn
    │   ├── plugin_call.rs                     # 插件调用 plugin_fn
    │   └── orchestration.rs                   # 服务编排 plugin_fn
    └── tests/                                 # 测试
        ├── mod.rs                             # 公共测试工具（make_input 等）
        ├── basic.rs                           # 基础功能测试
        ├── cache.rs                           # 缓存测试
        ├── database.rs                        # 数据库测试
        ├── plugin_call.rs                     # 插件调用测试
        └── orchestration.rs                   # 服务编排测试
```

### 2.3 文件命名优化说明

| 旧命名               | 新命名            | 优化理由                                          |
| ----------------- | -------------- | --------------------------------------------- |
| `host_traits.rs`  | `host.rs`      | 更简洁，"traits" 后缀冗余（Rust 惯例：trait 名即文件名）        |
| `core.rs`         | `handlers/` 目录 | "core" 语义模糊，"handlers" 明确表达"处理请求"的职责，且支持按功能拆分 |
| `extism_layer.rs` | `extism/` 目录   | "layer" 后缀冗余，目录形式支持按功能拆分                      |
| `models.rs`       | `models/` 目录   | 企业级插件模型多，按实体拆分更清晰                             |
| `tests.rs`        | `tests/` 目录    | 按功能分类测试，便于维护和扩展                               |

### 2.4 models/ 目录

#### models/mod.rs

```rust
//! 业务模型定义。
//!
//! 按实体组织模型，每个文件对应一个业务领域。

// 重导出 SDK 核心类型，方便业务代码直接 use crate::models::*
pub use cmx_plugin_sdk::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, PluginFunCallResponse, CallServiceRequest, CallServiceResponse,
};

pub mod order;
pub mod common;

pub use order::*;
pub use common::*;
```

#### models/order.rs

```rust
use serde::{Deserialize, Serialize};

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
```

#### models/common.rs

```rust
use serde::{Deserialize, Serialize};

/// 路由判断输入（服务编排用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInput {
    pub route: String,
}

/// 库存检查请求（用于插件调用示例）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCheckRequest {
    pub product_name: String,
    pub quantity: u32,
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

### 2.5 host.rs

```rust
use crate::models::*;
#[cfg(test)]
use mockall::automock;

/// 宿主功能 trait。
///
/// 定义了 WASM 插件可调用的全部宿主能力，包括日志、数据库、
/// 缓存、插件调用（本地+远程）和服务编排（本地+远程）。
#[cfg_attr(test, automock)]
pub trait HostFunctions {
    // ── 日志 ──────────────────────────────────────
    fn log_info(&self, message: &str) -> Result<(), String>;
    fn log_error(&self, message: &str) -> Result<(), String>;
    fn log_debug(&self, message: &str) -> Result<(), String>;
    fn log_warn(&self, message: &str) -> Result<(), String>;

    // ── 数据库 ────────────────────────────────────
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String>;
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String>;

    // ── 缓存 ──────────────────────────────────────
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String>;
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String>;
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String>;

    // ── 插件调用 ──────────────────────────────────
    fn call_plugin(&self, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;
    fn call_remote_plugin(&self, server_name: &str, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;

    // ── 服务编排 ──────────────────────────────────
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<CallServiceResponse, String>;
    fn call_remote_service(&self, server_name: &str, request: CallServiceRequest) -> Result<CallServiceResponse, String>;
}
```

### 2.6 handlers/ 目录

#### handlers/mod.rs

```rust
//! 业务处理逻辑。
//!
//! 按功能分类组织，每个文件对应一类宿主能力的使用示例。

use crate::host::HostFunctions;

/// 插件核心实现。
///
/// 通过泛型 `H: HostFunctions` 与具体宿主环境解耦，
/// 支持在测试中使用 MockHostFunctions。
pub struct PluginCore<H: HostFunctions> {
    host: H,
}

impl<H: HostFunctions> PluginCore<H> {
    /// 创建新的插件核心实例。
    pub fn new(host: H) -> Self {
        Self { host }
    }

    /// 获取宿主功能的引用。
    pub fn host(&self) -> &H {
        &self.host
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod plugin_call;
pub mod orchestration;
```

#### handlers/basic.rs

```rust
use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 问候函数。
    ///
    /// 简单入参出参示例，不依赖任何宿主函数，
    /// 适合作为第一个插件函数的参考。
    pub fn greet(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let name = input.input.as_str().unwrap_or("World");
        let result = serde_json::json!({
            "message": format!("Hello, {}!", name),
            "greeting": format!("Welcome to cmx-plugin-demo, {}!", name),
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 日志功能演示。
    ///
    /// 演示四级日志（info/error/debug/warn）的使用方式。
    pub fn demo_log(&self, _input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("[订单插件] 信息日志示例")?;
        self.host.log_error("[订单插件] 错误日志示例")?;
        self.host.log_debug("[订单插件] 调试日志示例")?;
        self.host.log_warn("[订单插件] 警告日志示例")?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": true,
            "message": "四级日志记录完成",
        })))
    }
}
```

#### handlers/cache.rs

```rust
use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 缓存订单状态（演示 cache_set）。
    pub fn cache_order_status(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let cache_key = format!("order:status:{}", request.order_id);
        let status_value = serde_json::json!({
            "order_id": request.order_id,
            "status": request.status,
        });
        self.host.cache_set(&cache_key, status_value, Some(3600))?;
        self.host.log_info(&format!("订单状态已缓存: {}", cache_key))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": true,
            "message": format!("订单 {} 状态已缓存", request.order_id),
        })))
    }

    /// 读取缓存的订单（演示 cache_get）。
    pub fn get_cached_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let cache_key = format!("order:status:{}", order_id);
        let response = self.host.cache_get(&cache_key)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": response.success,
            "cache_key": cache_key,
            "value": response.value,
            "exists": response.exists,
        })))
    }

    /// 删除订单缓存（演示 cache_delete）。
    pub fn remove_order_cache(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let cache_key = format!("order:status:{}", order_id);
        let response = self.host.cache_delete(&cache_key)?;
        self.host.log_info(&format!("订单缓存已删除: {}", cache_key))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": response.success,
            "cache_key": cache_key,
        })))
    }
}
```

#### handlers/database.rs

```rust
use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 查询订单列表（演示 db_query + 参数化查询）。
    pub fn query_orders(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: OrderQueryRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(OrderQueryRequest { order_id: None, customer_name: None, status: None });
        let mut sql = "SELECT id, customer_name, product_name, quantity, status FROM cmx_order WHERE 1=1".to_string();
        let mut params = Vec::new();
        if let Some(ref order_id) = request.order_id {
            sql.push_str(" AND id = ?");
            params.push(serde_json::json!(order_id));
        }
        if let Some(ref customer_name) = request.customer_name {
            sql.push_str(" AND customer_name = ?");
            params.push(serde_json::json!(customer_name));
        }
        if let Some(ref status) = request.status {
            sql.push_str(" AND status = ?");
            params.push(serde_json::json!(status));
        }
        let db_request = DbRequest {
            sql,
            params: if params.is_empty() { None } else { Some(params) },
            dataset_id: None,
            db_id: None,
            txn_id: None,
        };
        let db_response = self.host.db_query(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "dataset": db_response.dataset,
        })))
    }

    /// 创建订单（演示 db_execute + INSERT + 参数化查询）。
    pub fn create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let sql = "INSERT INTO cmx_order (customer_name, product_name, quantity, unit_price, status) VALUES (?, ?, ?, ?, 'pending')".to_string();
        let params = vec![
            serde_json::json!(request.customer_name),
            serde_json::json!(request.product_name),
            serde_json::json!(request.quantity),
            serde_json::json!(request.unit_price),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
        };
        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!("订单创建成功, 影响行数: {:?}", db_response.affected_rows))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }

    /// 更新订单状态（演示 db_execute + UPDATE + 参数化查询）。
    pub fn update_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let sql = "UPDATE cmx_order SET status = ? WHERE id = ?".to_string();
        let params = vec![
            serde_json::json!(request.status),
            serde_json::json!(request.order_id),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }

    /// 删除订单（演示 db_execute + DELETE + 参数化查询）。
    pub fn delete_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let sql = "DELETE FROM cmx_order WHERE id = ?".to_string();
        let params = vec![serde_json::json!(order_id)];
        let db_request = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
        };
        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!("订单已删除: {}", order_id))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
```

#### handlers/plugin\_call.rs

```rust
use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 调用库存插件检查库存（演示 call_plugin）。
    pub fn check_inventory(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: InventoryCheckRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let plugin_request = PluginFunRequest {
            plugin_id: "inventory-plugin".to_string(),
            function_name: "check_stock".to_string(),
            input: serde_json::json!({
                "product_name": request.product_name,
                "quantity": request.quantity,
            }),
            initial_input: None,
            debug: Some(false),
            server_name: None,
        };
        match self.host.call_plugin(plugin_request) {
            Ok(result) => {
                self.host.log_info("库存检查完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "inventory_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("库存检查失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("库存检查失败: {}", e),
                })))
            }
        }
    }

    /// 调用远程库存插件（演示 call_remote_plugin）。
    pub fn check_remote_inventory(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: InventoryCheckRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let plugin_request = PluginFunRequest {
            plugin_id: "inventory-plugin".to_string(),
            function_name: "check_stock".to_string(),
            input: serde_json::json!({
                "product_name": request.product_name,
                "quantity": request.quantity,
            }),
            initial_input: None,
            debug: Some(false),
            server_name: None,
        };
        match self.host.call_remote_plugin("remote-server", plugin_request) {
            Ok(result) => {
                self.host.log_info("远程库存检查完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "remote_inventory_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("远程库存检查失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("远程库存检查失败: {}", e),
                })))
            }
        }
    }

    /// 调用订单服务编排（演示 call_service_by_key）。
    pub fn call_order_service(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let service_request = CallServiceRequest {
            service_key: "order_service".to_string(),
            input: input.input.clone(),
            include_steps: Some(true),
            debug: Some(false),
            debug_node_id: None,
            debug_params: None,
            server_name: None,
        };
        match self.host.call_service_by_key(service_request) {
            Ok(result) => {
                self.host.log_info("订单服务编排调用完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "service_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("订单服务编排调用失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("服务编排调用失败: {}", e),
                })))
            }
        }
    }

    /// 调用远程服务编排（演示 call_remote_service）。
    pub fn call_remote_order_service(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let service_request = CallServiceRequest {
            service_key: "order_service".to_string(),
            input: input.input.clone(),
            include_steps: Some(true),
            debug: Some(false),
            debug_node_id: None,
            debug_params: None,
            server_name: None,
        };
        match self.host.call_remote_service("remote-server", service_request) {
            Ok(result) => {
                self.host.log_info("远程订单服务编排调用完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "remote_service_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("远程服务编排调用失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("远程服务编排调用失败: {}", e),
                })))
            }
        }
    }
}
```

#### handlers/orchestration.rs

```rust
use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 路由判断函数（服务编排用）。
    ///
    /// 根据输入的 route 字段决定返回哪个分支标识。
    #[doc_type = "branch_fn"]
    pub fn route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let route_input: RouteInput = serde_json::from_value(input.input.clone())
            .unwrap_or(RouteInput { route: "1".to_string() });
        let route = route_input.route.trim();
        let result = match route {
            "1" | "2" | "3" => route,
            _ => "1",
        };
        self.host.log_info(&format!("路由判断: route={}, 分支={}", route, result))?;
        Ok(FunctionOutput::from_json(serde_json::to_value(result).map_err(|e| e.to_string())?))
    }

    /// 通用分支处理函数（服务编排用）。
    ///
    /// 根据 input 中的 branch 字段区分处理逻辑。
    pub fn branch_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let branch = input.input.get("branch")
            .and_then(|v| v.as_str())
            .unwrap_or("1");
        self.host.log_info(&format!("执行分支{}处理", branch))?;
        let result = serde_json::json!({
            "branch": branch,
            "message": format!("分支{}处理完成", branch),
            "input": input.input,
            "initial_input": input.context.initial_input,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 合并结果函数（服务编排用）。
    ///
    /// 从上下文中获取各分支的输出并合并。
    pub fn merge_result(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行结果合并")?;
        let branch_output = input.context.get_step_output("branch_process")
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let result = serde_json::json!({
            "merged": true,
            "branch_output": branch_output,
            "message": "结果合并完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务内创建订单（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    pub fn tx_create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务创建订单, txn_id={:?}", txn_id))?;
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let sql = "INSERT INTO cmx_order (customer_name, product_name, quantity, unit_price, status) VALUES (?, ?, ?, ?, 'pending')".to_string();
        let params = vec![
            serde_json::json!(request.customer_name),
            serde_json::json!(request.product_name),
            serde_json::json!(request.quantity),
            serde_json::json!(request.unit_price),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "operation": "tx_create_order",
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务创建订单完成",
        })))
    }

    /// 事务内更新库存（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    pub fn tx_update_stock(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务更新库存, txn_id={:?}", txn_id))?;
        let product_name = input.input.get("product_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let quantity = input.input.get("quantity")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let sql = "UPDATE cmx_inventory SET stock = stock - ? WHERE product_name = ?".to_string();
        let params = vec![
            serde_json::json!(quantity),
            serde_json::json!(product_name),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "operation": "tx_update_stock",
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务更新库存完成",
        })))
    }

    /// 最终处理函数（服务编排用）。
    ///
    /// 整合各步骤的输出，演示多宿主函数组合使用。
    pub fn final_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行最终处理")?;
        let merge_output = input.context.get_step_output("merge_result")
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let tx_create_output = input.context.get_step_output("tx_create_order").cloned();
        let tx_stock_output = input.context.get_step_output("tx_update_stock").cloned();
        // 缓存最终结果
        self.host.cache_set(
            "order:final_result",
            serde_json::json!({"processed": true}),
            Some(3600),
        )?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "final": true,
            "merge_output": merge_output,
            "tx_create_output": tx_create_output,
            "tx_stock_output": tx_stock_output,
            "txn_id": input.context.txn_id,
            "message": "服务编排执行完成",
        })))
    }
}
```

### 2.7 extism/ 目录

#### extism/mod.rs

```rust
//! Extism 适配层。
//!
//! 将 HostCaller 的静态方法委托为 HostFunctions trait 实现，
//! 并通过 #[plugin_fn] 宏暴露插件函数。

use crate::host::HostFunctions;
use crate::models::*;
use cmx_plugin_sdk::HostCaller;

struct ExtismHost;

impl HostFunctions for ExtismHost {
    fn log_info(&self, message: &str) -> Result<(), String> {
        HostCaller::log_info(message).map_err(|e| e.to_string())
    }
    fn log_error(&self, message: &str) -> Result<(), String> {
        HostCaller::log_error(message).map_err(|e| e.to_string())
    }
    fn log_debug(&self, message: &str) -> Result<(), String> {
        HostCaller::log_debug(message).map_err(|e| e.to_string())
    }
    fn log_warn(&self, message: &str) -> Result<(), String> {
        HostCaller::log_warn(message).map_err(|e| e.to_string())
    }
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_query(request).map_err(|e| e.to_string())
    }
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_execute(request).map_err(|e| e.to_string())
    }
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_get(key).map_err(|e| e.to_string())
    }
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String> {
        HostCaller::cache_set(key, value, ttl_seconds).map_err(|e| e.to_string())
    }
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_delete(key).map_err(|e| e.to_string())
    }
    fn call_plugin(&self, request: PluginFunRequest) -> Result<PluginFunCallResponse, String> {
        HostCaller::call_plugin(request).map_err(|e| e.to_string())
    }
    fn call_remote_plugin(&self, server_name: &str, request: PluginFunRequest) -> Result<PluginFunCallResponse, String> {
        HostCaller::call_remote_plugin(server_name, request).map_err(|e| e.to_string())
    }
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<CallServiceResponse, String> {
        HostCaller::call_service_by_key(request).map_err(|e| e.to_string())
    }
    fn call_remote_service(&self, server_name: &str, request: CallServiceRequest) -> Result<CallServiceResponse, String> {
        HostCaller::call_remote_service(server_name, request).map_err(|e| e.to_string())
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod plugin_call;
pub mod orchestration;
```

#### extism/basic.rs

```rust
use crate::handlers::PluginCore;
use crate::extism::ExtismHost;
use cmx_plugin_sdk::*;
use extism_pdk::*;

/// 问候函数
///
/// 简单入参出参示例，不依赖任何宿主函数。
///
/// # Arguments
///
/// * `input` - `string` 名称。
///
/// # Returns
///
/// 返回包含问候语的 `FunctionOutput`。
#[plugin_fn]
pub fn greet(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.greet(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 日志功能演示
///
/// 演示四级日志（info/error/debug/warn）的使用方式。
#[plugin_fn]
pub fn demo_log(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_log(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

#### extism/cache.rs / database.rs / plugin\_call.rs / orchestration.rs

每个文件结构与 basic.rs 相同，包含对应 handlers 中函数的 `#[plugin_fn]` 包装。
每个函数遵循统一 3 行模式：

```rust
#[plugin_fn]
pub fn function_name(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.function_name(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

### 2.8 tests/ 目录

#### tests/mod.rs

```rust
use crate::host::MockHostFunctions;
use crate::models::*;
use cmx_plugin_sdk::{FunctionInput, SVRContext, DbResponse, CacheResponse};
use std::collections::HashMap;

/// 创建测试用 FunctionInput。
pub fn make_input(input_value: serde_json::Value) -> FunctionInput {
    FunctionInput {
        input: input_value,
        context: SVRContext::new(
            serde_json::Value::Null,
            HashMap::new(),
            chrono::Utc::now(),
            "test-request-id".to_string(),
        ),
        binary_data: HashMap::new(),
    }
}

/// 创建带上下文步骤输出的测试用 FunctionInput。
pub fn make_input_with_steps(input_value: serde_json::Value, steps: Vec<(&str, serde_json::Value)>) -> FunctionInput {
    let mut context = SVRContext::new(
        serde_json::Value::Null,
        HashMap::new(),
        chrono::Utc::now(),
        "test-request-id".to_string(),
    );
    for (key, value) in steps {
        context.add_step_output(key.to_string(), value);
    }
    FunctionInput {
        input: input_value,
        context,
        binary_data: HashMap::new(),
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod plugin_call;
pub mod orchestration;
```

#### tests/basic.rs, cache.rs, database.rs, plugin\_call.rs, orchestration.rs

每个文件包含对应功能的测试用例，使用 `MockHostFunctions` 进行单元测试。

### 2.9 lib.rs

```rust
//! cmx-plugin-demo — 企业级 WASM 插件开发最佳实践示例
//!
//! 以"订单管理"为业务场景，演示所有宿主函数的使用方式：
//!
//! - **基础功能**：`greet`（简单入参出参）、`demo_log`（四级日志）
//! - **缓存操作**：`cache_order_status`、`get_cached_order`、`remove_order_cache`
//! - **数据库操作**：`query_orders`、`create_order`、`update_order`、`delete_order`
//! - **插件调用**：`check_inventory`、`check_remote_inventory`、`call_order_service`、`call_remote_order_service`
//! - **服务编排**：`route_check`、`branch_process`、`merge_result`、`tx_create_order`、`tx_update_stock`、`final_process`
//!
//! # 架构模式
//!
//! 采用三层分离模式：
//! - `handlers/` — 纯业务逻辑，通过泛型 `H: HostFunctions` 与宿主解耦
//! - `host.rs` — 抽象接口，定义宿主能力 trait
//! - `extism/` — Extism 适配层，将 HostCaller 委托为 HostFunctions 实现

pub mod models;
pub mod host;
pub mod handlers;

#[cfg(test)]
pub mod tests;

#[cfg(feature = "extism")]
pub mod extism;
```

### 2.10 Cargo.toml

```toml
[package]
name = "cmx-plugin-demo"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# Extism PDK
extism-pdk = { version = "1.4.1", optional = true }
# CMX 插件 SDK
cmx-plugin-sdk = { version = "0.1.8", registry = "nora", default-features = false }
# 序列化框架
serde = { version = "1.0", features = ["derive"] }
# JSON 序列化/反序列化
serde_json = "1.0"
# 时间处理
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
# Mock 测试框架
mockall = "0.11"

[features]
# Extism 特性 — 编译 Wasm 时开启
extism = ["extism-pdk", "cmx-plugin-sdk/extism"]

[profile.release]
lto = true
opt-level = "s"

[profile.dev]
debug = 2
opt-level = 0
lto = false
strip = false
```

### 2.11 workspace Cargo.toml 更新

在 workspace 的 `members` 中添加 `"crates/libs/cmx-plugin-demo"`。

***

## 三、函数对照表（cmx-wasmdemo → cmx-plugin-demo）

| 旧函数                        | 新函数                                                              | 变化说明               |
| -------------------------- | ---------------------------------------------------------------- | ------------------ |
| `count_vowels`             | `greet`                                                          | 替换为有业务含义的简单函数      |
| `demo_log`                 | `demo_log`                                                       | 保留，改进日志消息内容        |
| `demo_cache`               | `cache_order_status` + `get_cached_order` + `remove_order_cache` | 拆分为 3 个独立函数        |
| `demo_database`            | `query_orders`                                                   | 改为有业务含义的查询         |
| —                          | `create_order`                                                   | 新增，演示 INSERT       |
| —                          | `update_order`                                                   | 新增，演示 UPDATE       |
| —                          | `delete_order`                                                   | 新增，演示 DELETE       |
| `demo_call_plugin`         | `check_inventory`                                                | 改为有业务含义的插件调用       |
| —                          | `check_remote_inventory`                                         | 新增，演示远程插件调用        |
| `demo_call_service_by_key` | `call_order_service`                                             | 改为有业务含义的服务编排调用     |
| —                          | `call_remote_order_service`                                      | 新增，演示远程服务编排调用      |
| `run_all_demos`            | 删除                                                               | 无业务价值              |
| `route_check`              | `route_check`                                                    | 保留                 |
| `branch_1_process`         | `branch_process`                                                 | 合并为通用分支处理函数        |
| `branch_2_process`         | `branch_process`                                                 | 合并                 |
| `branch_3_process`         | `branch_process`                                                 | 合并                 |
| `merge_result`             | `merge_result`                                                   | 保留，改进实现            |
| `tx_insert`                | `tx_create_order`                                                | 改为有业务含义的事务操作       |
| `tx_update`                | `tx_update_stock`                                                | 改为有业务含义的事务操作       |
| `tx_query`                 | 删除                                                               | 与 query\_orders 重复 |
| `tx_delete`                | 删除                                                               | 与 delete\_order 重复 |
| `final_process`            | `final_process`                                                  | 保留，简化实现            |

**总计**：17 个旧函数 → 19 个新函数（功能更完整，覆盖所有 13 个宿主方法）

***

## 四、假设与决策

1. **新建 crate 而非修改 wasmdemo** — 用户明确要求
2. **业务场景选择订单管理** — 通用且能覆盖所有宿主函数能力
3. **src/ 全目录拆分** — models/handlers/extism/tests 均按功能拆分为目录，适合企业级复杂插件
4. **文件命名优化** — host\_traits→host, core→handlers, extism\_layer→extism
5. **合并 branch\_1/2/3 为通用 branch\_process** — 减少重复代码
6. **SQL 使用参数化查询** — 安全最佳实践
7. **HostFunctions trait 增加 2 个远程调用方法** — 与 SDK 的 HostCaller 对齐
8. **Cargo.toml 使用 registry 引用** — 与模板保持一致

***

## 五、实施步骤

1. 创建目录结构（src/ 及其子目录、config/metadata/seeddata/servicedata/ 等）
2. 编写 Cargo.toml
3. 编写 src/lib.rs
4. 编写 src/host.rs
5. 编写 src/models/ 目录（mod.rs, order.rs, common.rs）
6. 编写 src/handlers/ 目录（mod.rs, basic.rs, cache.rs, database.rs, plugin\_call.rs, orchestration.rs）
7. 编写 src/extism/ 目录（mod.rs, basic.rs, cache.rs, database.rs, plugin\_call.rs, orchestration.rs）
8. 编写 src/tests/ 目录（mod.rs, basic.rs, cache.rs, database.rs, plugin\_call.rs, orchestration.rs）
9. 编写 manifest.json
10. 编写 .cargo/config.toml
11. 编写 config/domain\_app\_module\_config.json
12. 编写 metadata/order\_tables.json
13. 编写 seeddata/cmx\_order\_seed.json
14. 编写 servicedata/ 下 3 个流程定义文件
15. 创建 formdata/menudata/mcpdata/.gitkeep
16. 更新 workspace Cargo.toml 添加新 member
17. `cargo check` 验证编译
18. `cargo test` 验证测试

***

## 六、验证步骤

1. `cargo check` — 编译通过
2. `cargo test` — 所有测试通过
3. `cargo build --release --target wasm32-wasip1 --features extism` — WASM 构建通过
4. 检查每个宿主函数至少有一个对应的插件函数用例
5. 检查 servicedata/ 中的流程定义与插件函数对应
6. 检查 SQL 均使用参数化查询
7. 对比模板结构，确认兼容性

