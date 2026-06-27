# cmx-biz

> cmx 平台业务领域层，承载平台基础业务实体（Domain/Application/Module/SysDatasource）的 Entity/BMC/Filter/Service 定义，以及协议无关的插件函数调用与服务编排执行核心逻辑。

[![Version](https://img.shields.io/badge/version-0.1.11-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-biz` 是 cmx-container 平台的业务领域层，定位为「**模型层 + 协议无关执行核心**」的双重职责 crate。它向上为协议层（`cmx-api` HTTP、`cmx-rpc` gRPC）提供统一的业务实体与执行入口，向下基于基础设施层（`cmx-database`、`cmx-service`、`cmx-traits`、`cmx-core`）完成数据持久化与运行时调用。

### 「模型层 vs 执行逻辑」边界说明

本 crate 同时包含两类内容，这是评估中关注点之一，明确如下：

| 类别 | 模块 | 职责 |
|------|------|------|
| **模型层** | `domain` / `application` / `module` / `datasource` | 平台基础业务实体的 Entity / BMC / Filter / Service 定义，承担 CRUD 与自定义业务方法 |
| **执行逻辑** | `function_invoker` / `service_executor` | 从 HTTP 与 gRPC 两条协议路径中提取的「协议无关」共享执行核心，消除重复代码 |

为什么把执行逻辑放在 `cmx-biz` 而不是协议层？

- 这两段执行链路（插件函数调用、服务编排执行）处理的是统一的业务调用上下文 `SVRContext`，不依赖 HTTP / protobuf 等协议细节；
- 若放在 `cmx-api`，则 `cmx-rpc` 要么重复实现一遍（违反 DRY），要么反向依赖 `cmx-api`（引入 HTTP 概念，破坏分层）；
- 放在 `cmx-biz` 后，`cmx-api` 与 `cmx-rpc` 各自只负责协议适配（参数提取、响应封装），共享的执行核心位于二者之下的业务层，依赖方向单向且无环。

`function_invoker` 同时实现 `cmx_traits::function_invoker::FunctionInvoker` trait（`BizFunctionInvoker`），供组装层（如 `web-server`）构造后以 trait 对象注入 `cmx-rpc`，使 `cmx-rpc` 不必直接依赖 `cmx-biz`。

---

## 快速开始

### 安装

在 `Cargo.toml` 中添加依赖（版本跟随 workspace）：

```toml
[dependencies]
# 内部依赖 - 业务领域层
cmx-biz = { workspace = true }

# 如需 OpenAPI Schema 自动生成（utoipa::ToSchema），启用 openapi feature
# cmx-biz = { workspace = true, features = ["openapi"] }
```

### 核心示例

以下示例展示在协议层 handler 中调用 `cmx-biz` 的 Service 完成一次「查询域-应用-模块树形结构」的业务流程：

```rust
use cmx_biz::domain::{DomainService, DomainTreeNodeData};
use cmx_api_types::TreeNode;
use cmx_database::get_default_db_manager;

async fn get_domain_tree(db_id: &str) -> anyhow::Result<Vec<TreeNode<DomainTreeNodeData>>> {
    // 1. 获取全局数据库管理器
    let mm = get_default_db_manager();

    // 2. 调用 DomainService 自定义方法，执行 tree.sql 并构建三级树
    //    返回 域 → 应用 → 模块 的层级结构
    let tree = DomainService::get_tree(mm, db_id).await?;

    Ok(tree)
}
```

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| Domain（域/租户）管理 | `cmx_domain` 表的 Entity/BMC/Filter/Service，含搜索、树形查询 |
| Application（应用）管理 | `cmx_application` 表的 Entity/BMC/Filter，应用按域归属 |
| Module（模块）管理 | `cmx_module` 表的 Entity/BMC/Filter，模块按应用归属 |
| SysDatasource（数据源）管理 | `cmx_sys_datasource` 表的完整 CRUD，含动态连接池注册/注销、连接测试、字段加密 |
| 域-应用-模块树形查询 | 通过 `tree.sql` 一次查询构建三级树，实现 `TreeNodeData` trait |
| 协议无关插件函数调用 | `invoke_plugin_function` 自由函数 + `BizFunctionInvoker` trait 实现 |
| 协议无关服务编排执行 | `execute_service` 自由函数，封装 Orchestrator 调用与结果映射 |
| StepStatus 字符串互转 | `step_status_to_str` / `parse_step_status`，作为 str↔enum 双向转换的单一来源 |
| OpenAPI Schema 生成 | 可选 `openapi` feature，启用后实体派生 `utoipa::ToSchema` |

### 可选 Features

| Feature | 默认启用 | 说明 |
|---------|---------|------|
| `default` | ✅ | 基础功能（无 OpenAPI） |
| `openapi` | ❌ | 启用 `utoipa::ToSchema` 派生，供 `cmx-api` 自动生成 OpenAPI 文档 |

---

## 模块结构

```text
cmx-biz
├── src
│   ├── lib.rs                  # 模块导出与公共 API re-export（BizError/Result）
│   ├── error.rs                # BizError 错误类型 + From<BizError> for cmx_api_types::Error
│   ├── function_invoker.rs     # 插件函数调用核心逻辑（自由函数 + BizFunctionInvoker）
│   ├── service_executor.rs     # 服务编排执行核心逻辑（execute_service + StepStatus 转换）
│   ├── domain/                 # 域/租户管理模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── entity.rs           #   Domain / DomainForCreate / DomainForUpdate / DomainTreeNodeData
│   │   ├── bmc.rs              #   DomainBmc（cmx_domain 表元信息）
│   │   ├── filter.rs           #   DomainFilter（modql 过滤器）
│   │   ├── service.rs          #   DomainService（search / get_tree）
│   │   └── tree.sql            #   域-应用-模块递归 CTE 查询
│   ├── application/            # 应用管理模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── entity.rs           #   Application / ApplicationForCreate / ApplicationForUpdate
│   │   ├── bmc.rs              #   ApplicationBmc（cmx_application 表元信息）
│   │   ├── filter.rs           #   ApplicationFilter
│   │   └── service.rs          #   占位文件（待迁移）
│   ├── module/                 # 模块管理模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── entity.rs           #   Module / ModuleForCreate / ModuleForUpdate
│   │   ├── bmc.rs              #   ModuleBmc（cmx_module 表元信息）
│   │   ├── filter.rs           #   ModuleFilter
│   │   └── service.rs          #   占位文件（待迁移）
│   └── datasource/             # 系统数据源管理模块
│       ├── mod.rs              #   模块导出
│       ├── entity.rs           #   SysDatasource / SysDatasourceForCreate / SysDatasourceForUpdate
│       ├── bmc.rs              #   SysDatasourceBmc（cmx_sys_datasource 表元信息，含 db_url 加密）
│       ├── filter.rs           #   SysDatasourceFilter
│       └── service.rs          #   SysDatasourceService（CRUD + 动态连接池管理）
└── Cargo.toml
```

### 主要模块说明

#### `domain`

域/租户管理。`Domain` 是平台最高层级业务对象，`Application` 与 `Module` 均通过 `domain_code` 逻辑关联到域。`DomainService::get_tree` 执行 `tree.sql`（递归 CTE）一次查询返回三级扁平数据，再通过 `DomainTreeNodeData`（实现 `TreeNodeData` trait）调用 `TreeNode::from_list()` 构建树形结构。

#### `application` / `module`

应用与模块管理。两者均按「域 → 应用 → 模块」三级层级组织：
- `Application` 通过 `domain_code` 归属域，编码全局唯一（如 FI、CO、MM）
- `Module` 通过 `application_code` 归属应用，编码全局唯一（如 GL、AR、AP）

当前两个模块的 `service.rs` 为占位文件，CRUD 通过 `GenericCrudService::<XxxBmc, XxxFilter>` 在 handler 中直接调用。

#### `datasource`

系统数据源管理。`SysDatasourceBmc` 声明 `db_url` 为加密字段（`encrypted_fields()`），写入时自动加密、读取时自动解密。`SysDatasourceService` 在 CRUD 基础上扩展：
- `create` / `update` / `delete` 在事务内同步 `DatabaseManager` 的连接池注册/注销
- `test_connection` 调用 `DatabaseManager::health_check` 测试连接
- `get_by_db_id` 按 `db_id` 标识查询数据源配置

#### `function_invoker`

插件函数调用核心。提供两个层次的 API：
- 自由函数 `invoke_plugin_function(...)`：完整的协议无关调用链（检查安装 → 加载 WASM → 构建 FunctionInput → rmp-serde 序列化 → 调用 → 反序列化，含 JSON fallback）
- 结构体 `BizFunctionInvoker`：实现 `cmx_traits::function_invoker::FunctionInvoker` trait，持有 `RuntimeInvoker` 与 `PluginQuery`，将 trait 调用委托给自由函数，供组装层注入 `cmx-rpc`

**调用结果处理约定**：基础设施错误（插件未安装、WASM 加载失败、序列化失败）通过 `Err(BizError)` 返回；WASM 函数执行失败通过 `Ok(FunctionInvokeResult { success: false, ... })` 返回，由调用方决定如何映射为协议级错误。

#### `service_executor`

服务编排执行核心。提供自由函数 `execute_service(...)`：构造 `cmx_service::Orchestrator` 并执行服务编排，将 `OrchestrationResult` 转换为协议无关的 `ServiceExecuteCoreResult`（含 `StepStatus` 枚举转字符串）。

同时提供 `step_status_to_str` / `parse_step_status` 一对工具函数，作为 `StepStatus` 枚举与字符串表示双向转换的「单一来源」，避免协议层各自重复 match 逻辑。

---

## 依赖关系

### 上游依赖（cmx-biz 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-core` | 核心模型：`FunctionInput` / `FunctionOutput` / `SVRContext` / `ExecutionStep` / `StepStatus` / `DataSet` / `Row` / `Schema` |
| `cmx-api-types` | 通用 API 类型：`TreeNodeData` trait / `TreeNode` |
| `cmx-traits` | trait 定义：`PluginQuery` / `RuntimeInvoker` / `InvokeOptions` / `ServiceQuery` / `FunctionInvoker` / `TraitError` |
| `cmx-database` | `DatabaseManager` / `GenericCrudService` / `DbBmc` / `DbConfig` / `PoolConfig` / `DbType` / 事务上下文 |
| `cmx-service` | `Orchestrator` / `ExecuteOptions` / `DebugPrepareResult` |
| `serde` / `serde_json` | 序列化框架 |
| `rmp-serde` | MessagePack 序列化（WASM 函数输入/输出编码） |
| `modql` | MongoDB 风格的查询过滤语言（含 sea-query 集成） |
| `sea-query` / `sea-query-sqlx` / `sqlx` | SQL 查询构建与异步执行 |
| `time` / `chrono` | 时间处理 |
| `uuid` | 唯一标识符 |
| `thiserror` | 错误类型派生 |
| `async-trait` | 异步 trait |
| `tracing` | 结构化日志 |
| `utoipa`（可选） | OpenAPI 文档生成 |

### 下游使用方（谁依赖 cmx-biz）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-api` | `cmx-biz = { workspace = true, features = ["openapi"] }` | HTTP handler 直接使用 `DomainService` / `SysDatasourceService` / 各 Entity DTO，调用 `invoke_plugin_function` / `execute_service` 自由函数，并启用 `openapi` 生成 Schema |
| `web-server` | `cmx-biz = { workspace = true }` | 组装层，构造 `BizFunctionInvoker` 实现并以 `Arc<dyn FunctionInvoker>` trait 对象注入 `cmx-rpc`，避免基础设施层直接依赖业务层 |
| `cmx-rpc` | **不直接依赖**（仅依赖 `cmx-traits`） | gRPC 协议层通过 `cmx_traits::function_invoker::FunctionInvoker` trait 接收注入，源码不 `use cmx_biz`，Cargo.toml 也不声明 `cmx-biz`，实现完全解耦 |

### 在整体架构中的位置

```text
┌──────────────────────────────────────────────────────────────┐
│  协议层                                                       │
│  ┌────────────────────┐    ┌────────────────────────────┐    │
│  │ cmx-api (HTTP)     │    │ cmx-rpc (gRPC)             │    │
│  │ - 参数提取          │    │ - protobuf 序列化          │    │
│  │ - 响应封装 (JSON)   │    │ - 响应封装 (protobuf)      │    │
│  └─────────┬──────────┘    └───────────┬────────────────┘    │
│            │                            │                     │
│            │  直接调用 Entity/Service    │ 通过 trait 调用       │
│            │                            │ (FunctionInvoker /  │
│            │                            │  ServiceInvoker)    │
└────────────┼────────────────────────────┼─────────────────────┘
             ▼                            ▼
┌──────────────────────────────────────────────────────────────┐
│  cmx-biz（业务领域层）                                        │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ 模型层：domain / application / module / datasource      │  │
│  │   Entity + BMC + Filter + Service                       │  │
│  ├────────────────────────────────────────────────────────┤  │
│  │ 执行核心：function_invoker / service_executor           │  │
│  │   协议无关的统一调用入口（自由函数 + trait 实现）        │  │
│  └────────────────────────────────────────────────────────┘  │
└────────────┬─────────────────────────────────────────────────┘
             ▼
┌──────────────────────────────────────────────────────────────┐
│  基础设施层                                                   │
│  cmx-database │ cmx-service │ cmx-traits │ cmx-core          │
└──────────────────────────────────────────────────────────────┘
```

---

## 使用指南

### 一、业务实体 CRUD

cmx-biz 的实体 CRUD 通过 `cmx_database::crud::GenericCrudService` 泛型服务完成，调用时传入对应的 `XxxBmc`（表元信息）与可选的 `XxxFilter`（查询过滤器）。

#### 1.1 查询域列表（带过滤与分页）

```rust
use cmx_biz::domain::{DomainBmc, DomainFilter};
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};

async fn list_domains(
    mm: &DatabaseManager,
    db_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 构造过滤器（modql 风格，支持多种操作符）
    let filter = DomainFilter {
        name: Some(OpValsString(vec![OpValString::Contains(
            "财务".to_string(),
        )])),
        status: None,
        archived: None,
        code: None,
        r#type: None,
    };

    // 2. 分页参数
    let list_options = ListOptions {
        limit: Some(20),
        offset: Some(0),
        order_bys: Some("name".into()),
    };

    // 3. 调用 GenericCrudService（无需手写 SQL）
    let (dataset, total) = GenericCrudService::<DomainBmc, DomainFilter>::page(
        mm,
        db_id,
        None,                       // 事务 ID（None 表示无事务）
        Some(vec![filter]),         // 过滤器列表
        list_options,
    )
    .await?;

    println!("命中 {} 条 / 共 {} 条", dataset.row_count(), total);
    Ok(())
}
```

#### 1.2 创建数据源（含动态连接池注册）

`SysDatasourceService::create` 是扩展 `GenericCrudService` 的自定义 Service，在数据库写入后会向 `DatabaseManager` 动态注册连接池。

```rust
use cmx_biz::datasource::{SysDatasourceForCreate, SysDatasourceService};
use cmx_database::DatabaseManager;

async fn create_datasource(
    mm: &DatabaseManager,
    db_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = SysDatasourceForCreate {
        db_id: "tenant_fi".to_string(),
        db_type: "postgres".to_string(),
        db_url: "postgres://user:pass@host:5432/db".to_string(),
        db_schema: Some("public".to_string()),
        default_flag: Some(0),
        status: 1,                       // status=1 触发动态注册连接池
        max_connections: Some(10),
        min_connections: Some(2),
        ..Default::default()
    };

    // 内部流程：
    // 1. 校验 db_type 合法性
    // 2. 开启事务并写入 cmx_sys_datasource（db_url 自动加密）
    // 3. 注册连接池到 DatabaseManager（失败则回滚）
    // 4. 提交事务
    let result = SysDatasourceService::create(mm, db_id, data).await?;

    println!("创建成功，影响 {} 行", result.row_count());
    Ok(())
}
```

#### 1.3 更新数据源（注销旧池 + 注册新池）

```rust
use cmx_biz::datasource::{SysDatasourceForUpdate, SysDatasourceService};
use cmx_database::DatabaseManager;

async fn update_datasource(
    mm: &DatabaseManager,
    db_id: &str,
    id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = SysDatasourceForUpdate {
        db_url: Some("postgres://user:newpass@host:5432/db".to_string()),
        max_connections: Some(20),
        status: 1,
        ..Default::default()
    };

    // 内部流程：
    // 1. 开启事务
    // 2. 读取旧数据，执行数据库更新
    // 3. 注销旧的连接池
    // 4. 按 status 判断是否重新注册：
    //    - status=0（禁用）：仅注销，不重新注册
    //    - status=1（启用）：注销后重新注册，注册失败则回滚
    // 5. 提交事务
    let result = SysDatasourceService::update(mm, db_id, id, data).await?;
    println!("更新成功");
    Ok(())
}
```

#### 1.4 测试数据源连接

```rust
use cmx_biz::datasource::SysDatasourceService;

async fn test_datasource(db_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mm = cmx_database::get_default_db_manager();
    // 调用 DatabaseManager::health_check 验证连接池可用性
    let ok = SysDatasourceService::test_connection(mm, db_id).await?;
    println!("连接测试: {}", if ok { "通过" } else { "失败" });
    Ok(())
}
```

### 二、域-应用-模块树形查询

#### 2.1 获取三级树形结构

```rust
use cmx_biz::domain::DomainService;
use cmx_database::get_default_db_manager;

async fn get_tree(db_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mm = get_default_db_manager();

    // 执行 domain/tree.sql（递归 CTE，仅返回 status=1 且 archived=0 的节点）
    // 然后通过 TreeNodeData trait 调用 TreeNode::from_list() 构建树
    let tree = DomainService::get_tree(mm, db_id).await?;

    for domain in &tree {
        println!("[域] {} - {}", domain.code, domain.name);
        for app in &domain.children {
            println!("  └ [应用] {} - {}", app.code, app.name);
            for module in &app.children {
                println!("      └ [模块] {} - {}", module.code, module.name);
            }
        }
    }
    Ok(())
}
```

#### 2.2 关键字搜索域

```rust
use cmx_biz::domain::DomainService;
use cmx_database::get_default_db_manager;

async fn search_domains(
    keyword: &str,
    page: i64,
    page_size: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mm = get_default_db_manager();

    // 内部构造 DomainFilter（name Contains 操作符）并委托 GenericCrudService::page
    let (dataset, total) =
        DomainService::search(mm, "default", keyword, page, page_size).await?;

    println!("命中 {} 条 / 共 {} 条", dataset.row_count(), total);
    Ok(())
}
```

### 三、协议无关插件函数调用（执行核心）

`function_invoker` 提供两个层次的 API：自由函数（供 `cmx-api` 直接调用）与 trait 实现（供 `cmx-rpc` 通过 trait 对象调用）。

#### 3.1 直接调用自由函数（HTTP 协议层场景）

```rust
use std::sync::Arc;
use cmx_biz::function_invoker::invoke_plugin_function;
use cmx_core::model::service::SVRContext;
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::RuntimeInvoker;
use serde_json::json;

async fn call_plugin(
    runtime: &Arc<dyn RuntimeInvoker>,
    plugin_query: &Arc<dyn PluginQuery>,
) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::new(
        json!({"user_id": "u_001"}),
        std::collections::HashMap::new(),
        chrono::Utc::now(),
        format!("req-{}", uuid::Uuid::new_v4()),
    );

    // 完整调用链：
    // 1. 检查插件安装状态
    // 2. 加载 WASM 模块（未加载时）
    // 3. 构建 FunctionInput（input + svr_ctx）
    // 4. rmp-serde 序列化
    // 5. 调用 runtime.invoke_with_options
    // 6. 反序列化 FunctionOutput（rmp-serde，失败则 JSON fallback）
    let result = invoke_plugin_function(
        runtime,
        plugin_query,
        "billing-calculator",       // plugin_id
        "calculate_invoice",         // function_name
        json!({"amount": 1000}),     // 当前步骤输入
        None,                        // initial_input（None 时用 input 代替）
        svr_ctx,
        false,                       // debug
    )
    .await?;

    // 调用结果处理约定：
    // - 基础设施错误（插件未安装、WASM 加载失败、序列化失败）走 Err 分支
    // - WASM 函数执行失败走 Ok 分支，由调用方决定如何映射为协议响应
    if !result.success {
        let error_msg = result.error.unwrap_or_else(|| "未知错误".to_string());
        return Err(error_msg.into());
    }

    println!("调用成功，耗时 {} μs，结果: {:?}", result.elapsed_us, result.result);
    Ok(())
}
```

#### 3.2 通过 trait 对象调用（gRPC 协议层 / 组装层注入场景）

`BizFunctionInvoker` 实现 `cmx_traits::function_invoker::FunctionInvoker` trait，由组装层（`web-server`）构造后以 `Arc<dyn FunctionInvoker>` 注入 `cmx-rpc`，使 `cmx-rpc` 不必直接依赖 `cmx-biz`。

**组装层注入示例（参考 `web-server/src/main.rs`）：**

```rust
use std::sync::Arc;
use cmx_biz::function_invoker::BizFunctionInvoker;
use cmx_traits::function_invoker::FunctionInvoker;

// 在 web-server 组装层构造实现：
// - 从 cmx-runtime 获取 RuntimeInvoker（WASM 引擎）
// - 从 cmx-plugin  获取 PluginQuery（插件元数据查询）
// 注：cmx-biz 不直接依赖 cmx-runtime / cmx-plugin，运行时依赖通过 trait 注入
let function_invoker: Arc<dyn FunctionInvoker> = Arc::new(
    BizFunctionInvoker::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
    ),
);

// 把 trait 对象透传给 cmx-rpc 的初始化函数：
// init_rpc(service_invoker, function_invoker, plugin_data_importer).await?;
```

**消费方（cmx-rpc）使用方式：**

```rust
use std::sync::Arc;
use cmx_traits::function_invoker::FunctionInvoker;
use cmx_traits::error::TraitError;

async fn grpc_call_plugin(
    invoker: &Arc<dyn FunctionInvoker>,   // 由组装层注入
    plugin_id: &str,
    function_name: &str,
) -> Result<(), TraitError> {
    // cmx-rpc 源码不出现 `use cmx_biz::...`，仅依赖 cmx-traits 的 trait
    let result = invoker
        .invoke_plugin_function(
            plugin_id,
            function_name,
            serde_json::Value::Null,
            None,
            svr_ctx,
            false,
        )
        .await?;
    Ok(())
}
```

### 四、协议无关服务编排执行（执行核心）

`service_executor::execute_service` 是从 HTTP 路径提取的协议无关执行核心，封装 `cmx_service::Orchestrator` 的构造与结果映射。

#### 4.1 执行服务编排

```rust
use std::sync::Arc;
use cmx_biz::service_executor::{execute_service, ServiceExecuteCoreResult};
use cmx_core::model::service::SVRContext;
use cmx_service::ExecuteOptions;
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::ServiceQuery;
use serde_json::json;

async fn run_orchestration(
    runtime: &Arc<dyn RuntimeInvoker>,
    plugin_query: &Arc<dyn PluginQuery>,
    service_query: &Arc<dyn ServiceQuery>,
    default_db_id: &str,
    service_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::new(
        json!({"user_id": "u_001"}),
        std::collections::HashMap::new(),
        chrono::Utc::now(),
        format!("req-{}", uuid::Uuid::new_v4()),
    );

    // 控制是否返回 steps 数据、是否启用调试暂停
    let options = ExecuteOptions {
        return_steps: true,
        debug: false,
        ..Default::default()
    };

    // 完整调用链：
    // 1. 构造 Orchestrator（runtime + plugin_query + service_query + default_db_id）
    // 2. orchestrator.execute_service(service_key, svr_ctx, options)
    // 3. 将 OrchestrationResult 映射为 ServiceExecuteCoreResult（StepStatus 转字符串）
    let core_result: ServiceExecuteCoreResult = execute_service(
        runtime,
        plugin_query,
        service_query,
        default_db_id,
        service_key,
        svr_ctx,
        options,
    )
    .await?;

    // 结果处理约定：
    // - Err(BizError)：基础设施错误（服务未找到、编排配置缺失、内部错误）
    // - Ok(success=false)：编排执行完成但有节点失败
    // - Ok(success=true)：编排执行成功
    if !core_result.success {
        let msg = core_result
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| "未知编排错误".to_string());
        return Err(msg.into());
    }

    println!("编排成功，总耗时 {} μs", core_result.total_elapsed_us);
    for step in &core_result.steps {
        println!("  - [{}] {} -> {}", step.node_type, step.node_name, step.status);
    }
    Ok(())
}
```

#### 4.2 StepStatus 字符串互转（协议层共享工具）

```rust
use cmx_biz::service_executor::{step_status_to_str, parse_step_status};
use cmx_core::StepStatus;

// 枚举转字符串（用于持久化或返回给前端）
let s: &str = step_status_to_str(&StepStatus::Success); // "Success"

// 字符串转枚举（用于从配置或请求中解析）
let status: Option<StepStatus> = parse_step_status("Failed");
```

`step_status_to_str` / `parse_step_status` 实现已迁移至 `cmx_traits::step_status`，本 crate 仅通过 `pub use` 重导出以保持 `cmx_biz::service_executor::*` 路径向后兼容。

### 五、错误处理

`BizError` 是 cmx-biz 的统一错误类型，覆盖数据库、业务、插件、编排、序列化等场景。

#### 5.1 错误类型一览

```rust
use cmx_biz::BizError;

// 各错误变体对应的场景：
// - BizError::Crud(ServiceError)     数据库 CRUD 操作错误（自动 From）
// - BizError::Database(String)       数据库管理错误（连接池注册/注销等）
// - BizError::Business(String)        业务逻辑错误（数据校验失败、不支持的 db_type 等）
// - BizError::NotFound(String)       数据未找到（插件未安装、实体不存在等）
// - BizError::SerdeJson(Error)        JSON 序列化错误（自动 From）
// - BizError::PluginInvoke(String)   插件函数调用错误
// - BizError::Orchestration(String)   服务编排错误
// - BizError::Internal(String)        内部错误（tree.sql 查询失败、必填字段缺失等）

// 便捷构造器：
let _e1 = BizError::business("不支持的数据库类型: redis");
let _e2 = BizError::not_found("插件 billing-calculator 未安装");
let _e3 = BizError::internal("缺少必填字段: code");
```

#### 5.2 在协议层传播错误

`BizError` 实现了 `From<BizError> for cmx_api_types::Error`，使 `cmx-api` handler 中可使用 `?` 操作符直接传播业务层错误：

```rust
use cmx_biz::domain::DomainService;
use cmx_api_types::Error; // handler 的错误类型

async fn handler() -> Result<(), Error> {
    let mm = cmx_database::get_default_db_manager();
    // DomainService::get_tree 返回 cmx_biz::Result<_>
    // 通过 From<BizError> for cmx_api_types::Error 自动转换
    let _tree = DomainService::get_tree(mm, "default").await?;
    Ok(())
}
```

#### 5.3 在 trait 实现中映射错误

`BizError` 同时实现了 `From<BizError> for cmx_traits::error::TraitError`，使 `BizFunctionInvoker` 实现 `FunctionInvoker` trait 时可将基础设施错误统一映射为抽象层错误：

```rust
// BizFunctionInvoker::invoke_plugin_function 内部：
// invoke_plugin_function(...).await
//     .map_err(cmx_traits::error::TraitError::from)   // BizError -> TraitError
// 映射规则：
// - BizError::Business(msg)        -> TraitError::Business(msg)
// - BizError::NotFound(msg)        -> TraitError::NotFound(msg)
// - BizError::PluginInvoke(msg)    -> TraitError::WasmInvokeFailed(msg)
// - BizError::Orchestration(msg)    -> TraitError::OrchestrationFailed(msg)
// - BizError::Crud / Database / SerdeJson / Internal -> TraitError::Internal(...)
```

---

## 关键设计决策

### 1. 为什么 cmx-biz 同时是「模型层」又是「执行核心」？

`function_invoker` 与 `service_executor` 处理的是统一的业务调用上下文 `SVRContext`，不依赖 HTTP / protobuf 等协议细节。如果放在 `cmx-api`：

- `cmx-rpc` 要么重复实现一遍（违反 DRY），要么反向依赖 `cmx-api`（引入 HTTP 概念，破坏分层）；
- 放在 `cmx-biz` 后，`cmx-api` 与 `cmx-rpc` 各自只负责协议适配（参数提取、响应封装），共享的执行核心位于二者之下的业务层，依赖方向单向且无环。

### 2. 为什么 `BizFunctionInvoker` 不直接放在 `cmx-rpc`？

`BizFunctionInvoker` 依赖 `RuntimeInvoker`（来自 `cmx-runtime`）与 `PluginQuery`（来自 `cmx-plugin`）。若放在 `cmx-rpc`，则基础设施层 `cmx-rpc` 要反向依赖 `cmx-runtime` / `cmx-plugin` 等业务实现层，破坏分层。

正确做法是把「业务实现」放在 `cmx-biz`，把「trait 定义」放在 `cmx-traits`，把「组装」放在 `web-server`：

```text
web-server (组装层)
   │  构造 BizFunctionInvoker（依赖 cmx-runtime / cmx-plugin / cmx-biz）
   ▼
Arc<dyn FunctionInvoker>  ← trait 定义在 cmx-traits
   │
   ▼
cmx-rpc (协议层，仅依赖 cmx-traits)
```

### 3. 为什么自由函数与 trait 实现并存？

- **自由函数 `invoke_plugin_function(...)`**：供 `cmx-api` HTTP handler 直接调用，签名接受 `&Arc<dyn RuntimeInvoker>` 等具体参数，便于 handler 显式控制。
- **trait 实现 `BizFunctionInvoker`**：供 `cmx-rpc` 通过 `Arc<dyn FunctionInvoker>` 注入调用，把运行时依赖隐藏在结构体字段后。
- 二者共享同一份调用链实现：`BizFunctionInvoker::invoke_plugin_function` 内部直接委托给同名自由函数，避免重复实现。

### 4. 为什么「WASM 函数执行失败」走 `Ok` 而非 `Err`？

约定：
- `Err(BizError)` 表示**基础设施错误**（插件未安装、WASM 加载失败、序列化失败、网络异常等），调用方无法恢复；
- `Ok(FunctionInvokeResult { success: false, ... })` 表示**编排执行完成但函数返回失败**，调用方可以决定如何映射为协议级响应（HTTP 422 业务错误、gRPC 业务 status code 等）。

这种区分让协议层拥有错误映射的最终决定权，避免在业务核心层硬编码协议语义。

### 5. 为什么 `application` / `module` 的 `service.rs` 是占位文件？

当前两个模块的 CRUD 通过 `GenericCrudService::<XxxBmc, XxxFilter>` 在 handler 中直接调用，不需要自定义 Service。`service.rs` 保留为占位文件是为了未来迁移自定义业务方法时无需修改模块结构。

---

## 常见问题

### Q1: `cmx-biz` 与 `cmx-service` 的边界是什么？

**A**: `cmx-service` 提供 `Orchestrator`（服务编排引擎）及其执行模型，是纯执行器；`cmx-biz::service_executor` 在其之上包装「协议无关的执行入口」，负责构造 `Orchestrator` 并把 `OrchestrationResult` 映射为 `ServiceExecuteCoreResult`（含 `StepStatus` 转字符串）。简言之：`cmx-service` 提供「引擎」，`cmx-biz` 提供「业务调用入口」。

### Q2: `FunctionInvokeResult` 类型定义在哪里？

**A**: 定义在 `cmx_traits::function_invoker`，`cmx-biz` 通过 `pub use cmx_traits::function_invoker::FunctionInvokeResult;` 重导出，保持 `cmx_biz::function_invoker::FunctionInvokeResult` 路径向后兼容。同理 `step_status_to_str` / `parse_step_status` 实现位于 `cmx_traits::step_status`，本 crate 仅做重导出。

### Q3: 为什么 `cmx-rpc` 不在 `Cargo.toml` 中声明 `cmx-biz`？

**A**: 因为 `cmx-rpc` 源码不直接 `use cmx_biz::...`，它只通过 `cmx_traits::function_invoker::FunctionInvoker` trait 接收实现注入。组装层（`web-server`）负责构造 `BizFunctionInvoker` 并以 `Arc<dyn FunctionInvoker>` 透传给 `cmx-rpc` 的初始化函数。这样 `cmx-rpc` 的依赖图保持精简，不引入业务层概念。

### Q4: 启用 `openapi` feature 后有什么变化？

**A**: 所有 `Entity` / `ForCreate` / `ForUpdate` / `TreeNodeData` 结构体派生 `utoipa::ToSchema`，可在 `cmx-api` 中通过 `#[dependencies]` 或 `#[to_schema]` 引用来自动生成 OpenAPI 文档。未启用 `openapi` 时，这些派生宏不生效，编译产物更小。

### Q5: 数据源 `db_url` 是如何加密的？

**A**: `SysDatasourceBmc::encrypted_fields()` 返回 `&["db_url"]`，`GenericCrudService` 在写入时自动加密、读取时自动解密。加密密钥与算法由 `cmx-database` 基础设施层提供，业务层只声明「哪些字段需要加密」。

### Q6: `invoke_plugin_function` 反序列化失败时为什么有 JSON fallback？

**A**: WASM 函数可能直接返回 JSON 字节流而非 MessagePack 编码。`rmp_serde::from_slice` 失败时记录 `tracing::warn` 并尝试 `serde_json::from_slice`，最终 fallback 到 `Value::Null`。这种容错避免因编码不匹配导致整个调用链失败。