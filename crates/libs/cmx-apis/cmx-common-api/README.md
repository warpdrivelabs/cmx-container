# cmx-common-api

> 通用 API 层与装配中枢（原 `cmx-api` 重命名）：re-export `cmx-api-core` 共享骨架，保留 service / debug / portal / dev 四组 handler 与路由聚合、OpenAPI 文档。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]
[![Edition](https://img.shields.io/badge-edition-2024-orange.svg)]

## 项目简介

`cmx-common-api` 是 `cmx-apis/` crate 家族中的「跨域通用 + 装配中枢」成员。2026-07/08 的
「cmx-api 依赖与 handler 重构」把原大 `cmx-api` 拆为皮肤 crate 族：共享骨架下沉
`cmx-api-core`，各业务域 handler 迁出到自己的 `cmx-*-api` crate；本 crate 保留跨域通用
handler、路由聚合入口与 OpenAPI 文档切片，并通过 re-export 保持旧 `cmx_api::xxx` 路径兼容
（`cmx_common_api::CmxAppState`、`cmx_common_api::rest::handler::create` 等仍可用）。

## cmx-apis crate 家族

| crate | 职责 |
|-------|------|
| `cmx-api-core` | 共享骨架：CmxAppState / ModuleRoutes / rest / middleware / CRUD 宏 |
| `cmx-api-types` | 通用类型：ApiResp / Error / `*Doc` 参数 / TreeNode（叶子 crate） |
| `cmx-common-api`（本 crate） | 跨域通用 handler（service/debug/portal/dev）+ 路由聚合 + OpenAPI |
| `cmx-biz-api` | 业务域：Domain / Application / Menu / SysDatasource / Form / Module CRUD |
| `cmx-iam-api` | 认证与 IAM：Auth / User / Role / Permission / API Key / OAuth2 |
| `cmx-plugin-api` | 插件：插件管理 / 表元数据 / 插件市场 / Module 包 |
| `cmx-storage-api` | 文件存储 |
| `cmx-ai-api` | AI 中继（薄 HTTP 皮肤，委托 `cmx-ai` crate） |

## 快速开始

### 安装

```toml
[dependencies]
cmx-common-api = { workspace = true }
```

### 核心示例

```rust
use cmx_common_api::{routes::routes_impl::{api_routes, swagger_routes}, CmxAppState};

// 通用路由（service/debug/portal + /health）
let router = api_routes().with_state(CmxAppState::default());

// Swagger UI（挂载后访问 /swagger-ui/）
let swagger = swagger_routes();
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 共享骨架 re-export | `CmxAppState` / `ModuleRoutes` / `rest` / `middleware` / CRUD 宏（来自 cmx-api-core） |
| 通用类型 re-export | `ApiResp` / `Error` / `Pagination` / `*Doc` / `TreeNode`（来自 cmx-api-types） |
| service handler | WASM 插件服务调用与元数据管理（`/api/service/*`） |
| debug handler | 插件调试会话查询（`/api/debug/*`） |
| portal handler | 门户/设计器业务（迁移自 Node 后端，40+ 端点） |
| dev handler | 开发脚手架（`dev-tools` feature，仅单节点 dev） |
| 路由聚合 | `api_routes()` 统一注册 + `swagger_routes()` + `/health` 健康检查 |
| OpenAPI 文档 | `ApiDoc`（service）+ `PortalApiDoc`（portal 切片） |

## 模块结构

```
cmx-common-api
├── src/
│   ├── lib.rs              # 库入口：re-export cmx-api-core 骨架 + cmx-api-types 类型
│   ├── handlers/           # 剩余跨域通用 handler
│   │   ├── service/        # 服务调用（handler.rs + models.rs）
│   │   ├── debug/          # 插件调试会话（handler.rs + response.rs；request.rs 为未挂载遗留文件）
│   │   ├── portal/         # 门户/设计器业务（ai/data/launcher/legacy/meta/notify/pages/registry）
│   │   └── dev/            # 开发脚手架（#[cfg(feature = "dev-tools")]）
│   ├── routes/
│   │   ├── mod.rs          # re-export cmx-api-core 的 routes::{macros, traits}
│   │   └── routes_impl.rs  # api_routes() 聚合 + swagger_routes() + health_check
│   └── openapi.rs          # ApiDoc + PortalApiDoc
└── Cargo.toml              # feature: dev-tools（默认关闭）
```

## 主要模块说明

### `handlers` —— 通用业务 Handler

- `service`：WASM 插件服务调用 `/api/service/*`（详见「五、service 模块端点」）。
- `debug`：插件调试会话状态查询（断点/上下文/调用栈）：

  | 方法 | 路径 | 作用 |
  |------|------|------|
  | GET | `/api/debug/current` | 查询当前用户的插件调试会话状态 |
- `portal`：门户/设计器业务（迁移自 CMXPortalManager / CMXHTMLDesigner 的 Node 后端），
  路由路径与 Node 后端保持一致，响应统一 `ApiResp` 信封（详见「六、portal 模块端点」）。
- `dev`：开发脚手架（项目模板生成，本地 fs 写入/解压 zip，违反集群无状态约束，仅
  `dev-tools` feature 启用时注册并打印告警）。启用方式：

  ```toml
  [dependencies]
  cmx-common-api = { workspace = true, features = ["dev-tools"] }
  ```

  启用后启动时打印告警：`dev-tools feature 已启用：开发脚手架端点暴露，仅限单节点 dev，
  不可水平扩展！`。`legacy` 子模块为门户旧接口兼容层。

### `routes` —— 路由聚合

`routes_impl::api_routes()` 依次 merge service / debug / portal（及 dev-tools 下的 dev）
并挂 `/health` 健康检查（无需认证，供 Docker HEALTHCHECK 与负载均衡器使用）；
`swagger_routes()` 挂 Swagger UI；`health_check()` 为独立可复用的健康检查 handler。

### `openapi` —— 文档切片

- `ApiDoc`：service 域路径与 schemas（FunctionCallRequest/Response、ServiceExecute* 等）。
- `PortalApiDoc`：门户切片，不带 `info`（切片惯例），统一 tag「门户接口」，由
  cmx-platform-app `OpenApi::merge()` 合并进主文档；独立门户微服务（cmx-portalservice）
  复用同一装配核，Swagger 同样可见。

## 使用指南

### 一、应用状态管理

`CmxAppState` 定义于 `cmx-api-core`（本 crate re-export），通过 builder 风格注入各服务
trait 对象：

```rust
use cmx_common_api::CmxAppState;
use std::sync::Arc;

let state = CmxAppState::new()
    .with_plugin_query(plugin_manager)       // Arc<dyn PluginQuery>
    .with_runtime_invoker(wasm_engine)       // Arc<dyn RuntimeInvoker>
    .with_service_query(service_query)       // Arc<dyn ServiceQuery>
    .with_service_storage(service_storage)   // Arc<dyn ServiceStorage>
    .with_storage_service(storage)           // Arc<dyn StorageService>
    .with_auth_service(auth)                 // Arc<dyn AuthService>
    .with_iam(iam_state)                     // Arc<IamState>
    .with_resource_data_importer(importer)   // Arc<dyn ResourceDataImporter>
    .with_definition_importers(bundle);      // Arc<DefinitionImporterBundle>
```

`app_id` 字段在 `new()` 时从 ConfigManager 读取（应用隔离标识，初始化后不可变）。

`CmxAppState` 内部持有（均为 `Option`，经对应 `with_*` 注入）：

| 字段 | 类型 | 用途 |
|------|------|------|
| `app_id` | `String` | 应用隔离标识（多租户/多应用） |
| `plugin_query` | `Arc<dyn PluginQuery>` | 插件查询 |
| `runtime_invoker` | `Arc<dyn RuntimeInvoker>` | WASM 运行时调用 |
| `service_query` | `Arc<dyn ServiceQuery>` | 服务查询 |
| `service_storage` | `Arc<dyn ServiceStorage>` | 服务存储 |
| `storage_service` | `Arc<dyn StorageService>` | 文件存储 |
| `auth_service` | `Arc<dyn AuthService>` | 认证 |
| `iam` | `Arc<IamState>` | IAM 服务状态 |
| `resource_data_importer` | `Arc<dyn ResourceDataImporter>` | 资源数据导入 |
| `definition_importers` | `Arc<DefinitionImporterBundle>` | 模块资源定义导入器集合 |

在 handler 中经 `State(state): State<CmxAppState>` 提取器访问。

### 二、统一响应与错误

响应与错误类型来自 `cmx-api-types`（本 crate re-export），详见其 README：

```rust
use cmx_common_api::{ApiResp, Error};

let resp = ApiResp::ok(user_list);                         // {"code":0,"msg":"success","data":[...]}
let resp = ApiResp::ok_with_pagination(list, 1, 20, 100);  // 附分页元信息
let resp: ApiResp<()> = ApiResp::fail(400, "参数错误");

// Error 实现 IntoResponse，可直接在 handler 返回 Result<T> 中使用
fn find_user(id: &str) -> Result<User> {
    /* ... */ Err(Error::not_found("用户不存在"))
}
```

常用错误构造器与 HTTP 状态码映射：

| 构造器 / 变体 | 业务码 | HTTP 状态码 |
|---------------|--------|-------------|
| `Error::business_error` | 1 | 200 |
| `Error::bad_request` | 400 | 400 |
| `Error::unauthorized` | 401 | 401 |
| `Error::forbidden` | 403 | 403 |
| `Error::not_found` | 404 | 404 |
| `Error::conflict`（乐观锁） | 409 | 409 |
| `Error::validation_error` | 422 | 422 |
| `Error::rate_limit_exceeded` | 429 | 429 |
| `Error::internal_error` | 500 | 500 |
| `Error::service_unavailable` | 503 | 503 |
| `Error::Timeout` | 504 | 504 |

### 三、路由聚合与平台装配

各域 handler 实现 `ModuleRoutes` trait（定义于 cmx-api-core，本 crate re-export）：

```rust
pub trait ModuleRoutes {
    fn routes(self) -> Router<CmxAppState>;  // 注册该模块的路由
    fn prefix() -> &'static str;             // 模块前缀路径
    fn module_name(&self) -> &'static str;   // 模块名称（日志/调试）
}
```

装配层（cmx-platform-app）逐个 `merge`：

```rust
use cmx_common_api::routes::routes_impl::api_routes;
use cmx_common_api::routes::traits::ModuleRoutes;
use cmx_ai_api::AiModule;
use cmx_iam_api::{AuthModule, IamModule};
// ... 其余域 Module

let router = api_routes()       // service + debug + portal + /health（本 crate）
    .merge(AuthModule.routes()) // 认证（cmx-iam-api）
    .merge(IamModule.routes())  // IAM（cmx-iam-api）
    .merge(AiModule.routes())   // AI 中继（cmx-ai-api）
    /* Doc / Dct / Mdm / Job / Model / Code / Storage / Domain ... */
    ;
```

实际装配见 `cmx-platform-app/src/routes.rs`（平台主应用）与 `cmx-portalservice`
（独立门户微服务，复用 portal 泛型路由）。

`api_routes()` 内部实现（源码即迁移记录，已迁出的域以注释留痕）：

```rust
pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();
    // 认证（AuthModule）+ IAM（IamModule）路由已迁至 cmx-iam-api，由 cmx-platform-app 合并。
    // Domain/Application/Menu/SysDatasource/Form 路由已迁至 cmx-biz-api，由 platform-app 合并。
    // 插件管理 / 表元数据 / 插件市场 路由已迁至 cmx-plugin-api。
    // 文件存储路由（StorageModule）已迁至 cmx-storage-api。
    // AI 中继路由（AiModule）已迁至 cmx-ai-api。
    let router = router.merge(service::ServiceModule.routes());
    let router = router.merge(debug::DebugModule.routes());
    let router = router.merge(portal::PortalModule.routes());
    // dev-tools feature 启用时才注册 dev 路由（生产禁用）
    router.route("/health", get(health_check))
}
```

### 四、通用 CRUD

#### 4.1 rest 通用 handler（cmx-api-core，本 crate re-export）

`cmx_common_api::rest::handler` 提供 8 个泛型 handler，基于 `DbBmc` + modql Filter 工作：

| handler | 说明 |
|---------|------|
| `create` / `create_many` | 创建 / 批量创建 |
| `get_by_id` | 按 ID 获取（唯一 GET） |
| `update` / `update_many` | 更新 / 批量更新 |
| `delete` | 删除 |
| `list` / `page` | 列表 / 分页查询 |

按 AGENTS.md §八硬约束：除 `get_by_id`（GET）外一律 POST + application/json，
每个操作独立路径，禁止共享路径。

#### 4.2 register_crud_routes! 宏

按 `(router, bmc, filter, entity_create, entity_update, prefix)` 六参签名注册 8 条 CRUD 路由：

```rust
use cmx_common_api::register_crud_routes;

let router = register_crud_routes!(
    router,
    UserBmc, UserFilter, UserForCreate, UserForUpdate,
    "/api/users"
);
// 生成：POST /api/users/create | /create-many | /update | /update-many | /delete | /list | /page
//      GET  /api/users/get
```

#### 4.3 declare_crud_handlers! 宏

为实体生成带 OpenAPI 注解与权限注入的 CRUD handler 模块，支持三种权限配置模式：

```rust
// 模式一：统一资源名（权限码自动拼 :create/:read/:update/:delete 后缀）
declare_crud_handlers!(user_crud, User, UserBmc, UserForCreate, UserForUpdate,
    UserFilter, "User", "/users", "user");

// 模式二：无鉴权（仅 8 参）
declare_crud_handlers!(user_crud, User, UserBmc, UserForCreate, UserForUpdate,
    UserFilter, "User", "/users");

// 模式三：精细化 perms(...) 配置
declare_crud_handlers!(user_crud, User, UserBmc, UserForCreate, UserForUpdate,
    UserFilter, "User", "/users",
    perms(create = "user", read = "user", update = "user_admin", delete = "user_admin"));
```

另有 `register_crud_handlers_module!` / `setup_crud_api!` 组合宏。
按 AGENTS.md §八：`declare_crud_handlers!` 仅限各 `*-api` crate 内部使用。

### 五、service 模块端点

| 方法 | 路径 | 作用 |
|------|------|------|
| POST | `/api/service/call` | 按 ServiceCallRequest（体内 service_key + 参数）调用服务 |
| POST | `/api/service/execute` | 携带完整执行上下文的多步编排 |
| POST | `/api/service/execute/{service_key}` | 按 service_key 直接执行（便于外部系统直连） |
| POST | `/api/service/page` | 分页查询服务定义（`/list` 已废弃，由 `/page` 取代） |
| GET | `/api/service/by-plugin` | 按插件查询其下注册的服务清单 |
| GET | `/api/service/get` | 查询单个服务定义详情 |
| POST | `/api/service/delete` | 删除服务定义 |
| GET | `/api/service/exists` | 判断 service_key 是否已注册 |
| GET | `/api/service/openapi` | 导出本平台服务聚合后的 OpenAPI 规范（供外部 SDK 生成） |

核心请求/响应模型（`handlers::service::models`，均派生 ToSchema 进 Swagger）：

| 模型 | 用途 |
|------|------|
| `FunctionCallRequest` / `FunctionCallResponse` | `/call` 函数调用 |
| `ServiceExecuteRequest` / `ServiceExecuteResponse` / `ServiceExecutionStep` | `/execute` 多步编排（步骤/耗时/错误） |
| `ServiceOrchestrationError` | 编排错误 |
| `ServiceListItem` / `ServiceDetailResponse` | `/page`、`/get` 返回 |
| `ServiceGetQuery` / `ServiceByPluginQuery` / `ServiceExistsQuery` / `ServiceDeleteQuery` / `OpenApiQuery` | 各 GET/DELETE 端点查询参数 |

### 六、portal 模块端点

`PortalModule` 挂 `/api` 下（路径与原 Node 后端一致），按功能分子模块。主要端点：

**ai —— AI 对话中继 + 本地编辑代理**

| 方法 | 路径 | 作用 |
|------|------|------|
| POST | `/api/ai/chat` | 门户 AI 对话 |
| GET | `/api/agent/capabilities` | 编辑代理能力 |
| POST | `/api/agent/message` | 代理消息 |
| POST | `/api/agent/message/stream` | 代理消息（SSE 流式） |
| POST | `/api/agent/approvals/{id}` | 代理审批 |

**meta —— 工作区节点**

| 方法 | 路径 | 作用 |
|------|------|------|
| GET / POST | `/api/workspace-nodes` | 列出 / 保存工作区节点 |
| GET / DELETE | `/api/workspace-nodes/{id}` | 获取 / 删除单个节点 |

**pages —— 表单页 / 原生页 / HTML 页面**

| 方法 | 路径 | 作用 |
|------|------|------|
| GET / POST | `/api/form-pages`、`/api/native-pages`、`/api/html-pages` | 列出 / 保存三类页面 |
| POST | `/api/native-pages/batch`、`/api/html-pages/batch` | 批量保存 |
| GET | `/api/form-pages/{id}`、`/api/native-pages/{id}`、`/api/html-pages/{id}` | 获取单页 |

**data —— 事实数据 + 帮助中心**

| 方法 | 路径 | 作用 |
|------|------|------|
| GET | `/api/fact/list` | 事实数据列表 |
| POST | `/api/fact/get` | 事实数据查询 |
| GET | `/api/fact/{domain}/{app}/{module}/{file}` | 按路径读取事实数据 |
| GET | `/api/help/catalog` | 帮助中心目录 |
| POST | `/api/help/get` | 获取帮助文档 |
| POST/GET/DELETE | `/api/help/doc`、`/api/help/doc/{domain}/{app}/{module}/{file}` | 帮助文档保存 / 读取 / 删除 |

**notify —— 通知中心**

| 方法 | 路径 | 作用 |
|------|------|------|
| GET | `/api/notifications` | 通知列表 |
| GET | `/api/notifications/centers`、`/api/notifications/counts` | 通知中心 / 未读计数 |
| POST | `/api/notifications/publish`、`/api/notifications/mark-read` | 发布 / 标记已读 |
| GET | `/api/notifications/stream` | SSE 主动推送 |

**launcher / registry —— 启动器与注册表派生**

| 方法 | 路径 | 作用 |
|------|------|------|
| POST | `/api/launcher/resolve` | 功能启动器解析 |
| GET | `/api/registry/domains`、`/api/registry/apps`、`/api/registry/modules`、`/api/registry/dam` | 注册表只读派生（DAM） |
| GET | `/api/service-catalog`、`/api/service-catalog/{id}` | 服务目录 |
| GET | `/api/modules`、`/api/modules/{domain}/{application}/{module}` | 模块清单 |
| GET | `/api/modules/{domain}/{application}/{module}/resources/{type}`、`/api/module-resources` | 模块资源 |

portal handler 不读 AppState（走全局单例 `get_default_db_manager()` / `data_root()`，认证经
已泛型的 `CmxSvrContext`），路由对 state 泛型 `S`：平台内嵌壳实例化为
`portal_routes::<CmxAppState>()`，独立门户微服务（cmx-portalservice）实例化为
`portal_routes::<()>()`，同一份 handler 两处跑、能力不缩水。

> 模型中心接口（definitions / flexible_combination / model deploy）已从 portal 迁至独立 crate
> `cmx-model-api`（`ModelModule`，位于 `crates/libs/cmx-model/cmx-model-api/`，不在 cmx-apis/
> 目录下），由 cmx-platform-app 直接合并，不在本 crate。

### 七、中间件

中间件实现于 cmx-api-core（本 crate re-export 为 `cmx_common_api::middleware`）：

- `mw_context_resolver`：请求上下文解析（svrContext）
- `mw_auth`：认证（支持 `[auth].whitelist` 白名单与 query `access_token` 兜底）
- `mw_permission`：权限校验
- `cors_layer`：CORS 跨域（另有 `cors_layer_permissive` 宽松版）
- `mw_security_headers`：安全响应头
- `mw_trace` / `trace_layer`：请求追踪（OTel 风格，含 trace/sanitizer/detector 子模块）

> `mw_rate_limit` 限流中间件已在 cmx-api-core 中注释停用（`middleware/mod.rs` 中模块与
> re-export 均被注释），当前版本不可用；`cmx_api_types::Error::rate_limit_exceeded` 与
> `into_rate_limit_response()`（Retry-After 头）保留供调用方手工使用。

```rust
// cmx-platform-app/src/router.rs 的实际装配方式
use cmx_common_api::middleware::{
    cors_layer, mw_auth, mw_context_resolver, mw_permission, trace_layer,
};

let app = Router::new()
    .merge(api_routes)
    .layer(mw_auth())
    .layer(mw_permission())
    .layer(mw_context_resolver())
    .layer(trace_layer())
    .layer(cors_layer())
    .with_state(app_state);
```

### 八、OpenAPI 文档

```rust
use cmx_common_api::openapi::{ApiDoc, PortalApiDoc};
use cmx_common_api::routes::routes_impl::swagger_routes;
use utoipa::OpenApi;

// Swagger UI（swagger_routes() 已封装）：
//   UI:   /swagger-ui/
//   JSON: /api-docs/openapi.json
let swagger = swagger_routes();

// 门户切片由平台装配层聚合：
let merged = ApiDoc::openapi().merge(PortalApiDoc::openapi()); // platform-app 中继续 merge 各域 ApiDoc
```

## 路由迁移去向（2026-07/08 handler 大迁移）

原 cmx-api 的 handler 已按域拆分，`api_routes()` 中的对应注释即迁移记录：

| 原 handler（cmx-api） | 去向 |
|------------------------|------|
| Auth + IAM（User/Role/Permission/API Key/OAuth2/Audit...） | `cmx-iam-api`（AuthModule / IamModule） |
| Domain / Application / Menu / SysDatasource / Form | `cmx-biz-api` |
| Module CRUD + Module 包 | `cmx-biz-api`（ModuleCrudModule）+ `cmx-plugin-api`（ModulePackageModule） |
| 插件管理 / 表元数据 / 插件市场 | `cmx-plugin-api` |
| 文件存储 | `cmx-storage-api`（StorageModule） |
| AI 中继 | `cmx-ai-api`（AiModule） |
| rest / middleware / CRUD 宏 / actor / db_id / msgpack | 下沉 `cmx-api-core` |
| validation_fail_resp | 移至 `cmx-biz::errcode` |

各域路由统一由 `cmx-platform-app` 合并进主路由，OpenAPI 切片由 `OpenApi::merge()` 聚合。

### 旧路径迁移对照（cmx-api → 现路径）

| 旧路径（cmx-api 时代） | 现路径 |
|------------------------|--------|
| `cmx_api::CmxAppState` | `cmx_common_api::CmxAppState`（定义在 cmx-api-core，双路径可用） |
| `cmx_api::ApiResp` / `Error` / `Result` | `cmx_common_api::ApiResp` / `Error` / `Result`（定义在 cmx-api-types） |
| `cmx_api::rest::handler::{create, list, page, ...}` | `cmx_common_api::rest::handler::*`（同） |
| `cmx_api::middleware::{mw_auth, cors_layer, ...}` | `cmx_common_api::middleware::*`（同） |
| `cmx_api::routes::traits::ModuleRoutes` | `cmx_common_api::routes::traits::ModuleRoutes` |
| `cmx_api::register_crud_routes!` / `declare_crud_handlers!` | `cmx_common_api::register_crud_routes!` / `declare_crud_handlers!` |
| `cmx_api::db_id` / `msgpack` / `actor` | 下沉 `cmx-api-core`（cmx_api_core::db_id 等） |
| `cmx_api::validation_fail_resp` | `cmx_biz::errcode` |
| `cmx_api::handlers::{application, domain, menu, sys_datasource, module}` | `cmx-biz-api`（+ module 包部分在 `cmx-plugin-api`） |
| `cmx_api::handlers::{plugin, table_metadata}` | `cmx-plugin-api` |
| `cmx_api::handlers::service` / `debug` / `portal` | 保留本 crate（`cmx_common_api::handlers::*`） |

## 重构历史

| 时间 | 事件 |
|------|------|
| 2026-07-30 | 「cmx-api 依赖与 handler 重构」方案立项：拆分大 cmx-api |
| 2026-08-11 | 重构完成：共享骨架下沉 cmx-api-core，域 handler 迁入各 `cmx-*-api` 皮肤 crate；cmx-domain-api 分组目录改名 cmx-apis，本 crate 更名 cmx-common-api |
| 后续 | portal 中模型中心接口再迁独立 crate `cmx-model-api`；AI 中继路由迁 `cmx-ai-api`（OpenApi 切片由 platform-app 聚合） |

## 与其他 crate 的关系

```
cmx-api-types（叶子：响应/错误/Doc 类型）
        ↑
cmx-api-core（共享骨架：CmxAppState/ModuleRoutes/rest/middleware/CRUD 宏）
        ↑
cmx-common-api（本 crate：通用 handler + 路由聚合 + OpenAPI 切片）
        ↑
cmx-platform-app（平台总装配：合并本 crate api_routes() + 各域 *-api ModuleRoutes）

同级皮肤 crate：cmx-biz-api / cmx-iam-api / cmx-plugin-api / cmx-storage-api / cmx-ai-api
复用方：cmx-portalservice（独立门户微服务，复用 portal 泛型路由）
```

依赖的主要内部 crate（见 Cargo.toml）：cmx-api-core / cmx-api-types / cmx-biz / cmx-portal /
cmx-ai / cmx-auth / cmx-rpc / cmx-orchestrator-rpc / cmx-service / cmx-plugin / cmx-storage /
cmx-debug / cmx-metadata / cmx-buffer / cmx-traits / cmx-utils / cmx-core / cmx-database /
cmx-database-pg。

## 常见问题

### Q: cmx-api 与 cmx-common-api 是什么关系？

**A**: 2026-07/08 重构后原 `cmx-api` 拆分：共享骨架下沉 `cmx-api-core`、各域 handler 迁至
`cmx-*-api` 皮肤 crate；本 crate 因不再是唯一的 "api" crate 而改名 `cmx-common-api`
（common = 跨域通用 + 装配中枢），并通过 re-export 保持 `cmx_api::CmxAppState`、
`cmx_api::rest::handler::create` 等旧路径兼容。

### Q: 新增一个业务域的 handler 应该放哪？

**A**: 按 AGENTS.md §八「cmx-*-api Handler 规范」：`*-api` crate 应保持为纯 HTTP 适配层，
Entity / BMC / Filter / Service 归业务 crate；新域 handler 放对应业务域的 `*-api` crate
（没有则新建），实现 `ModuleRoutes` trait，由 cmx-platform-app 合并；本 crate 只收跨域通用 handler。

### Q: dev-tools feature 为什么默认关闭？

**A**: 开发脚手架涉及本地 fs 写入/解压 zip/写 settings.json，违反集群无状态约束
（AGENTS.md §五），仅单节点 dev 环境可启用；启用时启动会打印告警日志。

### Q: Swagger UI 从哪访问？

**A**: 装配层 merge `swagger_routes()` 后访问 `/swagger-ui/`，OpenAPI JSON 在
`/api-docs/openapi.json`（仅含 ApiDoc 覆盖的 service 域；门户与各域切片经 platform-app
`OpenApi::merge()` 聚合后同样可见）。

### Q: 健康检查端点需要认证吗？

**A**: 不需要。`/health` 无需认证，返回 `{"status":"ok"}`，供 Docker HEALTHCHECK 和
负载均衡器探测使用。

### Q: service 的 `/list` 接口去哪了？

**A**: 已废弃，由 `POST /api/service/page`（分页查询）取代；源码中留有注释
`// .route("/list", get(list_services))  // 已废弃，由 /page 取代`。

### Q: 门户路由能脱离平台单独部署吗？

**A**: 可以（P-S0 门户微服务化）。portal handler 不读 AppState，路由对 state 泛型：
cmx-portalservice 独立微服务用 `portal_routes::<()>()` 复用同一份路由表与 handler，
Swagger 同样可见（复用同一装配核）。
