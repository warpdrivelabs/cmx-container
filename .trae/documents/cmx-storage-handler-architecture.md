# cmx-storage Handler 架构方案 - 方案三

## 方案概述

创建独立的 `cmx-api-types` crate，存放纯**通用类型**（不包含任何业务逻辑），用于统一 HTTP 层接口。各业务模块保留自己的
handler 和业务类型。

## 核心原则

| 类型分类           | 示例                                             | 存放位置                    |
|----------------|------------------------------------------------|-------------------------|
| **通用 HTTP 类型** | ApiResp, Pagination, TreeNode                  | cmx-api-types           |
| **通用请求/文档类型**  | PageParamsDoc, ListParamsDoc, UpdatePayloadDoc | cmx-api-types           |
| **通用错误类型**     | ErrCode, Error (Web 层)                         | cmx-api-types           |
| **路由 Trait**   | ModuleRoutes                                   | cmx-api（依赖 CmxAppState） |
| **业务类型**       | FileInfo, UserEntity                           | 各业务模块                   |
| **Handler**    | storage handler, user handler                  | 各业务模块                   |
| **业务状态**       | CmxAppState                                    | cmx-api                 |

## cmx-api-types 完整类型清单

### 1. api\_response.rs — 统一响应格式

| 类型           | 说明                                  | 当前位置    |
|--------------|-------------------------------------|---------|
| `ApiResp<T>` | 统一响应格式（code, msg, data, pagination） | cmx-api |
| `Pagination` | 分页信息                                | cmx-api |
| `UnitResp`   | 空数据响应别名                             | cmx-api |

### 2. param\_doc.rs — OpenAPI 文档参数类型

| 类型                    | 说明              | 当前位置    |
|-----------------------|-----------------|---------|
| `GetParamsDoc`        | 获取单条记录的查询参数文档   | cmx-api |
| `UpdatePayloadDoc<E>` | 更新请求 Payload 文档 | cmx-api |
| `DeletePayloadDoc`    | 删除请求 Payload 文档 | cmx-api |
| `ListParamsDoc<F>`    | 列表查询参数文档        | cmx-api |
| `PageParamsDoc<F>`    | 分页查询参数文档        | cmx-api |
| `PAGE_SIZE_DEFAULT`   | 分页默认每页条数常量      | cmx-api |
| `PAGE_SIZE_MAX`       | 分页最大每页条数常量      | cmx-api |

### 3. tree.rs — 通用树结构

| 类型                   | 说明          | 当前位置    |
|----------------------|-------------|---------|
| `TreeNode<T>`        | 泛型树节点       | cmx-api |
| `TreeNodeData` trait | 树节点数据 trait | cmx-api |

### 4. error.rs — Web 层通用错误

| 类型          | 说明                        | 当前位置    |
|-------------|---------------------------|---------|
| `ErrCode`   | 错误码枚举                     | cmx-api |
| `Error`     | Web 层错误类型（含 IntoResponse） | cmx-api |
| `Result<T>` | Web 层 Result 别名           | cmx-api |

### 5. 不放入 cmx-api-types 的类型

| 类型                                      | 原因                                 | 保留位置        |
|-----------------------------------------|------------------------------------|-------------|
| `ModuleRoutes` trait                    | 依赖 `CmxAppState`，会循环依赖             | cmx-api     |
| `CmxAppState`                           | 业务状态，包含具体服务引用                      | cmx-api     |
| `AppStateInner`                         | 业务内部状态                             | cmx-api     |
| `header_parse::get_db_id_from_header()` | 依赖 `cmx_database`，非通用              | cmx-api     |
| CRUD handler 函数                         | 依赖 `CmxAppState` + `CmxSvrContext` | cmx-api     |
| CRUD 宏 (`declare_crud_handlers!` 等)     | 依赖 cmx-api 内部类型                    | cmx-api     |
| `FileInfo`, `FileQuery` 等               | 业务类型                               | cmx-storage |
| `PageParams<F>`, `ListParams<F>` 等      | 运行时参数（无 ToSchema），已存在于 cmx-core    | cmx-core    |

### 6. cmx-core 中已有的参数类型（不移动）

cmx-core 中已有 `PageParams<F>`、`ListParams<F>`、`GetParams`、`UpdatePayload<E>`、`DeletePayload`，
这些是**运行时参数**（无 `ToSchema`），与 cmx-api 中的 `*Doc` 版本（有 `ToSchema`）是**配套关系**：

* `cmx-core::PageParams<F>` → 运行时解析用

* `cmx-api-types::PageParamsDoc<F>` → OpenAPI 文档用

## cmx-api-types crate 结构

```
crates/libs/cmx-api-types/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── api_response.rs    # ApiResp<T>, Pagination, UnitResp
    ├── param_doc.rs       # PageParamsDoc, ListParamsDoc, UpdatePayloadDoc, DeletePayloadDoc, GetParamsDoc
    ├── tree.rs            # TreeNode<T>, TreeNodeData trait
    └── error.rs           # ErrCode, Error, Result<T>
```

### Cargo.toml

```toml
[package]
name = "cmx-api-types"
version = "0.1.0"
edition = "2021"

[dependencies]
# 序列化框架
serde = { workspace = true }
# JSON 处理
serde_json = { workspace = true }
# OpenAPI 文档生成
utoipa = { workspace = true, features = ["uuid", "chrono"] }
# HTTP 框架（error.rs 的 IntoResponse 需要）
axum = { workspace = true }
# 参数校验（error.rs 的 ValidationErrors 转换需要）
validator = { workspace = true }
# 错误处理
thiserror = { workspace = true }
# 日志
tracing = { workspace = true }
# modql 依赖（param_doc.rs 的 ListOptions 需要）
modql = { workspace = true }
# 内部依赖 - 核心类型
cmx-core = { workspace = true }
# 内部依赖 - 数据库错误转换
cmx-database = { workspace = true }

[lib]
path = "src/lib.rs"
```

### lib.rs

```rust
pub mod api_response;
pub mod error;
pub mod param_doc;
pub mod tree;

pub use api_response::{ApiResp, Pagination, UnitResp};
pub use error::{ErrCode, Error, Result};
pub use param_doc::*;
pub use tree::{TreeNode, TreeNodeData};
```

## 架构图

```
┌────────────────────────────────────────────────────────────────────────┐
│                           cmx-api-types                                │
│  ┌──────────────────────────────────────────────────────────────────┐ │
│  │  api_response.rs  → ApiResp<T>, Pagination, UnitResp            │ │
│  │  param_doc.rs     → PageParamsDoc, ListParamsDoc, *Doc          │ │
│  │  tree.rs          → TreeNode<T>, TreeNodeData                   │ │
│  │  error.rs         → ErrCode, Error, Result                      │ │
│  └──────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
                              ▲
         ┌────────────────────┼────────────────────┐
         │                    │                    │
┌────────┴────────┐  ┌────────┴────────┐  ┌───────┴────────┐
│    cmx-api      │  │ cmx-storage    │  │  其他业务模块   │
├─────────────────┤  ├────────────────┤  ├───────────────┤
│ CmxAppState     │  │ handler.rs     │  │ handler.rs    │
│ ModuleRoutes    │  │ service.rs     │  │ service.rs    │
│ CRUD 宏/handler │  │ types.rs       │  │ types.rs      │
│ header_parse.rs │  │ (FileInfo等)   │  │ (业务类型)     │
│ 重导出 ApiResp  │  │ 使用 ApiResp   │  │               │
└─────────────────┘  └────────────────┘  └───────────────┘
```

## 详细迁移步骤

### Step 1: 创建 cmx-api-types crate

创建 `crates/libs/cmx-api-types/` 目录和文件

### Step 2: 迁移 api\_response.rs

将 `cmx-api/src/api_response.rs` 内容移动到 `cmx-api-types/src/api_response.rs`

### Step 3: 迁移 param\_doc.rs

将 `cmx-api/src/rest/param_doc.rs` 内容移动到 `cmx-api-types/src/param_doc.rs`

### Step 4: 迁移 tree.rs

将 `cmx-api/src/rest/tree.rs` 内容移动到 `cmx-api-types/src/tree.rs`

### Step 5: 迁移 error.rs

将 `cmx-api/src/error.rs` 内容移动到 `cmx-api-types/src/error.rs`

### Step 6: 更新 workspace Cargo.toml

```toml
members = [
    # ...
    "crates/libs/cmx-api-types",
]

[workspace.dependencies]
cmx-api-types = { path = "crates/libs/cmx-api-types" }
```

### Step 7: 更新 cmx-api

1. 添加依赖 `cmx-api-types = { workspace = true }`
2. 删除 `cmx-api/src/api_response.rs`
3. 删除 `cmx-api/src/error.rs`
4. 删除 `cmx-api/src/rest/param_doc.rs`
5. 删除 `cmx-api/src/rest/tree.rs`
6. 更新 `cmx-api/src/lib.rs`：

```rust
pub use cmx_api_types::{ApiResp, Pagination, UnitResp, ErrCode, Error, Result};
pub use cmx_api_types::{PageParamsDoc, ListParamsDoc, UpdatePayloadDoc, DeletePayloadDoc, GetParamsDoc};
pub use cmx_api_types::{TreeNode, TreeNodeData};
```

1. 更新 `cmx-api/src/rest/mod.rs`：

```rust
pub mod handler;
pub mod header_parse;
// 删除 param_doc 和 tree（已迁移）
pub use cmx_api_types::{ListParamsDoc, PageParamsDoc, GetParamsDoc, UpdatePayloadDoc, DeletePayloadDoc};
pub use cmx_api_types::{TreeNode, TreeNodeData};
pub use handler::{create, create_many, get_by_id, update, update_many, delete, list, page};
```

1. 更新所有 `use crate::api_response::ApiResp` → `use cmx_api_types::ApiResp`
2. 更新所有 `use crate::error::` → `use cmx_api_types::`
3. 更新所有 `use crate::rest::param_doc::` → `use cmx_api_types::`

### Step 8: 更新 cmx-storage

1. 添加依赖 `cmx-api-types = { workspace = true }`
2. 修改 `handler.rs`：

    * 删除本地的 `ApiResponse<T>` 定义

    * 添加 `use cmx_api_types::ApiResp;`

    * 将 `ApiResponse::ok(...)` 替换为 `ApiResp::ok(...)`

    * 将 `error_response` 中的 `code: status.as_u16() as i32` 改为 `status.as_u16()`

    * 将 `message` 字段改为 `msg`

### Step 9: 删除 FromRef 桥接

删除 `cmx-api/src/app_state.rs` 中的 `FromRef<CmxAppState> for cmx_storage::handler::AppState` 实现

### Step 10: 编译验证

```bash
cargo check -p cmx-api-types
cargo check -p cmx-storage
cargo check -p cmx-api
cargo check -p cmx-plugin
cargo check -p web-server
```

## 依赖关系

```
cmx-api-types
    │
    ├──▶ cmx-api (重导出所有通用类型)
    ├──▶ cmx-storage (handler 使用 ApiResp + Error)
    └──▶ (未来) 其他业务模块

cmx-storage
    │
    ├──▶ cmx-api-types (ApiResp, Error)
    ├──▶ cmx-core, cmx-database (业务逻辑)
    └──▶ axum, utoipa (handler)

cmx-api
    │
    ├──▶ cmx-api-types (ApiResp, Error, *Doc, TreeNode)
    ├──▶ cmx-storage (StorageService)
    └──▶ cmx-core, cmx-database (业务逻辑)
```

## 优点

1. **类型统一** - ApiResp、Error 等通用类型集中管理
2. **模块完整** - handler 保留在各业务模块，便于独立测试和维护
3. **职责清晰** - cmx-api 只负责路由聚合和业务状态
4. **无业务侵入** - cmx-api-types 完全不含业务类型
5. **可复用** - 其他模块可直接使用 cmx-api-types 的通用类型
6. **渐进迁移** - ModuleRoutes 保留在 cmx-api，无需移动

## 缺点

1. **新增 crate** - 需要创建 cmx-api-types
2. **handler 保留** - cmx-storage 仍包含 HTTP 层依赖（axum, utoipa）
3. **ModuleRoutes 位置** - 保留在 cmx-api 而非 cmx-api-types（合理的技术约束：依赖 CmxAppState）
4. **迁移工作量** - 需要更新大量 import 路径

## 简化替代方案

如果不想创建新 crate，可以将通用类型合并到 cmx-core：

```
cmx-core/src/
├── api/
│   ├── api_response.rs
│   ├── param_doc.rs
│   ├── tree.rs
│   ├── error.rs
│   └── mod.rs
└── lib.rs
```

这样不需要新增 crate，但 cmx-core 会引入 axum、utoipa 等 HTTP 层依赖，定位不够清晰。

## 建议

**推荐创建独立的 cmx-api-types crate**，理由：

1. 语义更清晰（通用HTTP类型 ≠ 核心业务模型）
2. 依赖更干净（cmx-core 不需要引入 axum/utoipa）
3. 演进独立（HTTP 类型可独立版本迭代）
4. 渐进迁移（ModuleRoutes 保留在 cmx-api 无害）

