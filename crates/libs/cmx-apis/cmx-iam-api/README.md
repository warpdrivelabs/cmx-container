# cmx-iam-api

> cmx 平台 IAM（用户/角色/角色组/权限/互斥规则）与认证（登录/OAuth2/API Key）域的 HTTP 皮肤层：薄 axum handler 委托 cmx-iam / cmx-auth 服务，只做参数提取与响应封装。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-iam-api` 是 IAM 域的 HTTP 适配层，覆盖两大块：**认证**（账号会话 / 内置 OAuth2 授权服务器 / 第三方 OAuth2 Provider 接入 / API Key / OAuth2 客户端管理）与 **IAM 管理**（用户 / 角色 / 角色组 / 权限 / 职责互斥规则，含临时角色授权与审计查询）。

### 皮肤 vs 领域：职责分工

按 AGENTS.md 第八章规范，本 crate 是**纯 HTTP 适配层**：

| 关注点 | 本 crate（皮肤） | cmx-iam / cmx-auth（领域） |
|--------|----------------|---------------------------|
| 参数提取 / 校验 | axum extractor（`Json` / `Query` / `Path`） | — |
| 请求 / 响应 DTO | `request.rs` / `response.rs`（auth 模块）、handler 内局部 DTO | 跨 crate 共享 DTO 在 `cmx-core/src/model/` |
| 业务逻辑 | 无（注入式 Service 模式：`cmx_state.iam()?.user_service.xxx()`） | `UserService` / `RoleService` / `PermissionService` 等 trait + 实现 |
| Entity / Filter / BMC | `use cmx_iam::user::{UserFilter, UserForCreate, ...}` 引用，禁止重定义 | 全部领域模型定义 |
| OpenAPI | `#[utoipa::path]` 注解 + `IamApiDoc` 切片 | `openapi` feature 提供 `ToSchema` 派生 |

### 注入式 Service 模式

IAM 域采用规范中的「注入式」调用方式：`IamState` 由 cmx-api-core 持有（过渡期设计——服务 crate 不依赖 cmx-api-core，故不成环），handler 经 `cmx_state.iam()` 取得聚合并调用对应 service；认证相关则经 `cmx_state.auth_service()` 或 `GlobalAuthService` 全局态访问。OAuth2 授权服务器与第三方 Provider 经 `GlobalAuthService::get_oauth2()` / `get_provider_registry()` 获取策略对象。

### 现状注记（backlog）

按规范 8.1 的警示：`auth/api_key_handler` 与 `auth/oauth2_client_handler` 直接调用 `cmx_iam::api_key::store`（存储层函数）且 API Key 生成/哈希逻辑写在 handler 内，属于历史违规渗入，列为 backlog；新增代码不得沿用。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | `CmxAppState` / `IamState` / `CmxSvrContext` / `ApiResp` / `Error` / `ModuleRoutes` / `GlobalAuthService` |
| `cmx-api-types` | 响应信封与错误类型源头 |
| `cmx-iam`（openapi feature） | `UserService` / `RoleService` / `RoleGroupService` / `PermissionService` / `ExclusionRuleService` service trait、`UserForCreate` / `UserFilter` 等 DTO、`api_key::store`、`service_traits`（临时授权 / 审计响应类型） |
| `cmx-auth` | `OAuth2Policy` / `OAuth2ProviderRegistry` / `BUILTIN_WHITELIST`（经 GlobalAuthService 间接使用） |
| `cmx-buffer` | `GlobalCacheManager`（API Key 变更后的 Redis 两层缓存失效） |
| `cmx-core`（openapi feature） | `User` / `Role` 实体模型、`UpdatePayload` / `DeletePayload` / `PageParams` |
| `cmx-traits` | `AuthService` / `Credentials` / `DeviceInfo` / `AuthError` |
| `cmx-utils` / `axum` / `serde` / `serde_json` / `tracing` / `chrono` / `uuid` / `utoipa` | 常规 Web / 序列化 / 文档依赖 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-iam-api = { workspace = true }` | 平台总装配器：`routes()` 中 `.merge(AuthModule.routes())` / `.merge(IamModule.routes())` 挂载全部端点；`merged_openapi()` 中 `doc.merge(IamApiDoc::openapi())` 聚合文档切片 |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，随总装配器间接获得 IAM/Auth 全部 HTTP 端点 |
| `cmx-flowengine`（跨 workspace） | 不直接依赖 | 流程微服务独立 workspace，仅引 cmx-core / cmx-database-pg / cmx-web-chassis 等基础设施 |

---

## 核心功能与特性（路由端点分组）

所有路由挂 `/api` 前缀下（`AuthModule` nest 于 `/auth`，`IamModule` 各子模块路径内建 `/iam/...`）。

### AuthModule（`/api/auth`，认证）

| 分组 | 端点 | 说明 |
|------|------|------|
| 账号会话 | `POST /auth/login` | 账号密码登录，签发 access / refresh token（失败映射 401/403/429） |
| | `POST /auth/refresh` | refresh token 换新 access token |
| | `POST /auth/logout` | 登出并吊销当前 token（从 Authorization 头提取） |
| | `GET /auth/me` | 取当前登录用户信息 |
| | `POST /auth/validate` | 校验 token 有效性（供网关 / 第三方调用） |
| | `POST /auth/revoke-all` | 吊销该用户全部 token（强制全端下线） |
| | `POST /auth/heartbeat` | 心跳续期（滑动窗口刷新活跃时间） |
| | `POST /auth/change-password` | 修改当前用户密码 |
| | `GET /auth/health` | 认证服务探活（无需鉴权） |
| 内置 OAuth2 授权服务器（IdP） | `GET /auth/oauth2/authorize` | 授权码端点：重定向确认页 / 直接发码 |
| | `POST /auth/oauth2/login` | OAuth2 登录提交（账号密码 + 授权确认） |
| | `POST /auth/oauth2/token` | token 端点：换 access / refresh token |
| 第三方 OAuth2 Provider（RP） | `GET /auth/oauth2/providers` | 列出已配置的第三方 Provider |
| | `GET /auth/oauth2/provider/{provider}/authorize` | 跳转指定 Provider 授权页 |
| | `GET /auth/oauth2/{provider}/callback` | Provider 授权回调（code 换 token） |
| | `POST /auth/oauth2/provider/exchange` | 第三方 token 换平台 token（绑定 / 静默登录） |
| | `POST /auth/oauth2/provider/{provider}/link` | 绑定当前用户与第三方账号 |
| | `DELETE /auth/oauth2/provider/{provider}/unlink` | 解除绑定 |
| API Key 管理 | `POST /auth/api-keys/create` | 创建 API Key（明文仅创建时返回一次） |
| | `GET /auth/api-keys/list` | 列出当前用户 API Key |
| | `POST /auth/api-keys/delete` | 删除 API Key |
| | `POST /auth/api-keys/toggle-status` | 启用 / 禁用 API Key |
| OAuth2 客户端管理 | `POST /auth/oauth2-clients/create` | 注册接入本平台的第三方应用 |
| | `GET /auth/oauth2-clients/list` | 列出 OAuth2 客户端 |
| | `POST /auth/oauth2-clients/update` | 更新客户端配置 |
| | `POST /auth/oauth2-clients/delete` | 删除客户端 |

### IamModule（`/api/iam`，身份与访问管理）

| 分组 | 端点 | 说明 |
|------|------|------|
| 用户 CRUD | `POST /iam/users/create` / `update` / `delete`，`GET /iam/users/get`（按 username），`POST /iam/users/page` / `list` | 自定义 handler（UserForCreate/ForUpdate 不 derive Fields，需 Service 转换） |
| 用户-角色关联 | `POST /iam/users/assign-roles`（覆盖式）、`GET /iam/users/roles` | 分配 / 查询用户角色 |
| 临时角色授权 | `POST /iam/users/assign-temp-role` / `revoke-temp-role` / `revoke-temp-roles-batch` / `extend-temp-role`，`GET /iam/users/temp-assignments`，`GET /iam/roles/temp-assigned-users` | 带时效授权，到期由后台任务自动失效 |
| 角色 CRUD | `POST /iam/roles/create` / `update` / `delete`，`GET /iam/roles/get`，`POST /iam/roles/page` / `list` | 统一风格的自定义 handler |
| 角色-权限 / 角色-用户 | `POST /iam/roles/assign-permissions`（覆盖式）、`POST /iam/roles/assign-users`、`GET /iam/roles/permissions`、`GET /iam/roles/users` | 批量赋权与用户分配 |
| 角色组 | `/iam/role-groups/` 下 create / update / delete / get / page / list / `tree` / `combined-tree` | 组树与含角色计数的组合树 |
| 权限 | `/iam/permissions/` 下 create / update / delete / get / page / list / `tree` | 权限定义与权限树 |
| 权限数据集成 | `POST /iam/permissions/import`、`POST /iam/permissions/cleanup` | 插件数据导入 / 清理（权限中心数据集成） |
| 互斥规则 | `/iam/exclusion-rules/` 下 create、`update/{rule_id}`、`delete/{rule_id}`、`get/{rule_id}`、`page`、`toggle-status`、`items/add`、`items/remove`、`validate` | 职责分离（SoD）互斥规则管理 |
| 审计查询 | `GET /iam/users/effective-permissions`、`GET /iam/roles/permission-diff`、`GET /iam/permissions/usage-stat` | 有效权限合并查询 / 角色权限差异 / 权限使用统计 |

---

## 模块结构

```text
cmx-iam-api
├── src
│   ├── lib.rs                              # 模块导出（IamApiDoc / AuthModule / IamModule）
│   ├── openapi.rs                          # IamApiDoc：本域 OpenApi 切片（paths + schemas）
│   └── handlers
│       ├── mod.rs                          #   子模块声明（auth / iam）
│       ├── auth
│       │   ├── mod.rs                      #   AuthModule 路由聚合（nest "/auth"）+ 内部路由表
│       │   ├── handler.rs                  #   登录 / 刷新 / 登出 / me / validate / revoke-all / heartbeat / 改密 / health
│       │   ├── api_key_handler.rs          #   API Key 创建 / 列表 / 删除 / 启停（含 Redis 缓存失效）
│       │   ├── oauth2_handler.rs           #   内置 OAuth2 授权服务器（authorize / login / token）
│       │   ├── oauth2_client_handler.rs    #   OAuth2 客户端 CRUD
│       │   ├── oauth2_provider_handler.rs  #   第三方 Provider 接入（跳转 / 回调 / 兑换 / 绑定 / 解绑）
│       │   ├── oauth2_request.rs           #   OAuth2 请求 DTO
│       │   ├── oauth2_response.rs          #   OAuth2 响应 DTO
│       │   ├── request.rs                  #   认证请求 DTO（LoginRequest / RefreshRequest / ValidateRequest...）
│       │   └── response.rs                 #   认证响应 DTO（LoginResponse / ValidateResponse...）
│       └── iam
│           ├── mod.rs                      #   IamModule：merge 五个子模块路由
│           ├── user
│           │   ├── mod.rs                  #     UserModule 路由（CRUD + 关联 + 临时角色 + 审计）
│           │   ├── handler.rs              #     用户 CRUD / assign-roles / get_user_roles
│           │   ├── temp_role_handler.rs    #     临时角色授予 / 撤销 / 批量撤销 / 延期 / 查询
│           │   └── audit_handler.rs        #     有效权限审计查询
│           ├── role
│           │   ├── mod.rs                  #     RoleModule 路由
│           │   ├── handler.rs              #     角色 CRUD / 赋权 / 分配用户 / 权限与用户查询
│           │   └── audit_handler.rs        #     角色权限差异查询
│           ├── role_group
│           │   ├── mod.rs                  #     RoleGroupModule 路由
│           │   └── handler.rs              #     角色组 CRUD / 组树 / 组合树
│           ├── permission
│           │   ├── mod.rs                  #     PermissionModule 路由
│           │   ├── handler.rs              #     权限 CRUD / 权限树
│           │   ├── audit_handler.rs        #     权限使用统计
│           │   └── import_handler.rs       #     权限导入 / 清理（插件数据集成）
│           └── rule
│               ├── mod.rs                  #     RuleModule 路由（路径参数 {rule_id}）
│               └── handler.rs              #     互斥规则 CRUD / 启停 / 互斥对象管理 / 校验测试
└── Cargo.toml
```

---

## 关键类型 / API

### 模块路由注册器（lib.rs 顶层导出）

```rust
pub struct AuthModule;   // impl ModuleRoutes：Router::new().nest("/auth", inner_routes())
pub struct IamModule;    // impl ModuleRoutes：merge User/Role/RoleGroup/Permission/Rule 五个子模块

// prefix()： "auth" / "iam"；module_name()： "auth" / "iam"
```

### OpenAPI 切片

```rust
#[derive(OpenApi)]
pub struct IamApiDoc;   // 收录约 60 个 #[utoipa::path] + 全部请求/响应 schema，
                        // 由 cmx-platform-app 用 OpenApi::merge() 聚合进总文档
```

### 典型 handler 签名（注入式 Service 调用）

```rust
// 会话类：经 cmx_state.auth_service()（Arc<dyn AuthService>）
pub async fn auth_login(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResp<LoginResponse>>>;

// IAM 管理类：经 cmx_state.iam()?.user_service（Arc<dyn UserService>）
pub async fn create_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(data): Json<UserForCreate>,
) -> Result<Json<ApiResp<User>>>;

// 临时授权类：DTO 本地定义（AssignTempRoleRequest 等，derive ToSchema）
pub async fn assign_temp_role(/* ... */) -> Result<Json<ApiResp<serde_json::Value>>>;

// 第三方 OAuth2：经 GlobalAuthService::get_provider_registry()
pub async fn oauth2_provider_callback(/* Path(provider) + Query */);
```

---

## 使用示例

### 一、cmx-platform-app 合并 IAM 路由与文档（组装场景）

```rust
use cmx_iam_api::{AuthModule, IamApiDoc, IamModule};
use utoipa::OpenApi;

// 路由合并：AuthModule nest 于 /auth，IamModule 聚合五个子模块
let router = Router::new()
    .merge(AuthModule.routes())
    .merge(IamModule.routes());

// OpenAPI 聚合：以主文档为基底逐域 merge 切片
let mut doc = ApiDoc::openapi();
doc.merge(IamApiDoc::openapi());
```

### 二、典型 IAM handler：注入式 Service 协作（摘自 user/handler.rs 模式）

```rust
use axum::Json;
use axum::extract::State;
use cmx_core::model::iam::User;
use cmx_iam::user::UserForCreate;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, CmxAppState, Error, Result};

pub async fn create_user(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,   // 认证/追踪上下文（mw_auth + mw_context 注入）
    Json(data): Json<UserForCreate>,          // DTO 定义在 cmx-iam，皮肤层只引用
) -> Result<Json<ApiResp<User>>> {
    // 1. 从共享状态取 IAM 聚合；未初始化映射为业务错误
    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    // 2. 注入式调用：UserForCreate 不 derive Fields，须由 Service 层转换写库
    let user = iam
        .user_service
        .create_user(&svr_ctx, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    // 3. 统一信封封装
    Ok(Json(ApiResp::ok(user)))
}
```

### 三、登录错误分级映射（摘自 auth/handler.rs 模式）

```rust
use cmx_traits::auth::{AuthError, Credentials, DeviceInfo};
use cmx_api_core::{ApiResp, Error};

// Handler 内对 AuthError 分级：可预期失败返回业务码，风控触发 HTTP 429
match auth_service.authenticate(credentials, Some(device_info)).await {
    Ok(pair) => Ok(Json(ApiResp::ok(pair_to_login_response(pair)))),
    Err(AuthError::InvalidCredentials) => Ok(Json(ApiResp::fail(401, "未授权: 用户名或密码错误"))),
    Err(AuthError::UserDisabled) => Ok(Json(ApiResp::fail(403, "用户已被禁用"))),
    Err(AuthError::TooManyAttempts { secs, limit, window }) => {
        Err(Error::RateLimitExceeded { retry_after: secs, limit: limit as u64, window })
    }
    Err(other) => Err(Error::business_error(other.to_string())),
}
```

---

## 设计要点

1. **双 Module 拆分**：认证（`AuthModule`，nest `/auth`）与 IAM 管理（`IamModule`，路径内建 `/iam/...`）独立成模块，白名单只需放行 `/api/auth` 下的登录/发码端点，`/api/iam/**` 全部强制鉴权。
2. **审计与临时授权为独立 handler 文件**：`audit_handler.rs`（三处审计查询）与 `temp_role_handler.rs`（五类临时授权操作）从主 CRUD handler 中拆出，对应演进阶段（阶段 1 临时授权 / 阶段 5 审计）的增量能力。
3. **互斥规则用路径参数**：`update/{rule_id}`、`delete/{rule_id}`、`get/{rule_id}` 携带路径参数，是本 crate 内少数使用 `Path` 提取器的端点（其余遵循 POST + JSON body 规范）。
4. **API Key 明文只出现一次**：创建时生成并以响应返回，库内仅存哈希；启停 / 删除后主动失效 Redis 两层缓存（`auth:api_key:*`），保证多实例即时生效。

---

## 常见问题

### Q1: 为什么 IamState 定义在 cmx-api-core 而不是本 crate？

**A**: 过渡期设计（Strategy 2）。服务 crate（cmx-iam）不依赖 cmx-api-core，`IamState` 放在 cmx-api-core 只是让骨架层持有具体类型，依赖方向仍是「本 crate → cmx-api-core + cmx-iam」单向边，不成环；待阶段 4 trait 化后可移除。handler 经 `cmx_state.iam()` 访问，不受此影响。

### Q2: 登录失败为什么有的返回 200 + body code，有的返回 HTTP 错误？

**A**: 用户名密码错误 / 账号禁用属「可预期业务结果」，走 `ApiResp::fail(401/403, ...)`（HTTP 200 + 业务码）便于前端统一解包；而风控限流（`TooManyAttempts`）需要携带 `retry_after` 等 HTTP 语义，走 `Error::RateLimitExceeded` 映射为 HTTP 429。

### Q3: 临时角色到期后如何自动失效？

**A**: 到期清理由 cmx-iam 后台调度任务完成，本 crate 只负责授予 / 撤销 / 延期 / 查询的 HTTP 入口；查询有效权限时 `get_effective_permissions` 已按时间窗合并「永久 + 生效中的临时」授权。

### Q4: 哪些端点在认证白名单内？

**A**: 白名单由 `cmx_auth::config::BUILTIN_WHITELIST` 与 TOML `[auth].whitelist` 合并（支持 `*` / `**` 通配符），登录、OAuth2 发码 / token、`/auth/health` 等无需携带凭证；具体清单以 cmx-auth 配置为准，本 crate 不自行维护名单。
