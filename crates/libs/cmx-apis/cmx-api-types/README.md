# cmx-api-types

> CMX 平台通用 HTTP 类型库，统一 REST API 响应格式、错误处理、OpenAPI 文档参数和树形结构。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]
[![Edition](https://img.shields.io/badge-edition-2024-orange.svg)]

## 快速开始

### 安装

```toml
[dependencies]
cmx-api-types = { workspace = true }
```

### 核心示例

```rust
use cmx_api_types::{ApiResp, Error, Pagination, PageParamsDoc};

// 构建成功响应
let resp = ApiResp::ok("hello");
assert_eq!(resp.code, 0);
assert_eq!(resp.msg, "success");

// 构建分页响应
let resp = ApiResp::ok_with_pagination(vec![1, 2, 3], 1, 20, 100);
assert!(resp.pagination.is_some());

// 构建错误响应
let resp: ApiResp<()> = ApiResp::fail(400, "参数错误");
assert_eq!(resp.code, 400);
```

## 核心功能与特性

| 功能                             | 说明                                        |
|--------------------------------|-------------------------------------------|
| `ApiResp<T>`                   | 统一 REST API 响应格式，支持成功/失败/分页               |
| `Pagination`                   | 分页元信息，支持偏移量计算和前后页判断                       |
| `Error` / `ErrCode`            | Web 层错误类型，自动映射 HTTP 状态码，实现 `IntoResponse` |
| `PageParamsDoc<F>`             | 分页查询 OpenAPI 文档参数                         |
| `ListParamsDoc<F>`             | 列表查询 OpenAPI 文档参数                         |
| `UpdatePayloadDoc<E>`          | 更新请求 OpenAPI 文档参数                         |
| `DeletePayloadDoc`             | 删除请求 OpenAPI 文档参数                         |
| `GetParamsDoc`                 | 获取单条记录 OpenAPI 文档参数                       |
| `TreeNode<T>` / `TreeNodeData` | 泛型树形结构，支持从扁平列表构建树                         |

## 模块结构

```
cmx-api-types
├── api_response    # 统一响应格式（ApiResp, Pagination, UnitResp）
├── error           # Web 层错误类型（Error, ErrCode, Result）
├── param_doc       # OpenAPI 文档参数（PageParamsDoc, ListParamsDoc 等）
└── tree            # 泛型树形结构（TreeNode, TreeNodeData）
```

## 使用指南

### 一、统一响应格式

#### 1.1 构建成功响应

```rust
use cmx_api_types::ApiResp;

// 带数据的成功响应
let resp = ApiResp::ok(user_info);
// {"code":0,"msg":"success","data":{...}}

// 无数据的成功响应
let resp = ApiResp::<() >::ok_no_data();
// {"code":0,"msg":"success"}
```

#### 1.2 构建分页响应

```rust
use cmx_api_types::ApiResp;

let page = 1u64;
let page_size = 20u64;
let total = 100u64;
let resp = ApiResp::ok_with_pagination(user_list, page, page_size, total);
// {"code":0,"msg":"success","data":[...],"pagination":{"page":1,"pageSize":20,"total":100,"totalPages":5}}
```

#### 1.3 构建失败响应

```rust
use cmx_api_types::ApiResp;

let resp: ApiResp<() > = ApiResp::fail(400, "参数校验失败");
// {"code":400,"msg":"参数校验失败"}

// 带数据的失败响应
let resp = ApiResp::fail_with_data(404, "资源不存在", hint_info);
```

#### 1.4 列表响应快捷方法

```rust
use cmx_api_types::ApiResp;

// 非空列表
let resp = ApiResp::list(vec![1, 2, 3]);

// 空列表
let resp = ApiResp::<i32>::empty_list();
```

### 二、分页信息

#### 2.1 构建分页元信息

```rust
use cmx_api_types::Pagination;

let pagination = Pagination::new(1, 20, 100);
assert_eq!(pagination.offset(), 0);
assert!(pagination.has_next());
assert!(!pagination.has_prev());
```

### 三、错误处理

#### 3.1 使用 Error 类型

```rust
use cmx_api_types::{Error, ErrCode, Result};

fn find_user(id: u64) -> Result<User> {
    if id == 0 {
        return Err(Error::bad_request("用户 ID 不能为 0"));
    }
    // 查询用户...
    Ok(User { id, name: "test".into() })
}

// Error 自动实现 IntoResponse，可直接在 axum handler 中返回
```

#### 3.2 错误类型与 HTTP 状态码映射

```rust
use cmx_api_types::{Error, ErrCode};

let err = Error::not_found("用户不存在");
assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);

// BusinessError 特殊：HTTP 200 但 JSON code = 1
let err = Error::business_error("余额不足");
assert_eq!(err.status_code(), axum::http::StatusCode::OK);
```

#### 3.3 限流错误

```rust
use cmx_api_types::Error;

let err = Error::rate_limit_exceeded(30, 100, 60);
let response = err.into_rate_limit_response();
// 响应包含 Retry-After 头
```

### 四、OpenAPI 文档参数

#### 4.1 分页查询参数

```rust
use cmx_api_types::PageParamsDoc;

// PageParamsDoc 用于 utoipa 宏的 request_body
// 运行时使用 cmx_core::PageParams<F> 解析
#[derive(Deserialize)]
struct UserFilter {
    name: Option<String>,
}

let params: PageParamsDoc<UserFilter> = PageParamsDoc {
    filters: None,
    current: Some(2),
    size: Some(50),
    order_bys: None,
};

assert_eq!(params.get_page(), 2);
assert_eq!(params.get_size(), 50);
assert_eq!(params.get_offset(), 50);
```

#### 4.2 列表查询参数

```rust
use cmx_api_types::ListParamsDoc;

let params: ListParamsDoc<UserFilter> = ListParamsDoc {
    filters: None,
    order_bys: Some("-create_time".into()),
};

let list_options = params.to_list_options();
```

#### 4.3 CRUD 文档参数

```rust
use cmx_api_types::{GetParamsDoc, UpdatePayloadDoc, DeletePayloadDoc};
use serde_json::json;

// 获取单条记录
let get_params = GetParamsDoc { id: "123".into() };

// 更新记录
let update_payload = UpdatePayloadDoc {
id: json!("123"),
data: UserForUpdate { name: "李四".into() },
};

// 删除记录
let delete_payload = DeletePayloadDoc {
ids: vec![json!("123"), json!("456")],
};
```

### 五、树形结构

#### 5.1 实现 TreeNodeData trait

```rust
use cmx_api_types::{TreeNode, TreeNodeData};

#[derive(Clone)]
struct Dept {
    id: String,
    parent_id: Option<String>,
    name: String,
    sort_order: i32,
}

impl TreeNodeData for Dept {
    fn node_id(&self) -> &str { &self.id }
    fn parent_id(&self) -> Option<&str> { self.parent_id.as_deref() }
    fn sort_key(&self) -> i32 { self.sort_order }
}
```

#### 5.2 从扁平列表构建树

```rust
let departments = vec![
    Dept { id: "1".into(), parent_id: None, name: "总部".into(), sort_order: 0 },
    Dept { id: "2".into(), parent_id: Some("1".into()), name: "技术部".into(), sort_order: 1 },
    Dept { id: "3".into(), parent_id: Some("1".into()), name: "市场部".into(), sort_order: 2 },
];

let tree = TreeNode::from_list(departments);
// tree[0].data.name == "总部"
// tree[0].children.len() == 2
```

## 常见问题

### Q: ApiResp 的成功码为什么是 0 而不是 200？

**A**: 采用 `code: 0` 表示成功是 CMX 平台的统一规范。HTTP 状态码由 axum 框架自动设置（200 OK），`code` 字段用于业务层区分成功与失败。业务错误使用
`code: 1`（HTTP 200），其他错误码与 HTTP 状态码对齐（如 404、500 等）。

### Q: `*Doc` 类型和 `cmx_core` 中的类型有什么区别？

**A**: `*Doc` 类型（如 `PageParamsDoc`）带有 `#[derive(ToSchema)]`，用于 utoipa 生成 OpenAPI 文档；`cmx_core` 中的类型（如
`PageParams`）是运行时参数解析用的，不带 `ToSchema`。两者是配套关系：handler 函数签名使用 `cmx_core` 类型，utoipa 宏的
`request_body` 使用 `*Doc` 类型。

### Q: Error 类型如何与 axum 集成？

**A**: `Error` 实现了 `axum::response::IntoResponse`，可直接在 handler 返回类型 `Result<T>` 中使用。错误会自动映射为对应的
HTTP 状态码和 JSON 响应体 `{"code": ..., "msg": "..."}`。
