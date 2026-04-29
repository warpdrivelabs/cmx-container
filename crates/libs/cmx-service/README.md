# cmx-service

> 企业级通用服务层，作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker 完成请求处理。

## 项目简介

cmx-service 是 cmx-container 项目的服务编排层，负责服务编排执行、事务管理、服务注册与查询等核心功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-service = "0.1.0"
```

### 核心示例

```rust
use cmx_service::{Orchestrator, ServiceRegistry};
use cmx_core::model::service::ServiceOrchestration;

let orchestrator = Orchestrator::new(
    runtime_invoker,
    plugin_query,
    service_storage,
);
let result = orchestrator.execute(&orchestration, input).await?;
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| Orchestrator | 编排执行器，支持服务编排 JSON、事务框、多分支节点 |
| ServiceRegistry | 服务注册中心，提供服务信息的内存缓存 |
| ServiceRepository | 服务仓储层，提供服务定义的数据库访问 |
| ServiceQueryImpl | ServiceQuery trait 实现（缓存优先） |
| ServiceStorageImpl | ServiceStorage trait 实现 |
| ServiceLifecycleListener | 生命周期监听器，自动同步服务缓存 |

## 服务编排特性

- **线性流程执行**: start -> func -> func -> end
- **事务框支持**: 多个函数在同一个数据库事务中执行
- **多分支路由**: switch 节点根据返回值选择执行路径
- **SVRContext 上下文传递**: 初始入参、请求头、各步骤输出在函数间传递
- **调试模式**: 支持在指定节点处暂停执行

## 模块结构

```
cmx-service
├── src/
│   ├── lib.rs              # 库入口
│   ├── error.rs            # 错误类型定义
│   ├── orchestrator/       # 编排执行器
│   │   ├── executor.rs
│   │   ├── flow_navigator.rs
│   │   ├── mod.rs
│   │   ├── node_handler.rs
│   │   ├── transaction_manager.rs
│   │   └── types.rs
│   ├── registry.rs         # 服务注册中心
│   ├── repository.rs       # 服务仓储层
│   ├── service_query_impl.rs  # 服务查询实现
│   ├── service_storage_impl.rs # 服务存储实现
│   └── lifecycle_listener.rs   # 生命周期监听器
└── Cargo.toml
```

## 使用指南

### 一、编排执行器 (Orchestrator)

#### 1.1 创建编排执行器

```rust
use cmx_service::{Orchestrator, OrchestratorConfig};
use cmx_traits::{RuntimeInvoker, PluginQuery, ServiceStorage};
use std::sync::Arc;

async fn create_orchestrator(
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_storage: Arc<dyn ServiceStorage>,
) -> Orchestrator {
    let config = OrchestratorConfig::default();

    Orchestrator::new(
        runtime,
        plugin_query,
        service_storage,
        config,
    )
}
```

#### 1.2 执行服务编排

```rust
use cmx_core::model::service::ServiceOrchestration;
use serde_json::json;

async fn execute_service(
    orchestrator: &Orchestrator,
    orchestration: &ServiceOrchestration,
    input: serde_json::Value,
) -> Result<serde_json::Value, ServiceError> {
    let result = orchestrator
        .execute(orchestration, input)
        .await?;

    Ok(result)
}

// 示例编排执行
let orchestration = load_orchestration_from_json()?;
let result = orchestrator.execute(&orchestration, json!({
    "user_id": 123,
    "action": "create_order"
})).await?;
```

#### 1.3 调试模式执行

```rust
use cmx_service::{Orchestrator, DebugContext};

async fn debug_execute(
    orchestrator: &Orchestrator,
    orchestration: &ServiceOrchestration,
    input: serde_json::Value,
) -> Result<serde_json::Value, ServiceError> {
    let debug_ctx = DebugContext {
        break_at_nodes: vec!["step_2".to_string(), "step_4".to_string()],
        capture_state: true,
        verbose: true,
    };

    let result = orchestrator
        .execute_with_debug(orchestration, input, debug_ctx)
        .await?;

    Ok(result)
}
```

### 二、服务编排 JSON 结构

#### 2.1 线性流程

```json
{
  "id": "service_001",
  "name": "用户注册服务",
  "version": "1.0.0",
  "nodes": [
    {
      "id": "start",
      "type": "start",
      "next": "validate_input"
    },
    {
      "id": "validate_input",
      "type": "func",
      "plugin": "validator-plugin",
      "function": "validate",
      "next": "create_user"
    },
    {
      "id": "create_user",
      "type": "func",
      "plugin": "user-plugin",
      "function": "create",
      "next": "send_welcome"
    },
    {
      "id": "send_welcome",
      "type": "func",
      "plugin": "notification-plugin",
      "function": "send_email",
      "next": "end"
    },
    {
      "id": "end",
      "type": "end"
    }
  ]
}
```

#### 2.2 多分支流程

```json
{
  "id": "order_processing",
  "name": "订单处理服务",
  "nodes": [
    {"id": "start", "type": "start", "next": "check_stock"},
    {
      "id": "check_stock",
      "type": "func",
      "plugin": "inventory-plugin",
      "function": "check_stock",
      "next": "route_by_stock"
    },
    {
      "id": "route_by_stock",
      "type": "switch",
      "expression": "result.stock_status",
      "cases": {
        "available": "process_order",
        "insufficient": "notify_customer",
        "out_of_stock": "cancel_order"
      },
      "default": "notify_customer"
    },
    {
      "id": "process_order",
      "type": "func",
      "plugin": "order-plugin",
      "function": "process",
      "next": "end"
    },
    {
      "id": "notify_customer",
      "type": "func",
      "plugin": "notification-plugin",
      "function": "notify",
      "next": "end"
    },
    {
      "id": "cancel_order",
      "type": "func",
      "plugin": "order-plugin",
      "function": "cancel",
      "next": "end"
    },
    {"id": "end", "type": "end"}
  ]
}
```

#### 2.3 事务流程

```json
{
  "id": "transfer_service",
  "name": "转账服务",
  "nodes": [
    {"id": "start", "type": "start", "next": "begin_tx"},
    {
      "id": "begin_tx",
      "type": "transaction",
      "transaction_id": "tx_001",
      "steps": [
        {"id": "deduct_source", "type": "func", "plugin": "account-plugin", "function": "deduct"},
        {"id": "add_target", "type": "func", "plugin": "account-plugin", "function": "deposit"},
        {"id": "record_log", "type": "func", "plugin": "log-plugin", "function": "record"}
      ],
      "on_commit": "send_notification",
      "on_rollback": "compensate",
      "next": "end"
    },
    {"id": "send_notification", "type": "func", "plugin": "notification-plugin", "function": "notify", "next": "end"},
    {"id": "compensate", "type": "func", "plugin": "account-plugin", "function": "compensate", "next": "end"},
    {"id": "end", "type": "end"}
  ]
}
```

### 三、服务注册中心 (ServiceRegistry)

#### 3.1 注册服务

```rust
use cmx_service::{ServiceRegistry, ServiceInfo};
use cmx_core::model::service::ServiceOrchestration;

async fn register_service(
    registry: &ServiceRegistry,
    orchestration: ServiceOrchestration,
) -> Result<(), ServiceError> {
    let info = ServiceInfo {
        id: orchestration.id.clone(),
        name: orchestration.name.clone(),
        version: orchestration.version.clone(),
        plugin_id: extract_plugin_id(&orchestration)?,
        functions: extract_functions(&orchestration),
        status: "active".to_string(),
    };

    registry.register(info).await?;
    Ok(())
}
```

#### 3.2 查询服务

```rust
use cmx_service::ServiceRegistry;

async fn find_service(
    registry: &ServiceRegistry,
    service_id: &str,
) -> Result<Option<ServiceInfo>, ServiceError> {
    registry.find_by_id(service_id).await
}

async fn list_by_plugin(
    registry: &ServiceRegistry,
    plugin_id: &str,
) -> Result<Vec<ServiceInfo>, ServiceError> {
    registry.find_by_plugin(plugin_id).await
}

async fn list_all_services(
    registry: &ServiceRegistry,
) -> Result<Vec<ServiceInfo>, ServiceError> {
    registry.list_all().await
}
```

#### 3.3 注销服务

```rust
async fn unregister_service(
    registry: &ServiceRegistry,
    service_id: &str,
) -> Result<(), ServiceError> {
    registry.unregister(service_id).await
}
```

### 四、服务仓储层 (ServiceRepository)

#### 4.1 保存服务定义

```rust
use cmx_service::{ServiceRepository, ServiceDefinition};

async fn save_service(
    repo: &ServiceRepository,
    definition: ServiceDefinition,
) -> Result<(), ServiceError> {
    repo.save(&definition).await
}

async fn update_service(
    repo: &ServiceRepository,
    definition: ServiceDefinition,
) -> Result<(), ServiceError> {
    repo.update(&definition).await
}
```

#### 4.2 删除服务定义

```rust
async fn delete_service(
    repo: &ServiceRepository,
    service_id: &str,
) -> Result<(), ServiceError> {
    repo.delete(service_id).await
}
```

#### 4.3 查询服务定义

```rust
async fn get_service(
    repo: &ServiceRepository,
    service_id: &str,
) -> Result<Option<ServiceDefinition>, ServiceError> {
    repo.find_by_id(service_id).await
}

async fn list_services(
    repo: &ServiceRepository,
    page: u64,
    page_size: u64,
) -> Result<(Vec<ServiceDefinition>, i64), ServiceError> {
    repo.list(page, page_size).await
}
```

### 五、全局单例管理

#### 5.1 设置全局服务查询器

```rust
use cmx_service::{GlobalServiceQuery, ServiceQueryImpl};
use std::sync::Arc;

async fn init_service_query(
    repository: Arc<dyn ServiceRepository>,
    cache: Arc<dyn Cache>,
) -> Result<(), ServiceError> {
    let query = ServiceQueryImpl::new(repository, cache);
    GlobalServiceQuery::set(Arc::new(query)).await?;
    Ok(())
}
```

#### 5.2 获取全局服务查询器

```rust
use cmx_service::GlobalServiceQuery;

async fn use_service_query() -> Result<(), ServiceError> {
    let query = GlobalServiceQuery::get()
        .ok_or_else(|| ServiceError::NotFound("Service query not initialized".to_string()))?;

    let service = query.get_service("service_001").await?;
    Ok(())
}
```

#### 5.3 设置全局服务存储

```rust
use cmx_service::{GlobalServiceStorage, ServiceStorageImpl};

async fn init_service_storage(
    repository: Arc<dyn ServiceRepository>,
) -> Result<(), ServiceError> {
    let storage = ServiceStorageImpl::new(repository);
    GlobalServiceStorage::set(Arc::new(storage)).await?;
    Ok(())
}
```

### 六、生命周期监听

#### 6.1 实现生命周期监听器

```rust
use cmx_service::ServiceLifecycleListener;
use async_trait::async_trait;

struct PluginLifecycleHandler {
    registry: Arc<ServiceRegistry>,
}

#[async_trait]
impl ServiceLifecycleListener for PluginLifecycleHandler {
    async fn on_plugin_activated(&self, plugin_id: &str) {
        tracing::info!("Plugin activated: {}", plugin_id);
        // 刷新相关服务缓存
        self.registry.refresh_by_plugin(plugin_id).await;
    }

    async fn on_plugin_deactivated(&self, plugin_id: &str) {
        tracing::info!("Plugin deactivated: {}", plugin_id);
        // 标记相关服务为不可用
        self.registry.mark_unavailable_by_plugin(plugin_id).await;
    }

    async fn on_plugin_upgraded(&self, plugin_id: &str, new_version: &str) {
        tracing::info!("Plugin upgraded: {} -> {}", plugin_id, new_version);
        // 刷新服务缓存
        self.registry.refresh_by_plugin(plugin_id).await;
    }
}
```

### 七、错误处理

#### 7.1 错误类型

```rust
use cmx_service::ServiceError;

match result {
    Ok(value) => println!("Result: {:?}", value),
    Err(e) => {
        match e {
            ServiceError::NotFound(msg) => {
                eprintln!("Service not found: {}", msg);
            }
            ServiceError::PluginNotFound(plugin_id) => {
                eprintln!("Plugin not found: {}", plugin_id);
            }
            ServiceError::FunctionNotFound(func_name) => {
                eprintln!("Function not found: {}", func_name);
            }
            ServiceError::TransactionFailed(msg) => {
                eprintln!("Transaction failed: {}", msg);
            }
            ServiceError::InvalidOrchestration(msg) => {
                eprintln!("Invalid orchestration: {}", msg);
            }
            ServiceError::ExecutionFailed(msg) => {
                eprintln!("Execution failed: {}", msg);
            }
        }
    }
}
```

#### 7.2 重试机制

```rust
use cmx_service::{Orchestrator, RetryConfig};

let config = OrchestratorConfig::builder()
    .retry_config(RetryConfig {
        max_attempts: 3,
        initial_delay_ms: 100,
        max_delay_ms: 5000,
        backoff_multiplier: 2.0,
    })
    .build();

let orchestrator = Orchestrator::new(runtime, plugin_query, storage, config);
```

### 八、完整示例

```rust
use cmx_service::{
    Orchestrator, ServiceRegistry, ServiceRepository,
    GlobalServiceQuery, GlobalServiceStorage, GlobalServiceRegistry,
    OrchestratorConfig,
};
use cmx_core::model::service::ServiceOrchestration;
use cmx_traits::{RuntimeInvoker, PluginQuery, ServiceStorage};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化各组件
    let runtime: Arc<dyn RuntimeInvoker> = create_runtime()?;
    let plugin_query: Arc<dyn PluginQuery> = create_plugin_query()?;
    let service_storage: Arc<dyn ServiceStorage> = create_service_storage()?;
    let service_repo = create_repository()?;

    // 2. 初始化全局单例
    GlobalServiceStorage::set(service_storage.clone()).await?;
    GlobalServiceRegistry::set(Arc::new(ServiceRegistry::new())).await?;

    let query_impl = ServiceQueryImpl::new(
        service_repo.clone(),
        create_cache()?,
    );
    GlobalServiceQuery::set(Arc::new(query_impl)).await?;

    // 3. 创建编排执行器
    let config = OrchestratorConfig::default();
    let orchestrator = Orchestrator::new(
        runtime,
        plugin_query,
        service_storage,
        config,
    );

    // 4. 加载并执行编排
    let orchestration: ServiceOrchestration = load_orchestration("order_processing.json")?;

    let result = orchestrator.execute(&orchestration, serde_json::json!({
        "order_id": "ORD-12345",
        "user_id": 1001,
        "items": [
            {"product_id": "PROD-001", "quantity": 2},
            {"product_id": "PROD-002", "quantity": 1}
        ]
    })).await?;

    println!("Execution result: {:?}", result);

    Ok(())
}
```
