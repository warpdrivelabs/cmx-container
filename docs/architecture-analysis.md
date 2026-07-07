# CMX-Container 架构分析文档

> **主题**：单体应用 / 微服务双模运行能力分析
> **审查日期**：2026-07-03
> **审查范围**：cmx-container workspace 全量 crate，聚焦部署模式、数据库路由、服务间通信
> **配套文档**：`docs/deployment-mode-review.md`（评审与优化建议）

---

## 一、工程总览

CMX-Container 是一套「企业业务容器平台」（由 Node.js 迁移至 Rust），本质上是 **一个模块化单体（Modular Monolith）+ 可选 RPC 逃生舱** 的架构。整套系统由 **单一 `web-server` 二进制** 装配而成，通过运行时配置切换是否启用跨进程调用。

### 1.1 Crate 分层

```mermaid
graph TB
    subgraph Web["应用装配层"]
        WS["web-server<br/>(唯一二进制入口)"]
    end
    subgraph API["API 表现层"]
        API1["cmx-api<br/>(路由/中间件/CmxAppState)"]
        API2["cmx-api-types<br/>(ApiResp/OpenAPI)"]
    end
    subgraph Biz["业务域层"]
        IAM["cmx-iam<br/>用户/角色/权限/SoD"]
        PLG["cmx-plugin<br/>插件生命周期/市场/center_client"]
        BIZ["cmx-biz<br/>域/应用/模块/表单/菜单"]
        META["cmx-metadata<br/>表元数据"]
        SVC["cmx-service<br/>服务编排"]
        RT["cmx-runtime<br/>WASM 运行时"]
        DBG["cmx-debug"]
        PRT["cmx-portal 族<br/>(portal/form/model)"]
    end
    subgraph Traits["解耦抽象层"]
        TR["cmx-traits<br/>(auth/iam/plugin/service/rpc/event_bus)"]
    end
    subgraph Infra["基础设施层"]
        DB["cmx-database<br/>(多数据源/连接池)"]
        BUF["cmx-buffer<br/>(Redis/锁/PubSub)"]
        AUTH["cmx-auth<br/>(JWT/OAuth2/会话)"]
        AUD["cmx-audit"]
        STO["cmx-storage"]
        RPC["cmx-rpc<br/>(gRPC/volo)"]
        REG["cmx-registry-config<br/>(Nacos)"]
    end
    subgraph Found["基础层"]
        CORE["cmx-core<br/>(领域模型)"]
        UTL["cmx-utils<br/>(ConfigManager/加密)"]
        MAC["cmx-macros"]
        MQ["modql"]
    end

    WS --> API
    WS --> Biz
    WS --> Infra
    API --> Biz
    Biz --> Traits
    Biz --> Infra
    Infra --> Traits
    Infra --> Found
    Biz --> Found
    Traits --> CORE
```

### 1.2 解耦枢纽：`cmx-traits`

业务 crate 之间 **不直接依赖**，而是通过 `cmx-traits` 定义的 trait 以 `Arc<dyn Trait>` 形式交互。这是整个工程最关键的设计决策，也是双模切换的基石。

| Trait | 本地实现 | 远程实现 | 配置切换点 |
|-------|---------|---------|-----------|
| `ResourceDataImporter` | `cmx-biz::ResourceDataImporterImpl` | gRPC `CmxResourceDataService` | `center_client.mode` |
| `DefinitionImporterBundle`<br/>(Form/Menu/Table/Perm) | `Local*DefinitionImporter` | `remote_importers` (grpc/http) | `web-server/config/iam.rs:128-174` |
| `ServiceInvoker` | `cmx-service` | `ServiceOrchestrationClient` (gRPC) | `[rpc] enabled` |
| `FunctionInvoker` | `cmx-biz::BizFunctionInvoker` | 同上 gRPC | 同上 |
| `AuthService` | `cmx-auth::AuthServiceImpl` | ❌ **无** | — |
| `UserAuthQuery` | `cmx-iam::UserAuthQueryImpl` | ❌ **无** | — |

### 1.3 功能域归属

| 功能域 | 路由模块 | 承载 crate |
|--------|---------|-----------|
| 认证 (Auth) | `handlers::auth` | `cmx-auth` |
| 权限/用户/角色 (IAM) | `handlers::iam` | `cmx-iam` |
| 域/应用/模块 | `handlers::domain/application/module` | `cmx-biz` |
| 数据源 | `handlers::sys_datasource` | `cmx-biz` + `cmx-database` |
| 表单/菜单 | `handlers::form/menu` | `cmx-biz` |
| 插件生命周期/市场 | `handlers::plugin/marketplace` | `cmx-plugin` |
| 表元数据 (meta_table) | `handlers::table_metadata` | `cmx-metadata` |
| 服务编排 | `handlers::service` | `cmx-service` |
| 调试/WASM 调用 | `handlers::debug` | `cmx-debug` |
| Portal/设计器 | `handlers::portal` | `cmx-portal` 族 |

---

## 二、单体运行模式（Monolithic）

### 2.1 运行特征

- **单进程**：仅 `web-server` 一个二进制，所有功能域编译进同一进程。
- **配置标志**：`[center_client] mode = "local"`（默认）+ `[rpc] enabled = false`。
- **调用方式**：所有跨域调用走 **进程内 `Arc<dyn Trait>` 直接派发**，零网络开销。
- **数据库**：默认库与业务库指向 **同一物理数据库**（`primary` 与 `biz` 的 `db_url` 相同）。

### 2.2 单体运行时序图

```mermaid
sequenceDiagram
    autonumber
    participant Client as "客户端"
    participant Axum as "Axum Router (web-server)"
    participant MW as "中间件链 (cookie-ctx-auth-perm)"
    participant Handler as "cmx-api Handler"
    participant Trait as "Arc<dyn Trait> (CmxAppState)"
    participant LocalImpl as "本地实现 (cmx-iam/biz/plugin)"
    participant DBMgr as "DatabaseManager"
    participant DB as "PostgreSQL (默认库 = 业务库)"

    Client->>Axum: HTTP 请求 + Bearer Token
    Axum->>MW: 路由匹配
    MW->>MW: mw_ctx 解析 request_id
    MW->>MW: mw_auth 验证 JWT 得到 AuthContext
    MW->>MW: mw_permission 校验权限
    MW->>Handler: 注入 CmxSvrContext
    Handler->>Trait: 调用 dyn Trait 方法
    Trait->>LocalImpl: 进程内直接调用
    LocalImpl->>DBMgr: get_dbx(default_db_id / biz_db_id)
    Note over DBMgr: biz_db_id 解析回退默认库 (同库, 静默回退)
    DBMgr->>DB: SQL 执行
    DB-->>DBMgr: 结果集
    LocalImpl-->>Trait: 返回
    Trait-->>Handler: 返回
    Handler-->>Client: ApiResp JSON
```

### 2.3 单体架构图

```mermaid
graph TB
    subgraph Proc["web-server 进程（单体）"]
        HTTP["Axum HTTP :8080"]
        Routers["cmx-api 路由族"]
        State["CmxAppState<br/>(注入 Local 实现的 Trait Object)"]
        IamL["cmx-iam (Local)"]
        BizL["cmx-biz (Local)"]
        PlgL["cmx-plugin (Local)"]
        MetaL["cmx-metadata (Local)"]
        Runtime["cmx-runtime WASM"]
    end
    subgraph Store["存储"]
        Redis[("Redis<br/>会话/缓存/锁")]
        PG[("PostgreSQL<br/>cmx 库<br/>系统表+业务表")]
    end

    HTTP --> Routers --> State
    State --> IamL & BizL & PlgL & MetaL
    IamL & BizL & PlgL & MetaL --> PG
    State --> Redis
    Runtime -.WASM 调用.-> BizL

    note["mono: 默认库与业务库为同一物理库<br/>所有 trait 均为 Local 实现<br/>无 gRPC / 无远程 center_client"]
```

> **要点**：单体模式下，没有任何流量离开进程。`app_id ≡ module_code` 的约束、`domain/application/module` 三元组仅用于给数据源行打标，在单实例部署里三者恒为 `"default"`。

---

## 三、微服务运行模式（Microservice）

### 3.1 运行特征

- **多进程**：每个服务域（如 IAM 中心、表单中心、流程中心、业务编排中心）可独立部署为 `web-server` 实例，通过 `[app] module_code` 标识自身所属。
- **配置标志**：`[center_client] mode = "grpc"|"http_url"|"http_discovery"` + `[rpc] enabled = true`。
- **调用方式**：跨域调用经 Nacos 服务发现后走 gRPC（volo）或 HTTP multipart。
- **数据库**：每个服务实例仅装载属于自己的数据源（`load_active_datasources` 按 `domain/application/module` 过滤），默认库（系统表）与业务库分离。

### 3.2 微服务架构图

```mermaid
graph TB
    subgraph GW["入口"]
        Edge["HTTP 边缘<br/>(任一实例或网关)"]
    end
    subgraph NACOS["服务注册/发现"]
        Reg[("Nacos")]
    end
    subgraph SvcIAM["IAM 服务实例<br/>module_code=iam"]
        H1["Axum :8080"]
        G1["gRPC :9090<br/>CmxResourceDataService"]
        I1["cmx-iam Local"]
    end
    subgraph SvcForm["表单中心实例<br/>module_code=form"]
        H2["Axum :8080"]
        G2["gRPC :9090"]
        F2["cmx-biz::form Local"]
    end
    subgraph SvcBiz["业务编排实例<br/>module_code=biz"]
        H3["Axum :8080"]
        G3["gRPC :9090"]
        B3["cmx-service + runtime"]
    end
    subgraph DBs["数据库"]
        SysDB[("默认库 PG<br/>cmx_*_sys/auth/iam/meta_define")]
        BizDB[("业务库 PG<br/>model_*/插件建表")]
    end
    subgraph Redis["共享态"]
        RS[("Redis<br/>会话/权限缓存/锁")]
    end

    Edge --> H1
    Reg -.发现.-> SvcIAM & SvcForm & SvcBiz
    SvcIAM & SvcForm & SvcBiz -.注册.-> Reg

    H1 --> I1
    H2 --> F2
    H3 --> B3
    I1 --> SysDB
    F2 --> SysDB
    B3 --> BizDB

    B3 -.模块导入: Remote DefinitionImporterBundle.-> G1 & G2
    B3 -.服务编排: ServiceOrchestrationClient.-> G2
    SvcIAM & SvcForm & SvcBiz --> RS
```

### 3.3 微服务跨实例调用时序（以模块资源导入为例）

```mermaid
sequenceDiagram
    autonumber
    participant Biz as "业务编排实例 (module_code=biz)"
    participant Nacos as "Nacos"
    participant FormCenter as "表单中心实例 (module_code=form)"
    participant DB as "默认库"

    Note over Biz: center_client.mode = grpc
    Biz->>Biz: 解析包内 forms/*.json
    Biz->>Nacos: 查询 form_service 健康实例
    Nacos-->>Biz: 实例列表 + grpc_port
    Biz->>FormCenter: gRPC import_resource_data (domain/app/module/app_id + ZIP)
    Note over Biz,FormCenter: 当前无 Authorization 元数据 / 当前无 request_id 传播
    FormCenter->>FormCenter: RemoteFormDefinitionImporter 本地落库
    FormCenter->>DB: INSERT cmx_form
    DB-->>FormCenter: OK
    FormCenter-->>Biz: ImportResult
```

> **关键风险标注**：当前微服务路径上，gRPC 服务端**无认证拦截器**，客户端**不携带身份/请求上下文**（详见评审文档 🔴 项）。这意味着在真正拆分为独立网络服务前，这些缺口必须补齐，否则 gRPC 端口（9090）将成为未授权访问入口。

---

## 四、数据库双模对照

### 4.1 配置层

`[[databases]]` 数组中每个数据源由以下字段区分语义：

| 字段 | 含义 |
|------|------|
| `db_id` | 逻辑标识（`primary`/`biz`/...） |
| `default: bool` | 是否主数据源（全局唯一） |
| `source_type` | `"default"` \| `"biz"` \| `"other"`（未设置时由 `default` 派生） |
| `domain_code/application_code/module_code` | 所属身份三元组（启动时注入） |

### 4.2 双模映射

```mermaid
graph LR
    subgraph Mono["单体模式"]
        MCFG["config: primary(default=biz 同库)"]
        MDb[("单一 PostgreSQL<br/>cmx 库")]
        MCFG --> MDb
    end
    subgraph Micro["微服务模式"]
        μCFG1["默认库 default=true<br/>source_type=default"]
        μCFG2["业务库 default=false<br/>source_type=biz"]
        μSys[("PG cmx_sys<br/>系统表")]
        μBiz[("PG cmx_biz<br/>业务表+model_*")]
        μCFG1 --> μSys
        μCFG2 --> μBiz
    end
```

| 维度 | 单体 | 微服务 |
|------|------|--------|
| 默认库与业务库 | **同一物理库** | **物理分离** |
| `get_biz_db_id()` | 回退到默认库（同库） | 返回 biz 库 db_id |
| 系统表（plugin/iam/auth/meta_define/...） | 默认库 | 默认库（各服务共享或集中） |
| 业务表（model_*/插件建表） | 默认库（同库） | 业务库 |
| 表路由决策点 | 集中于 `deploy.rs` / `module_install.rs` / importer | 同左（无集中抽象） |

### 4.3 表归类

**系统/平台表（驻留默认库）**：`cmx_domain/application/module`、`cmx_sys_datasource`、`cmx_plugin(_versions)`、`cmx_audit_log`、`cmx_plugin_audit_log`、`cmx_auth_*`（6 张）、`cmx_user/role/role_group/permission/...`（9 张）、`cmx_meta_table_define(_version)`、`cmx_service_define(_version)`、`cmx_marketplace_*`、`cmx_file_*`、`cmx_form/menu`、`cmx_module_current_version/version_history`。

**业务表（驻留业务库）**：`cmx_model_meta/module/deploy_history/source`、`cmx_model_registry`（仅默认库）、插件运行时动态创建的表（由 `cmx_meta_table_define.db_id` 列记录其物理归属）。

> 注意 `cmx_model_*` 表会在**默认库与每个业务库**同时创建（由 `PgTableDefineExecutor` 程序化建表 + 主库迁移保底）。

### 4.4 表路由现状（关键观察）

工程**没有集中的 `DbRouter` 抽象**，「哪张表写入哪个库」的决策分散在 20+ 调用点：

- `PluginPersistence` → 写默认库（事务守卫）
- `DeployService::deploy` → `request.db_id ?: get_biz_db_id()`
- `ModuleInstallService` → 校验/版本台账写默认库；资源/表定义写业务库
- `LocalTableDefinitionImporter` → 业务表建在业务库，元数据注册在默认库（最清晰的跨库指针设计）
- `cmx-iam` → `auth_db_id ?: default_db_id`
- `cmx-audit` → 构造期固定 db_id

这种「策略散落」使路由规则容易被无声破坏（详见评审文档 🟡 项）。

---

## 五、身份与数据隔离

### 5.1 身份三元组的双重角色

`domain_code / application_code / module_code`（及其等价的 `app_id`）在系统中承担**两种不同职责**，当前实现存在概念混淆：

| 职责 | 当前实现 | 说明 |
|------|---------|------|
| **数据路由键** | ✅ 已落地 | 标记数据源归属，`load_active_datasources` 按此过滤；导入请求体携带此字段路由到对应中心 |
| **请求上下文（调用者身份）** | ❌ 未落地 | 跨服务调用**不**作为 header/metadata 传播；下游无法得知调用者是谁 |

依据 `AGENTS.md §6`：当前 `app_id ≡ module_code`，二者恒等，行级多租户尚未实现，隔离粒度为**数据源/连接池级**而非行级。

### 5.2 隔离模型

```mermaid
graph TB
    subgraph Isolation["当前隔离模型"]
        direction TB
        L1["数据源级隔离<br/>(domain/app/module 三元组过滤数据源)"]
        L2["❌ 无行级多租户<br/>(业务表无 tenant_id 列)"]
        L1 --> L2
    end
```

这意味着：微服务模式下，一个模块实例只能「看到」自己的数据源池；但若多个模块共享同一业务库，**库内表数据并不天然隔离**，需依赖业务层的 `module_code/app_id` WHERE 条件（如 `cmx_plugin`、`cmx_meta_table_define` 等表）。

---

## 六、小结：双模能力评估

| 能力维度 | 单体 | 微服务 | 说明 |
|---------|:----:|:------:|------|
| 进程内 trait 直调 | ✅ | — | 默认行为 |
| 跨进程 gRPC 调用 | — | ⚠️ 部分 | 仅 orchestrator/resource_data 两个域有 RPC 面 |
| 服务注册发现 | — | ✅ | Nacos 完备，含 watch + 负载均衡 |
| Local↔Remote 配置切换 | ✅ | ✅ | OCP 合规，`center_client.mode` 驱动 |
| 数据库双模 | ✅ | ⚠️ | 路由分散，回退静默，漂移不可检测 |
| **跨服务认证** | — | 🔴 **缺失** | gRPC 无拦截器 |
| **身份/请求上下文传播** | — | 🔴 **缺失** | 客户端无 auth header |
| **IAM/Auth 远程化** | — | 🔴 **缺失** | 无 client trait 实现 |
| 会话水平扩展 | ✅ | ✅ | Redis 已支持 |

**结论**：工程的「形」（trait 抽象、服务发现、传输无关）已具备微服务雏形，但「神」（信任边界、身份传播、IAM 远程化）尚未补齐。**当前可以安全地以单体运行；在补齐三大 🔴 缺口前，不建议将服务真正拆分到独立网络部署。** 具体问题与优化路线见 `docs/deployment-mode-review.md`。
