# cmx-api

通用 Web API 开发框架，提供错误处理、响应封装、中间件和通用 CRUD 功能。

## 功能特性

- **通用 CRUD 框架** - 开箱即用的 CRUD 操作
- **过滤和排序** - 支持 modql 的 FilterGroups 和 ListOptions
- **多数据库支持** - 通过 db_id 参数支持多租户场景
- **时间戳处理** - 自动处理 created_at、updated_at 等字段
- **路由注册宏** - 一行代码注册标准 CRUD 路由
- **扩展机制** - 支持自定义 Service 和 Handler

## 快速开始

### 1. 添加依赖

```toml
[dependencies]
cmx-api = { path = "path/to/cmx-api" }
cmx-database = { path = "path/to/cmx-database" }
cmx-core = { path = "path/to/cmx-core" }
modql = { version = "0.4", features = ["with-sea-query"] }
```

### 2. 定义实体

```rust
use cmx_api::DbBmc;
use modql::filter::FilterNodes;
use serde::{Deserialize, Serialize};

// 定义过滤器
#[derive(Debug, Clone, FilterNodes, Serialize, Deserialize)]
pub struct UserFilter {
    pub name: Option<modql::filter::OpValsString>,
    pub email: Option<modql::filter::OpValsString>,
}

// 定义 DbBmc
pub struct UserBmc;

impl DbBmc for UserBmc {
    const TABLE: &'static str = "users";
    const PK_COLUMN: &'static str = "id";
}
```

### 3. 注册路由

```rust
use axum::Router;
use cmx_api::register_crud_routes;
use cmx_database::DatabaseManager;

let router = Router::new().with_state(mm);
let router = register_crud_routes!(router, UserBmc, UserFilter, "/api/users");
```

### 4. API 接口

注册后自动提供以下接口：

| 方法 | 路径 | 说明 |
|-----|------|------|
| POST | `/api/users/create` | 创建用户 |
| GET | `/api/users/get?id=xxx` | 获取用户 |
| POST | `/api/users/update` | 更新用户 |
| GET | `/api/users/delete?id=xxx` | 删除用户 |
| POST | `/api/users/list` | 列表查询 |
| POST | `/api/users/page` | 分页查询 |

## 模块结构

```
cmx-api/
├── src/
│   ├── lib.rs           # 模块入口
│   ├── error.rs         # 错误类型
│   ├── response.rs      # 响应封装
│   ├── crud/            # CRUD 核心
│   │   ├── traits.rs    # DbBmc trait
│   │   ├── service.rs   # GenericCrudService
│   │   ├── macros.rs    # 路由注册宏
│   │   └── utils.rs     # 工具函数
│   ├── rest/            # REST 层
│   │   ├── handler.rs   # Handler 函数
│   │   └── params.rs    # 参数解析
│   ├── middleware/      # 中间件
│   └── models/          # 示例模型
└── guide.md             # 详细指南
```

## 文档

- [guide.md](./guide.md) - 详细使用指南
- [自定义CRUD扩展机制设计.md](../../.trae/documents/自定义CRUD扩展机制设计.md) - 扩展机制说明

## 示例

完整示例参见：`examples/custom-crud/`

## 许可证

MIT
