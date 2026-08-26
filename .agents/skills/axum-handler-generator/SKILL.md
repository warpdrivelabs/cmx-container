---
name: axum-handler-generator
description: 当用户要求生成/编写/新增 axum REST 接口 handler、HTTP API、CRUD 接口、列表/分页查询接口，或涉及 cmx-apis 域皮肤 crate（cmx-biz-api / cmx-iam-api / cmx-plugin-api 等）的路由注册、ModuleRoutes、declare_crud_handlers 宏、utoipa 注解、ApiResp 响应封装时必用。触发关键词：axum、rest、接口、handler、生成接口、新增接口、路由、CRUD、分页查询、列表查询。
---

# axum REST Handler 开发规范（cmx-apis）

> 本技能指导在 cmx-container 中生成 HTTP handler 代码。原 cmx-api 单体已拆分为
> `crates/libs/cmx-apis/` 下 8 个 crate（提交 2657bdad），本文全部路径基于新架构。
> 生成任何 handler 前，先按第四章决策树定路径，再查 references 对应模板。

---

## 一、架构地图：cmx-apis 8 crate 分工

所有路径相对于 `crates/libs/cmx-apis/`：

| crate | 职责 | 典型 handler |
|-------|------|-------------|
| `cmx-api-core` | **共享骨架**：`CmxAppState`、`ModuleRoutes` trait、通用 CRUD handler（`rest/handler.rs`）、CRUD 宏（`routes/macros.rs`）、全套中间件（`middleware/`） | 无业务 handler，纯骨架 |
| `cmx-api-types` | 通用 HTTP 类型：`ApiResp` / `Error` / `Pagination` / `*Doc` 文档类型 / `TreeNode` | — |
| `cmx-common-api` | **装配中枢**（原 cmx-api 重命名）：re-export 骨架；保留 portal / service / debug / dev 四组 handler 与路由聚合 | `src/handlers/portal/` |
| `cmx-iam-api` | IAM + 认证域皮肤（user / role / role_group / permission / rule / auth），委托 cmx-iam / cmx-auth | `src/handlers/iam/user/handler.rs` |
| `cmx-biz-api` | 业务基础模型域（domain / application / module / menu / sys_datasource / form），委托 cmx-biz；含宏集中调用 `src/crud_handlers.rs` 与 `src/openapi.rs` | `src/handlers/application/handler.rs` |
| `cmx-plugin-api` | 插件域皮肤（plugin / marketplace / module 包 / table_metadata），委托 cmx-plugin | `src/handlers/{marketplace,table_metadata}/` |
| `cmx-ai-api` | AI 中继薄代理 | `src/handler.rs` |
| `cmx-storage-api` | 文件存储纯路由胶水（不写 handler，装配 cmx-storage 的函数） | `src/lib.rs` |

**依赖方向（无环）**：域 `*-api` crate → `cmx-api-core` + `cmx-api-types` + 对应业务 crate（cmx-biz / cmx-iam / cmx-plugin / ...）；业务 crate **不反向**依赖 api crate；`cmx-common-api` re-export 骨架供旧引用方过渡。

**关键基础设施位置**：

| 内容 | 真实路径 |
|------|---------|
| CRUD 宏 `declare_crud_handlers!` / `register_crud_handlers_module!` | `cmx-api-core/src/routes/macros.rs`（`#[macro_export]` 到 crate 根，用法 `cmx_api_core::declare_crud_handlers!`） |
| `ModuleRoutes` trait | `cmx-api-core/src/routes/traits.rs`（re-export 为 `cmx_api_core::ModuleRoutes`） |
| 8 个通用 CRUD Handler（create / create_many / get_by_id / update / update_many / delete / list / page） | `cmx-api-core/src/rest/handler.rs` |
| `CmxAppState` / `CmxSvrContext` | `cmx-api-core/src/app_state.rs` / `cmx-api-core/src/middleware/mw_context.rs` |
| `ApiResp` / `Error` / `Result` | `cmx-api-types/src/{api_response,error}.rs`（`cmx_api_core` 已 re-export，域 crate 直接 `use cmx_api_core::{ApiResp, Error, Result}`） |
| `PageParams` / `ListParams` / `GetParams` / `UpdatePayload` / `DeletePayload` | `cmx-core/src/model/data/request/params.rs`（经 lib.rs 顶层 re-export，用法 `cmx_core::PageParams`） |

---

## 二、职责边界：`*-api` crate 是纯 HTTP 适配层

**只保留**：Handler 薄层（HTTP → Service → HTTP）、路由注册（`ModuleRoutes` + 宏）、
本模块专用 Request/Response DTO（带 `ToSchema`）、utoipa 注解（`openapi.rs`）。

**禁止包含**（归业务 crate）：Entity / BMC / Filter / Service / modql 定义。

| 禁止内容 | 正确位置 |
|---------|---------|
| Entity（含 ForCreate / ForUpdate） | `<业务 crate>/src/<module>/entity.rs` |
| BMC（表映射） | `<业务 crate>/src/<module>/bmc.rs` |
| Filter（过滤器） | `<业务 crate>/src/<module>/filter.rs` |
| Service（业务逻辑） | `<业务 crate>/src/<module>/service.rs` |

跨 crate 共享 DTO（≥2 个不同 crate 含 WASM 使用）下沉到 **`cmx-core/src/model/`**
（`model/` 下的子域目录，勿臆造其他目录名）。
判定：仅本模块用 → `<handler>/request.rs`；cmx-api + 业务 crate + WASM 共用 → `cmx-core`。

> ⚠️ 现状有违规渗入（`cmx-common-api` 的 portal / model_center、`cmx-iam-api` 的
> auth/api_key 等 handler 内手写 SQL），列为 backlog，新增代码不得沿用。

---

## 三、HTTP 方法与路由规范

### 3.1 方法约定（硬约束）

| 操作 | 方法 | 参数提取 |
|------|------|---------|
| 查询单条（get_by_id） | **GET** | `Query<cmx_core::GetParams>` |
| 创建 / 批量创建 | **POST** | `Json<E>` / `Json<Vec<E>>` |
| 更新 / 批量更新 | **POST** | `Json<cmx_core::UpdatePayload<E>>` / `Json<Vec<...>>` |
| 删除 | **POST** | `Json<cmx_core::DeletePayload>` |
| 列表 / 分页查询 | **POST** | `Json<cmx_core::ListParams<F>>` / `Json<cmx_core::PageParams<F>>` |

除 `get_by_id` 用 GET 外**一律 POST + application/json**。
与根目录 AGENTS.md 全局规则 5 叠加生效：新接口**禁用可变路径段**（路径只允许固定资源段，
资源标识 / 过滤 / 操作参数走 query 或 body）、**禁用 PUT/PATCH/DELETE**、更新删除一律 POST。

### 3.2 路径约定

每个操作**独立路径**，禁止不同操作共享同一路径靠方法区分：

```
/xxx/create  /xxx/create-many  /xxx/get(GET)  /xxx/update  /xxx/update-many
/xxx/delete  /xxx/list  /xxx/page
```

### 3.3 路由注册

- 所有 handler 模块**必须**实现 `ModuleRoutes` trait（`cmx_api_core::ModuleRoutes`），
  三个方法：`routes(self) -> Router<CmxAppState>` / `prefix() -> &'static str` / `module_name(&self) -> &'static str`。
- `declare_crud_handlers!` 宏**仅限各 `*-api` crate 内部使用**（AGENTS.md §八）。
- 各域 crate 的 Module 由 **cmx-platform-app**（`crates/libs/cmx-platform-app/src/routes.rs`，
  `.merge(XxxModule.routes())` 链）合并进主路由；OpenAPI 切片（各域 `src/openapi.rs`
  的 `XxxApiDoc`）同样由 platform-app `OpenApi::merge()` 聚合。

---

## 四、决策树：新接口怎么写

```
要新增一个 HTTP 接口
│
├─ 是标准单表 CRUD 且无业务副作用？
│   ├─ 是 → 用 declare_crud_handlers! 宏（cmx-biz-api/src/crud_handlers.rs 集中声明）
│   │       + register_crud_handlers_module! 注册路由
│   │       ⚠️ 写操作需触发副作用（DAM 资产搬移/树形字段维护/引用校验）时禁用宏，
│   │          必须手写 handler 委托 Service（参考 domain/application/menu 的手写原因）
│   └─ 否（自定义查询/业务操作）→ 手写 handler
│
├─ 业务类型归属哪个 crate？
│   ├─ Entity/BMC/Filter/Service 未定义 → 先到业务 crate 定义（设计 Filter 前先调 modql 技能）
│   └─ 已定义 → *-api crate 内 use 引用，禁止重新定义
│
├─ Service 是哪种模式？
│   ├─ 静态模式（cmx-biz 等）：XxxService::create(mm, &db_id, ...)
│   │   → handler 内 get_default_db_manager() + get_db_id_from_header(&headers)
│   └─ 注入式模式（cmx-iam 等）：cmx_state.iam()?.user_service.create_user(&svr_ctx, ...)
│       → handler 不拿 mm/db_id（Service 内部持有）
│
└─ 涉及 list / page？
    → 遵守第五章契约 + references/service-patterns.md 最佳实践
```

---

## 五、核心契约：list / page 的 request_body 与签名（统一口径）

> 此口径与 cmx-container/AGENTS.md §七完全一致，是本技能最高优先级规范。

**utoipa 注解的 `request_body` 必须用 `serde_json::Value` 作泛型；函数签名必须用具体 Filter 类型**：

```rust
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = cmx_core::PageParams<serde_json::Value>,   // 注解用 Value
    responses((status = 200, description = "查询成功", body = ApiResp<DataSet>)),
    tag = "Xxx"
)]
pub async fn xxx_page(
    /* ... */
    Json(params): Json<cmx_core::PageParams<XxxFilter>>,      // 签名用具体 Filter
) -> Result<Json<ApiResp<DataSet>>> { /* ... */ }
```

| 操作 | `request_body` 注解 | 函数签名 |
|------|--------------------|---------|
| list | `cmx_core::ListParams<serde_json::Value>` | `Json<ListParams<XxxFilter>>` |
| page | `cmx_core::PageParams<serde_json::Value>` | `Json<PageParams<XxxFilter>>` |
| create | `XxxForCreate`（具体类型，需 `ToSchema`） | `Json<XxxForCreate>` |
| update | `cmx_core::UpdatePayload<XxxForUpdate>` | `Json<UpdatePayload<XxxForUpdate>>` |
| delete | `cmx_core::DeletePayload` | `Json<DeletePayload>` |

原因：modql 的 `FilterNodes` 不支持 `ToSchema`，`Value` 仅用于文档生成，运行时签名
用具体 Filter 正常反序列化。`Value` 禁止扩散到函数签名。

**Service 端契约**：`list` / `page` 方法必须接收
`filters: Option<Vec<XxxFilter>>` + `list_options: ListOptions` 两个结构化参数，
禁止平铺 `(page, page_size, keyword, ...)`。细节见 `references/service-patterns.md`。

---

## 六、标准工作流（新增一个实体的接口）

1. **业务 crate 定义类型**：`<业务 crate>/src/<module>/{entity,bmc,filter,service}.rs`
   —— Filter 字段用 `Option<OpValsXxx>`、Entity 派生 `Fields`（设计前先调 modql 技能）。
2. **选域皮肤 crate**：按第一章表格确定（业务模型 → cmx-biz-api；IAM → cmx-iam-api；插件 → cmx-plugin-api）。
3. **创建 handler 模块**：`src/handlers/<module>/{mod.rs, handler.rs[, request.rs, response.rs]}`，
   或走宏路径在本域 `crud_handlers.rs` 声明。
4. **实现 ModuleRoutes**：mod.rs 注册路由；聚合到上层 Module。
5. **注册 OpenAPI**：本域 `src/openapi.rs` 的 paths/components 加条目。
6. **验证**：`cargo check -p <域 crate>`；改了公用库时按 AGENTS.md 要求在
   cmx-portalservice / cmx-flowengine 两个下游 ws 各跑 `cargo check`。

模板与逐步代码见 `references/handler-templates.md`。

---

## 七、错误处理与响应

- 统一响应：`ApiResp::ok(data)` / `ApiResp::ok(())` / 分页 `ApiResp::ok_with_pagination(data, page, size, total)`。
- Handler 返回 `Result<Json<ApiResp<T>>>`（`cmx_api_core::Result`）。
- 错误构造：`Error::business_error("...")`（业务错误）、`Error::InternalError("...")`（内部错误）；
  业务 crate 错误用 `.map_err(|e| Error::business_error(e.to_string()))?` 转换。
- 分页响应 JSON：`{ code, msg, data, pagination: { page, pageSize, total, totalPages } }`。

---

## 八、技能协同（互链）

- **modql 技能**：设计 Filter（OpVals 类型 / FilterNodes / `#[modql(rel)]` 表别名）、
  Entity `Fields`、`ListOptions` / `order_bys` 语义 → 写 list / page 前先调。
- **cmx-sql-execution 技能**：Service 内手写 SQL、`DataValue` 参数构造（`dv!` / ParamsBuilder /
  NullTyped）、事务 → 绕过 GenericCrudService 手写 SQL 前先调。
- **pg-table-generator / sql-guide**：新建表 DDL / SQL 迁移文件。
- 分工：本技能管 **HTTP 协议层**（handler / utoipa / 路由），modql 管查询语义层，
  cmx-sql-execution 管 SQL 执行层；各技能触发表详见其 SKILL.md，不在此复述。

---

## 九、references 索引

| 文件 | 内容 | 何时读 |
|------|------|--------|
| `references/handler-templates.md` | import 清单、Handler 全套模板（create/get/update/delete/list/page/JOIN 分页/默认 filter 注入/注入式 Service/全局单例）、utoipa 注解规范、CRUD 宏三种权限模式与宏 vs 手写决策、ModuleRoutes/路由聚合模板、request.rs DTO 规范、openapi.rs 注册 | 写任何 handler 代码前 |
| `references/service-patterns.md` | 两种 Service 模式对比与完整示例、list/page 最佳实践（三步提取/标准签名/反模式）、ListOptions 前端 JSON 约定、Entity/BMC/Filter 定义要点、DTO 归属决策 | 写 Service 或 list/page 接口前 |

---

## 十、关键参考文件（真实路径）

| 场景 | 参考文件 |
|------|---------|
| 静态 Service 单表 page / list（标准） | `crates/libs/cmx-apis/cmx-biz-api/src/handlers/application/handler.rs` |
| 静态 Service 多表 JOIN page_custom | 同上 `application_custom_page` |
| 宏生成的标准 CRUD | `crates/libs/cmx-apis/cmx-biz-api/src/crud_handlers.rs` + `handlers/form/mod.rs` + `handlers/sys_datasource/mod.rs` |
| 注入式 Service CRUD + page / list | `crates/libs/cmx-apis/cmx-iam-api/src/handlers/iam/user/handler.rs` |
| 注入式模块聚合 | `crates/libs/cmx-apis/cmx-iam-api/src/handlers/iam/mod.rs` |
| 默认 filter 注入（多租户 app_id） | `crates/libs/cmx-apis/cmx-plugin-api/src/handlers/table_metadata/handler.rs` |
| 外部 Service（marketplace） | `crates/libs/cmx-apis/cmx-plugin-api/src/handlers/marketplace/handler.rs` |
| OpenAPI 切片注册 | `crates/libs/cmx-apis/cmx-biz-api/src/openapi.rs` |
| 通用 CRUD Handler（宏内部委托） | `crates/libs/cmx-apis/cmx-api-core/src/rest/handler.rs` |
| CRUD 宏定义 | `crates/libs/cmx-apis/cmx-api-core/src/routes/macros.rs` |
| 静态 Service 实现 | `crates/libs/cmx-biz/src/datasource/service.rs`、`crates/libs/cmx-biz/src/domain/service.rs` |
| 静态 Service + JOIN | `crates/libs/cmx-biz/src/application/service.rs` |
| 注入式 Service 实现 | `crates/libs/cmx-iam/src/user/service.rs` |
| 参数类型真源 | `crates/libs/cmx-core/src/model/data/request/params.rs` |

---

## 十一、自检清单

生成或审查 handler 代码时逐条核对：

- [ ] handler 在正确的域 `*-api` crate 内，未在 api crate 重新定义业务类型？
- [ ] 除 GET 查单条外全部 POST；路径无 PUT/PATCH/DELETE、无可变路径段、每操作独立路径？
- [ ] list / page 的 `request_body` 用 `serde_json::Value`、签名用具体 Filter？
- [ ] Service 签名是 `filters + list_options` 结构化参数，无平铺？
- [ ] handler 三步提取：`to_list_options()` / `get_page()` / `get_size()` + `filters.filter(\|v\| !v.is_empty())`？
- [ ] `page` 返回带 total 并用 `ApiResp::ok_with_pagination`？
- [ ] 多表 JOIN 时 Filter 字段带 `#[modql(rel = "表别名")]`？
- [ ] 多租户 app_id 默认值在 handler 注入（`cmx_state.app_id()`），不在 Service 硬编码？
- [ ] 实现了 `ModuleRoutes` 并注册到本域 openapi.rs？
- [ ] import 用真实路径（`cmx_api_core::*` / `cmx_core::*` / `cmx_<biz>::*`），无 `crate::api_response` 等旧路径？
- [ ] 标准单表 CRUD 无副作用时用了宏，有副作用（DAM 钩子 / 树形字段）时手写委托 Service？
