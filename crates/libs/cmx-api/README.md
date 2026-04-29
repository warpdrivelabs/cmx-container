# cmx-api

> 提供 Web API 开发所需的基础组件，包括错误处理、响应封装、中间件和通用 CRUD 框架。

## 项目简介

cmx-api 是 cmx-container 项目的 HTTP API 层，基于 Axum 框架构建，提供 RESTful API 路由、请求/响应处理、数据验证和 OpenAPI 文档生成等功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-api = "0.1.0"
```

### 核心示例

```rust
use cmx_api::{ApiResp, CmxAppState, routes};
use axum::{Router, routing::get};

let app = Router::new()
    .route("/api/health", get(|| async { ApiResp::success("OK") }))
    .with_state(app_state);

println!("{}", ApiResp::success("hello"));
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| REST 协议层 | 提供标准 REST 接口封装 |
| CRUD 框架 | 通用增删改查路由自动注册 |
| 业务模型 Handler | 自定义 HTTP Handler 实现 |
| 中间件 | 请求追踪、CORS、Cookie 管理等 |
| 错误处理 | 统一错误类型和响应封装 |
| OpenAPI 文档 | 自动生成 Swagger UI |

## 模块结构

```
cmx-api
├── src/
│   ├── lib.rs              # 库入口
│   ├── api_response.rs     # API 响应封装
│   ├── app_state.rs        # 应用状态管理
│   ├── error.rs            # 错误类型定义
│   ├── middleware/         # 中间件模块
│   │   ├── mod.rs
│   │   ├── mw_context.rs
│   │   ├── mw_cors.rs
│   │   ├── mw_rate_limit.rs
│   │   ├── mw_security_headers.rs
│   │   └── mw_trace.rs
│   ├── handlers/           # 业务模型 Handler
│   │   ├── application/
│   │   ├── debug/
│   │   ├── dev/
│   │   ├── domain/
│   │   ├── module/
│   │   ├── plugin/
│   │   ├── service/
│   │   ├── sys_datasource/
│   │   └── table_metadata/
│   ├── rest/               # REST 协议层
│   │   ├── handler.rs
│   │   ├── header_parse.rs
│   │   ├── mod.rs
│   │   ├── param_doc.rs
│   │   └── tree.rs
│   ├── routes/             # 路由注册
│   │   ├── crud_handlers.rs
│   │   ├── macros.rs
│   │   ├── mod.rs
│   │   ├── routes_impl.rs
│   │   └── traits.rs
│   └── openapi.rs          # OpenAPI 文档
└── Cargo.toml
```

## 主要模块说明

### `rest`

REST 协议层模块，提供 CRUD 操作接口：
- `create`: 创建资源
- `create_many`: 批量创建
- `get_by_id`: 根据 ID 获取
- `update`: 更新资源
- `update_many`: 批量更新
- `delete`: 删除资源
- `list`: 列表查询
- `page`: 分页查询

### `middleware`

提供以下中间件：
- `mw_context_resolver`: 请求上下文解析
- `cors_layer`: CORS 跨域支持
- `mw_trace`: 请求追踪
- `mw_rate_limit`: 限流
- `mw_security_headers`: 安全响应头

### `handlers`

业务 Handler 模块，包含各业务域的 HTTP 处理逻辑。

## 使用指南

### 一、应用状态管理

#### 1.1 定义应用状态

```rust
use cmx_api::{CmxAppState, AppStateInner};
use std::sync::Arc;

#[derive(Clone)]
struct MyService {
    db: DatabasePool,
    cache: RedisClient,
}

type MyAppState = CmxAppState<MyService>;

fn create_app_state() -> MyAppState {
    let inner = AppStateInner {
        service: Arc::new(MyService {
            db: create_db_pool(),
            cache: create_redis_client(),
        }),
        // 其他状态字段...
    };

    CmxAppState::new(inner)
}
```

#### 1.2 在 Handler 中访问状态

```rust
use axum::{extract::State, http::StatusCode};

async fn get_handler(
    State(state): State<MyAppState>,
    Path(id): Path<i64>,
) -> Result<Json<MyEntity>, StatusCode> {
    let entity = state.service.db.find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match entity {
        Some(e) => Ok(Json(e)),
        None => Err(StatusCode::NOT_FOUND),
    }
}
```

### 二、API 响应封装

#### 2.1 成功响应

```rust
use cmx_api::ApiResp;
use serde_json::Value;

// 方式一：直接返回成功
ApiResp::success("操作成功")

// 方式二：返回带数据
ApiResp::success(data)

// 方式三：返回自定义消息
ApiResp::success_with_message(data, "查询成功")

// 响应格式
// {
//     "code": 200,
//     "message": "success",
//     "data": { ... }
// }
```

#### 2.2 分页响应

```rust
use cmx_api::ApiResp;

// 返回分页数据
ApiResp::page(
    data,           // 数据列表
    total,          // 总记录数
    page,           // 当前页码
    page_size,      // 每页大小
)

// 响应格式
// {
//     "code": 200,
//     "message": "success",
//     "data": {
//         "list": [...],
//         "pagination": {
//             "total": 100,
//             "page": 1,
//             "page_size": 10,
//             "total_pages": 10
//         }
//     }
// }
```

#### 2.3 错误响应

```rust
use cmx_api::{ApiResp, ApiError};

// 方式一：使用 ApiError
ApiResp::error(ApiError::not_found("资源不存在"))
ApiResp::error(ApiError::bad_request("参数错误"))
ApiResp::error(ApiError::unauthorized("未授权"))
ApiResp::error(ApiError::internal_error("服务器内部错误"))

// 方式二：自定义错误码和消息
ApiResp::error_with_code(400, "VALIDATION_ERROR", "字段验证失败")

// 响应格式
// {
//     "code": 404,
//     "message": "资源不存在",
//     "error": {
//         "code": "NOT_FOUND",
//         "details": null
//     }
// }
```

### 三、CRUD 路由注册

#### 3.1 定义 Entity Handler

```rust
use cmx_api::{EntityHandler, CrudOperations};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyEntity {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

pub struct MyEntityHandler;

#[async_trait]
impl EntityHandler for MyEntityHandler {
    type Entity = MyEntity;
    type CreateRequest = CreateMyEntity;
    type UpdateRequest = UpdateMyEntity;
    type ListQuery = ListQuery;

    async fn create(&self, data: CreateMyEntity) -> Result<Self::Entity, ApiError> {
        // 创建逻辑
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<Self::Entity>, ApiError> {
        // 查询逻辑
    }

    async fn update(&self, id: i64, data: UpdateMyEntity) -> Result<Self::Entity, ApiError> {
        // 更新逻辑
    }

    async fn delete(&self, id: i64) -> Result<(), ApiError> {
        // 删除逻辑
    }

    async fn list(&self, query: ListQuery) -> Result<Vec<Self::Entity>, ApiError> {
        // 列表查询逻辑
    }

    async fn page(&self, query: ListQuery) -> Result<(Vec<Self::Entity>, i64), ApiError> {
        // 分页查询逻辑，返回 (数据列表, 总数)
    }
}
```

#### 3.2 注册 CRUD 路由

```rust
use cmx_api::{register_crud_routes, CmxAppState};

fn main() {
    let state = create_app_state();
    let handler = MyEntityHandler;

    let app = register_crud_routes!(
        Router::new(),
        "/api/entities",
        handler,
        state
    );
}

// 生成的路由：
// POST   /api/entities          - create
// POST   /api/entities/batch    - create_many
// GET    /api/entities/:id       - get_by_id
// PUT    /api/entities/:id       - update
// PUT    /api/entities/batch     - update_many
// DELETE /api/entities/:id       - delete
// GET    /api/entities          - list
// GET    /api/entities/page     - page
```

### 四、自定义业务路由

#### 4.1 注册业务 Handler

```rust
use axum::{routing::post, Router};

async fn custom_handler(
    State(state): State<MyAppState>,
    Json(payload): Json<CustomRequest>,
) -> Result<Json<ApiResp<CustomResponse>>, StatusCode> {
    // 业务逻辑
    let result = process_custom业务(state.service, payload).await?;

    Ok(Json(ApiResp::success(result)))
}

let app = Router::new()
    .route("/api/entities", post(custom_handler))
    .with_state(state);
```

#### 4.2 路由分组

```rust
use axum::{routing::get, Router};

fn create_entity_routes<S>(service: Arc<S>) -> Router
where
    S: MyService + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list_entities).post(create_entity))
        .route("/:id", get(get_entity).put(update_entity).delete(delete_entity))
        .route("/:id/details", get(get_entity_details))
        .route("/export", post(export_entities))
        .with_state(service)
}
```

### 五、中间件使用

#### 5.1 启用中间件

```rust
use cmx_api::middleware::{
    mw_context_resolver, cors_layer, mw_trace, mw_rate_limit,
};

let app = Router::new()
    .layer(mw_context_resolver())  // 请求上下文解析
    .layer(cors_layer())           // CORS 支持
    .layer(mw_trace())            // 请求追踪
    .layer(mw_rate_limit(100))    // 限流：每分钟 100 请求
    .route("/api/health", get(health_handler));
```

#### 5.2 自定义中间件

```rust
use axum::{
    middleware::Next,
    extract::Request,
    response::Response,
};

async fn custom_middleware(
    request: Request,
    next: Next,
) -> Response {
    let start = std::time::Instant::now();

    // 添加自定义请求处理逻辑
    let response = next.run(request).await;

    // 添加自定义响应处理逻辑
    println!("Request took: {:?}", start.elapsed());

    response
}

let app = Router::new()
    .route_layer(middleware::from_fn(custom_middleware))
    // ...
```

### 六、请求参数解析

#### 6.1 路径参数

```rust
use axum::{routing::get, Router, Path};

async fn get_by_id(
    Path(id): Path<i64>,
) -> Result<Json<MyEntity>, StatusCode> {
    // id 会自动解析为 i64 类型
}

async fn get_by_ids(
    Path(ids): Path<Vec<i64>>,
) -> Result<Json<Vec<MyEntity>>, StatusCode> {
    // 路径: /entities/1,2,3
}

Router::new().route("/entities/:id", get(get_by_id))
```

#### 6.2 查询参数

```rust
use axum::{routing::get, extract::Query};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    page: Option<u64>,
    page_size: Option<u64>,
    #[serde(rename = "name")]
    name_filter: Option<String>,
    status: Option<String>,
}

async fn list_entities(
    Query(query): Query<ListQuery>,
) -> Result<Json<ApiResp<Vec<MyEntity>>>, StatusCode> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(10);
    // ...
}
```

#### 6.3 请求头解析

```rust
use axum::{routing::get, extract::HeaderMap};

async fn get_with_headers(
    headers: HeaderMap,
) -> Result<Json<ApiResp<()>>, StatusCode> {
    // 解析特定请求头
    if let Some(auth) = headers.get("Authorization") {
        println!("Token: {:?}", auth);
    }

    // 获取 X-Request-Id
    let request_id = headers
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok());
}
```

### 七、OpenAPI 文档

#### 7.1 生成 OpenAPI 规范

```rust
use cmx_api::openapi::{OpenApiBuilder, Info, Paths};

let openapi = OpenApiBuilder::new()
    .info(Info {
        title: "My API".to_string(),
        version: "1.0.0".to_string(),
        description: Some("API Documentation".to_string()),
    })
    .paths(build_paths())
    .build();
```

#### 7.2 导出 Swagger UI

```rust
use axum::{routing::get, Router};

async fn swagger_ui() -> impl IntoResponse {
    // 访问 /swagger 显示交互式文档
}

let app = Router::new()
    .route("/swagger", get(swagger_ui))
    .route("/swagger/openapi.json", get(openapi_json));
```

### 八、错误处理

#### 8.1 使用 ApiError

```rust
use cmx_api::{ApiError, ApiResp};

fn handle_result<T>(result: Result<T, MyError>) -> Result<Json<ApiResp<T>>, StatusCode> {
    match result {
        Ok(data) => Ok(Json(ApiResp::success(data))),
        Err(e) => {
            match e {
                MyError::NotFound => Err(StatusCode::NOT_FOUND),
                MyError::Validation(msg) => Ok(Json(ApiResp::error_with_code(
                    400, "VALIDATION_ERROR", &msg
                ))),
                MyError::Unauthorized => Err(StatusCode::UNAUTHORIZED),
                MyError::Internal(msg) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    }
}
```

#### 8.2 全局错误处理

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

impl IntoResponse for MyError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            MyError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", "资源不存在"),
            MyError::Validation(msg) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", &msg),
            MyError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "未授权"),
            MyError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", &msg),
        };

        (status, Json(ApiResp::error_with_code(status.as_u16() as i32, code, message))).into_response()
    }
}
```

### 九、完整示例

```rust
use cmx_api::{
    ApiResp, ApiError, CmxAppState,
    register_crud_routes, EntityHandler,
};
use axum::{routing::get, Router, Json, Path, extract::Query};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserQuery {
    pub page: Option<u64>,
    pub page_size: Option<u64>,
    pub username: Option<String>,
}

pub struct UserHandler;

#[async_trait]
impl EntityHandler for UserHandler {
    type Entity = User;
    type CreateRequest = CreateUser;
    type UpdateRequest = UpdateUser;
    type ListQuery = UserQuery;

    async fn create(&self, data: CreateUser) -> Result<Self::Entity, ApiError> {
        let user = User {
            id: generate_id(),
            username: data.username,
            email: data.email,
        };
        save_user(&user).await?;
        Ok(user)
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<Self::Entity>, ApiError> {
        find_user_by_id(id).await.map_err(|e| ApiError::internal(e.to_string()))
    }

    async fn update(&self, id: i64, data: UpdateUser) -> Result<Self::Entity, ApiError> {
        let mut user = find_user_by_id(id)
            .await?
            .ok_or_else(|| ApiError::not_found("用户不存在"))?;

        if let Some(email) = data.email {
            user.email = email;
        }

        save_user(&user).await?;
        Ok(user)
    }

    async fn delete(&self, id: i64) -> Result<(), ApiError> {
        delete_user(id).await.map_err(|e| ApiError::internal(e.to_string()))
    }

    async fn list(&self, query: UserQuery) -> Result<Vec<Self::Entity>, ApiError> {
        find_users(query).await.map_err(|e| ApiError::internal(e.to_string()))
    }

    async fn page(&self, query: UserQuery) -> Result<(Vec<Self::Entity>, i64), ApiError> {
        let page = query.page.unwrap_or(1) as i64;
        let page_size = query.page_size.unwrap_or(10) as i64;
        find_users_paged(query, page, page_size)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))
    }
}

#[tokio::main]
async fn main() {
    let state = create_app_state();
    let handler = UserHandler;

    let app = register_crud_routes!(
        Router::new(),
        "/api/users",
        handler,
        state
    );

    println!("Server starting on http://0.0.0.0:8080");
}
```
