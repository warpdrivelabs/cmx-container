# cmx-api-core

> cmx 平台 Web API 共享骨架层：承载各域 `cmx-*-api` crate 共用的 `CmxAppState` 应用状态、`ModuleRoutes` 路由契约、通用 CRUD handler、CRUD 声明宏与全套 HTTP 中间件。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-api-core` 是从原 `cmx-api` 抽出的**稳定 HTTP 骨架层**。cmx-container 的 `crates/libs/cmx-apis/` 目录采用「域皮肤 crate」架构：每个 `cmx-<domain>-api` 只做协议适配（参数提取 / 响应封装），业务逻辑在对应域 crate（cmx-iam / cmx-plugin / cmx-biz / cmx-ai / cmx-storage 等）。而这些皮肤 crate 共同依赖的东西——应用共享状态、路由注册契约、泛型 CRUD handler、认证/权限/追踪中间件——全部沉淀在本 crate，避免每个域重复实现。

### 职责边界（对齐 AGENTS.md 第八章规范）

按仓库规范，各 `*-api` crate 应保持为**纯 HTTP 适配层**：Entity / BMC / Filter / Service 归业务 crate，`*-api` 通过 `use` 引用、禁止重新定义。本 crate 提供这个"纯适配层"所需的全部地基：

| 类别 | 模块 | 内容 |
|------|------|------|
| 应用状态 | `app_state` | `CmxAppState`（trait 对象注入容器）+ `IamState`（IAM 服务聚合） |
| 路由契约 | `routes::traits` | `ModuleRoutes` trait——所有 handler 模块的路由注册契约 |
| 路由宏 | `routes::macros` | `declare_crud_handlers!` / `register_crud_handlers_module!` 等 CRUD 宏 |
| 通用 CRUD | `rest` | 8 个泛型 handler（create/list/page/get/update/...）+ header 解析 |
| 中间件 | `middleware` | 认证 / 上下文 / 权限 / CORS / 安全头 / 追踪（旧版 + 高性能新版） |
| 横切工具 | `db_id` / `actor` / `msgpack` | 请求库路由、操作者身份提取、msgpack 成功信封 |

### 依赖方向（无环设计）

```
各域 *-api crate（cmx-iam-api / cmx-biz-api / ...）
    →  本 crate（骨架） + 对应服务 crate（cmx-iam / cmx-biz / ...）
服务 crate（cmx-biz / cmx-iam / ...）不反向依赖本 crate
```

过渡期说明：本 crate 为 `mw_auth` / `CmxAppState` 持有 cmx-iam（`IamState`）与 cmx-storage（`storage_service`）的具体类型。由于服务 crate 不依赖本 crate，这是单向边，**不成环**；阶段 4（可选）trait 化后可移除。

### ApiResp / Result / Error re-export 策略

`ApiResp` / `Result` / `Error` 等响应类型实际定义在 `cmx-api-types`，本 crate 顶层 re-export。这使 CRUD 宏里的 `$crate::Error` / `$crate::ApiResp` / `$crate::Result`（`$crate` 解析到本 crate）自动生效，宏零改动；也使迁入的 rest / middleware 模块（原 `use crate::ApiResp`）零改动解析。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-types` | `ApiResp` / `Error` / `Result` / `Pagination` / `UnitResp` / `*ParamsDoc` / `TreeNode` 等响应与文档类型（本 crate re-export） |
| `cmx-core` | `SVRContext` / `AuthContext` / `ListParams` / `PageParams` / `UpdatePayload` / `DeletePayload` / `DataSet` |
| `cmx-traits` | `AuthService` / `UserAuthQuery` / `PermissionChecker` / `PluginQuery` / `RuntimeInvoker` / `ServiceQuery` / `ServiceStorage` / `ResourceDataImporter` 等 trait + `context_scope` task-local |
| `cmx-utils` | `ConfigManager`（app_id）/ `UuidGenerator` |
| `cmx-database` | `DatabaseManager` / `GenericCrudService` / `DbBmc` / `get_default_db_manager()` |
| `cmx-database-pg` | `get_default_pg_db_manager()`（db_id.rs 业务库路由回退，infra 非 business） |
| `cmx-auth` | `BUILTIN_WHITELIST` / `OAuth2Policy` / `OAuth2ProviderRegistry`（mw_auth 用） |
| `cmx-iam` | `IamState` 聚合的服务 trait（`UserService` 等）与 `IamChecker`、`ExclusionRuleService` |
| `cmx-storage` | `StorageService` / `StorageAppState`（`FromRef` 转换目标） |
| `modql` | `FilterGroups` / `IntoFilterNodes` / `HasSeaFields`（CRUD 泛型约束） |
| `axum` / `tower-http` | Web 框架 / CORS 层 |
| `regex` | 认证白名单通配符规则编译 |
| `rmp` | msgpack 二进制信封编码（`msgpack.rs`） |
| `utoipa` | CRUD 宏的 `#[utoipa::path]` 属性展开 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-iam-api` / `cmx-plugin-api` / `cmx-biz-api` / `cmx-ai-api` / `cmx-storage-api` | `cmx-api-core = { workspace = true }` | 域皮肤 crate：实现 `ModuleRoutes`、复用 `CmxAppState` / `CmxSvrContext` / `ApiResp` |
| `cmx-common-api` / `cmx-rpt-api` / `cmx-rule-api` / `cmx-flow-api` / `cmx-doc-api` / `cmx-dct-api` / `cmx-mdm-api` / `cmx-code-api` / `cmx-job-api` / `cmx-model-api` | `cmx-api-core = { workspace = true }` | 同上（其余域皮肤 crate） |
| `cmx-platform-app` | 经各 `*-api` 传递依赖 | 平台总装配器：合并各 Module 路由时使用 `CmxAppState` 作为 Router 状态 |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，间接依赖本 crate 的全部基建 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 应用共享状态 | `CmxAppState`：以 builder 风格注入 9 类 trait 对象（plugin_query / runtime_invoker / service_query / service_storage / storage_service / auth_service / iam / resource_data_importer / definition_importers）+ `app_id` 多租户隔离标识 |
| IAM 状态聚合 | `IamState` 聚合 `UserService` / `RoleService` / `RoleGroupService` / `PermissionService` / `ExclusionRuleService` / `PermissionChecker` / `UserAuthQuery`，经 `CmxAppState::iam()` 访问 |
| 路由注册契约 | `ModuleRoutes` trait：`routes()` 返回 `Router<CmxAppState>`，`prefix()` / `module_name()` 供日志与文档 |
| 通用 CRUD handler | `rest::handler` 提供 8 个泛型函数（create / create_many / get_by_id / update / update_many / delete / list / page），按 `db_id` 请求头路由数据库，委托 `GenericCrudService` |
| CRUD 声明宏 | `declare_crud_handlers!`（三种权限模式）生成带 OpenAPI 注解 + 权限检查的 8 个 handler；`register_crud_handlers_module!` / `setup_crud_api!` 一键注册 |
| 认证中间件 | `mw_auth`：X-API-Key / Bearer JWT（含 query `access_token` 兜底）/ on-behalf-of 委托头，通配符白名单（`*` 单层、`**` 多层），注入 `AuthContext` 到 `CmxSvrContext` 与 task-local scope |
| 权限中间件 | `mw_permission`：路由前缀 → 权限码映射（TOML `[iam_permissions]`），最长前缀匹配，`system:all` 短路放行，无权返回 403 |
| 上下文中间件 | `mw_context_resolver`：为每个请求构建 `SVRContext`（request_id / headers / 时间戳）并以 `CmxSvrContext` 提取器暴露 |
| CORS / 安全头 | `cors_layer()` / `cors_layer_permissive()`、`mw_security_headers`（nosniff / DENY / HSTS 等 5 个安全头） |
| 请求追踪 | `mw_trace`（旧版全量打印）与 `trace_layer`（高性能版：INFO 零开销 / DEBUG 全量脱敏，运行时按日志级别自动切换） |
| 请求库路由 | `db_id::resolve_db_id_from_headers`：`db_id` 头缺失/空/非法时回退业务库（`get_biz_db_id`）；`rest::header_parse::get_db_id_from_header` 回退默认库 |
| 操作者提取 | `actor::actor_id_i64` / `actor::actor_name`：从 `CmxSvrContext.auth_context` 提取操作者，缺失兜底 `0` / "系统" |
| msgpack 信封 | `msgpack::encode_envelope_ok` / `msgpack_ok_response`：`{code:0, msg:"success", data}` 的 `application/x-msgpack` 响应（doc/dct 列式二进制端点共用） |
| 状态桥接 | `FromRef<CmxAppState> for cmx_storage::handler::AppState`：axum 状态自动拆解（孤儿规则要求 impl 定义在本 crate） |

---

## 模块结构

```text
cmx-api-core
├── src
│   ├── lib.rs                    # 模块声明 + ApiResp/Error/Result 等 re-export + 顶层便捷导出
│   ├── app_state.rs              # CmxAppState（builder 注入 trait 对象）+ IamState + FromRef 桥接
│   ├── actor.rs                  # 操作者身份提取（actor_id_i64 / actor_name）
│   ├── db_id.rs                  # 请求库路由（resolve_db_id_from_headers / resolve_db_id，回退业务库）
│   ├── msgpack.rs                # msgpack 成功信封（encode_envelope_ok / msgpack_ok_response）
│   ├── rest/                     # REST 协议层（通用 CRUD）
│   │   ├── mod.rs                #   模块导出 + Doc 类型 re-export
│   │   ├── handler.rs            #   8 个泛型 CRUD handler（委托 GenericCrudService）
│   │   └── header_parse.rs       #   get_db_id_from_header（db_id 头 → 默认库回退）
│   ├── routes/                   # 路由注册基建
│   │   ├── mod.rs                #   模块导出
│   │   ├── traits.rs             #   ModuleRoutes trait（路由注册契约）
│   │   └── macros.rs             #   register_crud_routes! / declare_crud_handlers! 等 5 个宏
│   └── middleware/               # 中间件族
│       ├── mod.rs                #   模块导出（mw_rate_limit 已注释停用）
│       ├── mw_auth.rs            #   认证中间件 + GlobalAuthService（白名单/API-Key/OAuth2 全局态）
│       ├── mw_context.rs         #   上下文中间件 + CmxSvrContext 提取器
│       ├── mw_permission.rs      #   权限中间件 + GlobalPermissionConfig
│       ├── mw_cors.rs            #   CORS 配置与层构造
│       ├── mw_security_headers.rs#   安全响应头中间件
│       ├── mw_trace.rs           #   旧版请求追踪中间件（全量打印）
│       └── trace/                #   高性能请求追踪（按日志级别切换 INFO/DEBUG 模式）
│           ├── mod.rs            #     TraceConfig / TraceMode 导出
│           ├── config.rs         #     运行时日志级别探测与模式选择
│           ├── detector.rs       #     文件上传/下载排除检测
│           ├── layer.rs          #     trace_layer 实现
│           └── sanitizer.rs      #     敏感字段脱敏
└── Cargo.toml
```

---

## 关键类型 / API

### CmxAppState（应用共享状态）

```rust
pub struct CmxAppState {
    pub app_id: String,                       // 应用隔离标识（初始化后不可变）
    // 私有 Option 字段 + builder 注入 + getter：
    // plugin_query / runtime_invoker / service_query / service_storage /
    // storage_service / auth_service / iam / resource_data_importers / definition_importers
}

impl CmxAppState {
    pub fn new() -> Self;                                  // app_id 取自 ConfigManager
    pub fn with_plugin_query(self, q: Arc<dyn PluginQuery>) -> Self;
    pub fn with_runtime_invoker(self, i: Arc<dyn RuntimeInvoker>) -> Self;
    pub fn with_iam(self, iam: Arc<IamState>) -> Self;
    pub fn with_auth_service(self, s: Arc<dyn AuthService>) -> Self;
    // ... 其余 with_* 同构
    pub fn iam(&self) -> Option<&Arc<IamState>>;
    pub fn auth_service(&self) -> Option<&Arc<dyn AuthService>>;
    pub fn is_fully_initialized(&self) -> bool;            // plugin_query + runtime_invoker 均已设置
}
```

### ModuleRoutes trait（路由契约）

```rust
pub trait ModuleRoutes {
    fn routes(self) -> Router<CmxAppState>;   // 注册该模块的路由
    fn prefix() -> &'static str;              // 模块前缀路径
    fn module_name(&self) -> &'static str;    // 模块名称（日志/调试）
}
```

### 通用 CRUD handler（rest::handler）

```rust
pub async fn create<MC: DbBmc, E: HasSeaFields + DeserializeOwned>(...) -> Result<Json<ApiResp<DataSet>>>;
pub async fn create_many<MC, E>(...)  -> Result<Json<ApiResp<DataSet>>>;
pub async fn get_by_id<MC: DbBmc>(...) -> Result<Json<ApiResp<DataSet>>>;
pub async fn update<MC, E>(...)        -> Result<Json<ApiResp<DataSet>>>;
pub async fn update_many<MC, E>(...)   -> Result<Json<ApiResp<DataSet>>>;
pub async fn delete<MC: DbBmc>(...)    -> Result<Json<ApiResp<DataSet>>>;
pub async fn list<MC, F>(...)          -> Result<Json<ApiResp<DataSet>>>;   // F: IntoFilterNodes
pub async fn page<MC, F>(...)          -> Result<Json<ApiResp<DataSet>>>;   // 含分页信息
```

### CRUD 宏（`#[macro_export]`，经 `cmx_api_core::` 调用）

| 宏 | 用途 |
|----|------|
| `declare_crud_handlers!(mod, entity, bmc, for_create, for_update, filter, tag, prefix, ...)` | 生成带 OpenAPI 注解的 8 handler 模块；第 9 参数可选：统一资源名 / `perms(create=..., read=..., update=..., delete=...)` / 省略（无鉴权） |
| `register_crud_handlers_module!(router, mod, prefix)` | 把生成的模块注册到 router（8 条路由） |
| `setup_crud_api!(router, ...)` | 声明 + 注册组合宏 |
| `register_crud_routes!(router, bmc, filter, e_create, e_update, prefix)` | 旧版：直接挂 rest::handler 泛型函数（无 OpenAPI） |

权限注入规则：create/create_many → `{resource}:create`；get/list/page → `{resource}:read`；update/update_many → `{resource}:update`；delete → `{resource}:delete`。空资源名 = 不鉴权。

### 中间件与全局管理器

```rust
// 认证：需在 mw_context_resolver 之后注册
pub async fn mw_auth(req: Request<Body>, next: Next) -> Result<Response, StatusCode>;
pub struct GlobalAuthService;      // initialize / initialize_whitelist / initialize_oauth2 /
                                   // initialize_provider_registry / is_whitelisted / get ...

// 上下文与提取器
pub async fn mw_context_resolver(req: Request<Body>, next: Next) -> Result<Response>;
pub struct CmxSvrContext(pub SVRContext);   // 实现 FromRequestParts，handler 直接作参数提取

// 权限
pub async fn mw_permission(...) -> Result<Response, StatusCode>;
pub struct GlobalPermissionConfig;  // initialize / initialize_checker / find_required_permission

// CORS / 安全头 / 追踪
pub fn cors_layer() -> CorsLayer;
pub fn cors_layer_permissive() -> CorsLayer;
pub async fn mw_security_headers(req: Request<Body>, next: Next) -> Response;
pub async fn mw_trace(req: Request<Body>, next: Next) -> Response;   // 旧版
pub fn trace_layer() -> ...;                                          // 高性能版（trace::layer）
```

---

## 使用示例

### 一、组装 CmxAppState（组装层场景，如 cmx-platform-app）

```rust
use std::sync::Arc;
use cmx_api_core::{CmxAppState, IamState};

// 组装层构造状态：以 builder 链式注入各域 trait 对象实现。
// DatabaseManager 不经 state 传递，各 handler 通过 get_default_db_manager() 全局获取。
let iam = Arc::new(IamState {
    user_service: /* Arc<dyn UserService> 实现 */,
    role_service: /* ... */,
    role_group_service: /* ... */,
    permission_service: /* ... */,
    rule_service: None,          // 互斥规则服务可选
    permission_checker: /* Arc<dyn PermissionChecker> */,
    iam_checker: None,           // finalize_iam_state 阶段注入
    user_auth_query: /* Arc<dyn UserAuthQuery> */,
});

let state = CmxAppState::new()               // app_id 自动取自 ConfigManager
    .with_plugin_query(plugin_manager)       // Arc<dyn PluginQuery>
    .with_runtime_invoker(wasm_engine)       // Arc<dyn RuntimeInvoker>
    .with_iam(iam)
    .with_auth_service(auth_service_impl);   // Arc<dyn AuthService>

assert!(state.is_fully_initialized());       // plugin_query + runtime_invoker 均已设置
```

### 二、实现 ModuleRoutes 挂载模块路由（各域 *-api crate 场景）

```rust
use axum::Router;
use axum::routing::{get, post};
use cmx_api_core::{CmxAppState, ModuleRoutes};

struct DemoModule;

impl ModuleRoutes for DemoModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 按仓库规范：除 get_by_id（GET）外 CRUD 一律 POST + application/json，
            // 每个操作独立路径，禁止共享路径
            .route("/demo/create", post(create_demo))
            .route("/demo/get", get(get_demo))
    }
    fn prefix() -> &'static str { "demo" }
    fn module_name(&self) -> &'static str { "demo" }
}

// cmx-platform-app 侧合并：Router::new().merge(DemoModule.routes())
```

### 三、declare_crud_handlers! 宏生成全套 CRUD（带权限与 OpenAPI）

```rust
use cmx_api_core::declare_crud_handlers;

// 为实体生成 8 个 handler（create/create-many/get/update/update-many/delete/list/page），
// 自动拼接权限码 user:create / user:read / user:update / user:delete 并调用
// svr_ctx.require_permission 校验；未传第 9 参数则不鉴权。
declare_crud_handlers!(
    user_crud,        // 生成的模块名
    User,             // 实体类型
    UserBmc,          // BMC（表元信息，实现 DbBmc）
    UserForCreate,    // 创建 DTO（不含 id / create_time / update_time）
    UserForUpdate,    // 更新 DTO（全 Option）
    UserFilter,       // modql 过滤器（derive FilterNodes）
    "User",           // OpenAPI tag
    "/users",         // 路由前缀
    "user"            // 权限资源名（统一模式）
);

// 注册到路由（在实现 ModuleRoutes 的 routes() 中）：
// cmx_api_core::register_crud_handlers_module!(router, user_crud, "/users");
```

### 四、handler 内使用 CmxSvrContext 与 db_id 路由

```rust
use axum::Json;
use axum::http::HeaderMap;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, CmxAppState, Result};
use cmx_api_core::db_id::resolve_db_id_from_headers;
use axum::extract::State;

pub async fn my_handler(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,   // mw_context_resolver 注入，提取器直接取用
    headers: HeaderMap,
) -> Result<Json<ApiResp<()>>> {
    // db_id 头存在且非空 → 用它；缺失/空串/非 UTF-8 → 回退业务库
    let db_id = resolve_db_id_from_headers(&headers).await;

    // 细粒度权限检查（非宏生成 handler 手动调用）
    svr_ctx.require_permission("demo:read").map_err(cmx_api_core::Error::from)?;

    // 操作者身份（doc/mdm 等 handler 共用兜底逻辑）
    let _operator = cmx_api_core::actor::actor_name(&CmxSvrContext(svr_ctx));
    Ok(Json(ApiResp::ok(())))
}
```

### 五、装配中间件栈（服务启动场景）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::{
    cors_layer, mw_auth, mw_context_resolver, mw_permission, mw_security_headers,
    GlobalAuthService, GlobalPermissionConfig,
};

// 启动早期：注入全局认证服务 + 白名单（内置 BUILTIN_WHITELIST 与 TOML [auth].whitelist 合并）
GlobalAuthService::initialize(auth_service_impl).expect("认证服务重复初始化");
GlobalAuthService::initialize_whitelist(vec!["/api/public/**".into()]).unwrap();

// 启动早期：注入路由→权限码映射（TOML [iam_permissions] section）
GlobalPermissionConfig::initialize(permission_map).unwrap();

let app = Router::new()
    .merge(/* 各模块路由 */)
    .layer(axum::middleware::from_fn(mw_auth))             // 认证（需在 mw_context 之后）
    .layer(axum::middleware::from_fn(mw_context_resolver)) // 构建 CmxSvrContext
    .layer(axum::middleware::from_fn(mw_permission))       // 权限（在 mw_auth 之后）
    .layer(axum::middleware::from_fn(mw_security_headers)) // 安全响应头
    .layer(cors_layer())                                    // CORS
    .with_state(state);
```

---

## 设计要点

1. **宏内 `$crate` 稳定性**：CRUD 宏以 `$crate::rest::handler::*` / `$crate::Error` 寻址，`#[macro_export]` 导出到 crate 根，调用方无需 `use` 对应模块；ApiResp/Error 的 re-export 保证宏在任意 crate 展开后仍解析到本 crate 路径。
2. **双层结构体模式**（宏设计原则）：handler 函数签名使用 cmx-core 运行时参数类型（`PageParams<F>` / `UpdatePayload<E>`），utoipa 注解使用文档类型（`PageParamsDoc` 等），签名与文档解耦。
3. **白名单通配符语义**：普通规则前缀匹配（隐式 `**` 后缀，向后兼容）；`*` 匹配单层路径段（不含 `/`）；`**` 匹配多层；正则元字符自动转义。规则在启动时一次性编译为 `Prefix | Regex` 枚举。
4. **mw_auth 的 Bearer 语义**：`Authorization: Bearer <jwt>` 只承载终端用户 JWT，服务 key 只走 `X-API-Key`；`X-Delegated-User-Token` 支持 on-behalf-of 委托；query `access_token` 仅为 EventSource（SSE）兜底。
5. **两个 db_id 解析器的差异**：`rest::header_parse::get_db_id_from_header` 回退**默认库**（通用 CRUD 用）；`db_id::resolve_db_id_from_headers` 回退**业务库**（`get_biz_db_id`，doc/dct/code/mdm 等域用）。新代码统一走后者。
