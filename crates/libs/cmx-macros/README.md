# cmx-macros

> 权限/角色注解属性宏 — 对标 Spring Security 注解体系，为 cmx-container 提供 7 个声明式权限控制宏。

[![Version](https://img.shields.io/badge/version-0.1.9-blue.svg)](https://crates.io)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/)
[![Type](https://img.shields.io/badge/type-proc--macro-purple.svg)](https://doc.rust-lang.org/reference/procedural-macros.html)

`cmx-macros` 是一个过程宏（proc-macro）crate，为 cmx-container 框架的 Web 路由处理器提供声明式权限/角色注解。通过在 `axum` handler 函数上添加属性宏，即可在编译期完成权限检查代码注入、权限元数据注册和路由处理器登记，无需手动编写鉴权样板代码。

**核心价值**：
- 🎯 **声明式鉴权** — 一行注解完成权限/角色检查，对标 Spring Security `hasAuthority/hasRole/permitAll`
- 📝 **编译期注册** — 通过 `inventory` 在编译期收集所有权限定义与路由登记，启动期可做漏写告警与 DB 一致性校验
- 🔗 **零运行时开销** — 鉴权逻辑直接注入函数体首行，无反射、无动态分发
- 🛡️ **短路放行** — 拥有 `system:all` 权限或 `admin` 角色的用户自动跳过所有检查

---

## 快速开始

### 安装指南

在调用方 crate 的 `Cargo.toml` 中添加依赖（通过 workspace 引用）：

```toml
[dependencies]
# 内部依赖 - 权限注解宏
cmx-macros = { workspace = true }
# 内部依赖 - 核心库（提供 CmxSvrContext / AuthContext / PermissionDeniedError）
cmx-core = { workspace = true }
# 内部依赖 - API 类型（提供 Error 转换）
cmx-api-types = { workspace = true }
# 编译期全局注册
inventory = { workspace = true }
```

> ⚠️ **重要**：本 crate 是 proc-macro crate，只能作为依赖被引用，不能直接运行示例代码。所有示例均需在 axum handler 上下文中使用。

### 核心示例

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_core::model::data::dataset::DataSet;

/// 创建用户接口（必须拥有 user:create 权限）
///
/// 宏会自动完成三件事：
/// 1. 编译期注册权限元数据 RegisteredPermission { key, group, display, description }
/// 2. 编译期登记路由处理器 RegisteredRouteHandler { is_public: false }
/// 3. 在函数体首行注入 svr_ctx.require_permission("user:create")? 检查
#[cmx_macros::has_permission(
    key = "user:create",           // 权限码（唯一标识）
    group = "用户管理",             // 权限分组（用于管理界面分类展示）
    display = "创建用户",           // 显示名称
    description = "创建新用户账户"   // 详细描述
)]
pub async fn create_user(
    CmxSvrContext(svr_ctx): CmxSvrContext,  // 必须包含此参数（宏通过类型路径识别）
    Json(payload): Json<UserForCreate>,
) -> Result<Json<DataSet>, cmx_api_types::Error> {
    // ⬇️ 宏已在此处自动注入：svr_ctx.require_permission("user:create")?;
    let user = user_bmc::create(&svr_ctx, payload).await?;
    Ok(Json(DataSet::from(user)))
}
```

---

## 核心功能与特性

### 宏一览表

| 宏 | 对标 Spring Security | 语义 | 参数语法 | 注册元数据 | 注册路由登记 |
|------|---------------------|------|----------|-----------|-------------|
| `#[has_permission]` | `hasAuthority` | 单权限检查 | `key="...", group="...", display="...", description="..."` | ✅ `RegisteredPermission` | ✅ `is_public: false` |
| `#[has_permissions]` | 多个 `hasAuthority` 用 `and` 连接 | AND（全部权限） | `"a", "b", "c"` | ❌ | ✅ `is_public: false` |
| `#[has_any_permission]` | `hasAnyAuthority` | OR（任一权限） | `"a", "b", "c"` | ❌ | ✅ `is_public: false` |
| `#[has_role]` | `hasRole` | 单角色检查 | `"admin"` | ❌ | ✅ `is_public: false` |
| `#[has_roles]` | 多个 `hasRole` 用 `and` 连接 | AND（全部角色） | `"admin", "auditor"` | ❌ | ✅ `is_public: false` |
| `#[has_any_role]` | `hasAnyRole` | OR（任一角色） | `"admin", "manager"` | ❌ | ✅ `is_public: false` |
| `#[permit_all]` | `permitAll` | 公开访问（无鉴权） | 无参数 | ❌ | ✅ `is_public: true` |

### 可选 Features

本 crate 无可选 features，所有宏默认全部启用。

### 关键特性

| 特性 | 说明 |
|------|------|
| **类型路径识别** | 宏通过匹配函数参数中 `CmxSvrContext(svr_ctx): CmxSvrContext` 模式自动提取 binding，无需手动指定 |
| **绝对路径生成** | 生成的代码使用 `::cmx_core::...` 和 `::inventory::submit!` 绝对路径，避免路径污染 |
| **短路放行机制** | 拥有 `system:all` 权限或 `admin` 角色的用户自动跳过所有权限/角色检查 |
| **错误自动转换** | 注入的检查代码使用 `.map_err(::cmx_api_types::Error::from)?` 自动转换错误类型 |
| **编译期登记** | 所有宏均通过 `inventory::submit!` 注册 `RegisteredRouteHandler`，用于漏写统计 |

---

## 模块结构

```text
cmx-macros
└── src
    └── lib.rs          # 7 个 proc-macro 属性宏定义 + 辅助函数
```

### 主要模块说明

#### `src/lib.rs`

包含全部 7 个 `#[proc_macro_attribute]` 宏定义及 3 个内部辅助函数：

| 内部函数 | 作用 |
|---------|------|
| `find_svr_ctx_binding` | 在函数参数中查找 `CmxSvrContext` 类型的 binding（通过类型路径匹配，非参数名） |
| `gen_route_handler_registration` | 生成 `inventory::submit!` 注册 `RegisteredRouteHandler` 的代码 |
| `parse_str_list` | 解析逗号分隔的字符串字面量列表（如 `"a", "b"`） |

---

## 使用指南

### 一、权限注解宏（3 个）

#### 1.1 `#[has_permission]` — 单权限检查（含元数据注册）

**场景说明**：保护需要特定权限才能访问的路由，同时将权限元数据（分组、显示名、描述）注册到全局权限表，供前端管理界面或运维查询使用。这是**唯一**会注册 `RegisteredPermission` 元数据的宏。

**参数说明**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `key` | 字符串字面量 | ✅ | 权限码，全局唯一标识（如 `"user:create"`） |
| `group` | 字符串字面量 | ❌ | 权限分组，用于管理界面分类展示 |
| `display` | 字符串字面量 | ❌ | 显示名称（中文友好） |
| `description` | 字符串字面量 | ❌ | 详细描述 |

**完整示例**：

```rust
use axum::extract::State;
use axum::Json;
use cmx_api::app_state::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 创建智能体接口
///
/// 业务逻辑：接收智能体配置 DTO，调用 BMC 层创建新智能体记录。
/// 鉴权要求：调用者必须拥有 `agent:create` 权限。
#[cmx_macros::has_permission(
    key = "agent:create",                    // 权限码
    group = "智能体管理",                     // 分组
    display = "创建智能体",                   // 显示名
    description = "创建新的智能体配置记录"     // 描述
)]
pub async fn create_agent(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,   // ⚠️ 必须包含此参数
    Json(payload): Json<AgentForCreate>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：svr_ctx.require_permission("agent:create").map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：调用 BMC 层创建智能体
    let agent = AgentBmc::create(&svr_ctx, &cmx_state, payload).await?;

    // 返回数据集
    Ok(Json(DataSet::from(agent)))
}
```

**宏展开效果**（简化示意）：

```rust
pub async fn create_agent(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<AgentForCreate>,
) -> Result<Json<DataSet>> {
    // 1. 注入的权限检查代码
    svr_ctx.require_permission("agent:create")
        .map_err(::cmx_api_types::Error::from)?;

    // 2. 原始函数体
    let agent = AgentBmc::create(&svr_ctx, &cmx_state, payload).await?;
    Ok(Json(DataSet::from(agent)))
}

// 3. 编译期注册权限元数据
::inventory::submit! {
    ::cmx_core::model::iam::registry::RegisteredPermission {
        key: "agent:create",
        group: "智能体管理",
        display: "创建智能体",
        description: "创建新的智能体配置记录",
        source: "cmx-macros:create_agent",
    }
}

// 4. 编译期登记路由处理器
::inventory::submit! {
    ::cmx_core::model::iam::registry::RegisteredRouteHandler {
        handler_name: "create_agent",
        is_public: false,
        source: "cmx-macros:create_agent",
    }
}
```

#### 1.2 `#[has_permissions]` — 全部权限检查（AND 语义）

**场景说明**：要求调用者**同时拥有**所有指定权限才能访问。例如导出智能体详情需要同时拥有 `agent:export` 和 `agent:read` 权限。

**参数说明**：逗号分隔的字符串字面量列表（类似 `#[derive(Debug, Clone)]` 语法）。

**完整示例**：

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 导出智能体详情接口
///
/// 业务逻辑：导出指定智能体的完整配置（含敏感字段）。
/// 鉴权要求：必须同时拥有 `agent:export` 和 `agent:read` 权限。
#[cmx_macros::has_permissions("agent:export", "agent:read")]
pub async fn export_agent_detail(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<ExportParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：
    // svr_ctx.require_all_permissions(&["agent:export", "agent:read"])
    //     .map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：导出智能体详情
    let detail = AgentBmc::export_detail(&svr_ctx, params.id).await?;
    Ok(Json(DataSet::from(detail)))
}
```

#### 1.3 `#[has_any_permission]` — 任一权限检查（OR 语义）

**场景说明**：调用者拥有**任意一个**指定权限即可访问。例如查看报表接口，拥有 `report:view` 或 `report:export` 任一权限即可。

**完整示例**：

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 查看报表接口
///
/// 业务逻辑：根据报表 ID 查询报表内容。
/// 鉴权要求：拥有 `report:view` 或 `report:export` 任一权限即可。
#[cmx_macros::has_any_permission("report:view", "report:export")]
pub async fn view_report(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<ReportParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：
    // svr_ctx.require_any_permission(&["report:view", "report:export"])
    //     .map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：查询报表
    let report = ReportBmc::get(&svr_ctx, params.id).await?;
    Ok(Json(DataSet::from(report)))
}
```

### 二、角色注解宏（3 个）

#### 2.1 `#[has_role]` — 单角色检查

**场景说明**：要求调用者拥有指定角色才能访问。例如系统设置接口仅限 `admin` 角色访问。

**参数说明**：单个字符串字面量。

**完整示例**：

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 系统设置接口
///
/// 业务逻辑：读取或修改系统级配置。
/// 鉴权要求：必须拥有 `admin` 角色。
#[cmx_macros::has_role("admin")]
pub async fn system_settings(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<SettingsParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：
    // svr_ctx.require_role("admin").map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：处理系统设置
    let settings = SystemBmc::get_settings(&svr_ctx).await?;
    Ok(Json(DataSet::from(settings)))
}
```

#### 2.2 `#[has_roles]` — 全部角色检查（AND 语义）

**场景说明**：要求调用者**同时拥有**所有指定角色。例如审计管理员操作接口需要同时拥有 `admin` 和 `auditor` 角色。

**完整示例**：

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 审计管理员操作接口
///
/// 业务逻辑：查询管理员的敏感操作日志。
/// 鉴权要求：必须同时拥有 `admin` 和 `auditor` 角色。
#[cmx_macros::has_roles("admin", "auditor")]
pub async fn audit_admin_op(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<AuditParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：
    // svr_ctx.require_all_roles(&["admin", "auditor"])
    //     .map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：查询审计日志
    let logs = AuditBmc::list_admin_ops(&svr_ctx, params).await?;
    Ok(Json(DataSet::from(logs)))
}
```

#### 2.3 `#[has_any_role]` — 任一角色检查（OR 语义）

**场景说明**：调用者拥有**任意一个**指定角色即可访问。例如团队管理接口，`admin` 或 `manager` 角色均可。

**完整示例**：

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// 团队管理接口
///
/// 业务逻辑：管理团队成员（增删改查）。
/// 鉴权要求：拥有 `admin` 或 `manager` 任一角色即可。
#[cmx_macros::has_any_role("admin", "manager")]
pub async fn manage_team(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<TeamParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已自动注入：
    // svr_ctx.require_any_role(&["admin", "manager"])
    //     .map_err(::cmx_api_types::Error::from)?;

    // 业务逻辑：管理团队
    let team = TeamBmc::manage(&svr_ctx, params).await?;
    Ok(Json(DataSet::from(team)))
}
```

### 三、公开访问标记

#### 3.1 `#[permit_all]` — 公开路由标记

**场景说明**：标记无需认证/权限即可访问的公开路由（如健康检查、登录接口）。仅登记到 `RegisteredRouteHandler { is_public: true }` 用于漏写统计，不注入任何鉴权代码。

**参数说明**：无参数。

**完整示例**：

```rust
use axum::Json;
use serde_json::{json, Value};

/// 健康检查接口
///
/// 业务逻辑：返回服务健康状态，供负载均衡器探活。
/// 鉴权要求：公开访问，无需认证。
#[cmx_macros::permit_all]
pub async fn health_check() -> Json<Value> {
    // ⚠️ 宏不会注入任何鉴权代码，函数体保持原样

    // 业务逻辑：返回健康状态
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}
```

**宏展开效果**：

```rust
pub async fn health_check() -> Json<Value> {
    // 函数体保持原样，无注入
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

// 仅注册路由处理器登记（is_public: true）
::inventory::submit! {
    ::cmx_core::model::iam::registry::RegisteredRouteHandler {
        handler_name: "health_check",
        is_public: true,
        source: "cmx-macros:health_check",
    }
}
```

### 四、错误处理

#### 4.1 鉴权失败错误类型

宏注入的权限检查代码在鉴权失败时会返回 `cmx_core::model::iam::PermissionDeniedError`，并通过 `.map_err(::cmx_api_types::Error::from)?` 自动转换为 `cmx_api_types::Error`。

**错误类型定义**（来自 `cmx-core`）：

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum PermissionDeniedError {
    /// 未认证（auth_context 缺失）
    #[error("未认证:缺少认证上下文")]
    Unauthenticated,

    /// 权限不足
    #[error("用户 {user_id} 缺少权限: {permission}")]
    Permission { user_id: String, permission: String },

    /// 角色不足（单角色）
    #[error("用户 {user_id} 缺少角色: {role}")]
    Role { user_id: String, role: String },

    /// 角色不足（多角色，AND/OR 语义）
    #[error("用户 {user_id} 缺少角色(需{requirement}): {roles}")]
    Roles {
        user_id: String,
        requirement: RoleRequirement,  // All 或 Any
        roles: String,
    },
}
```

#### 4.2 编译期错误处理

宏在编译期会进行参数校验，若校验失败会输出清晰的编译错误：

```rust
// ❌ 错误示例 1：缺少 CmxSvrContext 参数
#[cmx_macros::has_permission(key = "user:create", group = "用户管理", display = "创建", description = "")]
pub async fn create_user(Json(payload): Json<UserForCreate>) -> Result<Json<DataSet>> {
    // 编译错误：权限注解宏要求函数包含 CmxSvrContext 类型参数
    //          (如 CmxSvrContext(svr_ctx): CmxSvrContext)
    todo!()
}

// ❌ 错误示例 2：has_role 参数不是字符串字面量
#[cmx_macros::has_role(123)]
pub async fn handler(CmxSvrContext(svr_ctx): CmxSvrContext) -> Result<Json<DataSet>> {
    // 编译错误：expected string literal
    todo!()
}

// ❌ 错误示例 3：has_permissions 参数格式错误
#[cmx_macros::has_permissions("a", 123)]
pub async fn handler(CmxSvrContext(svr_ctx): CmxSvrContext) -> Result<Json<DataSet>> {
    // 编译错误：expected string literal
    todo!()
}
```

#### 4.3 运行时错误处理示例

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_api_types::Error;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::iam::PermissionDeniedError;

/// 删除用户接口
///
/// 演示运行时鉴权失败的错误处理。
/// 鉴权失败时，宏注入的代码会返回 Error::PermissionDenied，
/// 调用方无需额外处理，错误会自动转换为 HTTP 403 响应。
#[cmx_macros::has_permission(
    key = "user:delete",
    group = "用户管理",
    display = "删除用户",
    description = "删除指定用户账户"
)]
pub async fn delete_user(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(params): Json<DeleteParams>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏注入的检查代码等价于：
    // if !svr_ctx.has_permission("user:delete") {
    //     return Err(Error::from(PermissionDeniedError::Permission {
    //         user_id: svr_ctx.auth_context.as_ref().unwrap().user_id.clone(),
    //         permission: "user:delete".to_string(),
    //     }));
    // }

    // 业务逻辑：删除用户
    let result = UserBmc::delete(&svr_ctx, params.id).await?;
    Ok(Json(DataSet::from(result)))
}

/// 手动捕获鉴权错误（如需自定义响应）
pub async fn handle_permission_error(err: &Error) -> (axum::http::StatusCode, String) {
    // 通过 downcast 检查是否为权限拒绝错误
    match err {
        Error::PermissionDenied(denied) => match denied {
            PermissionDeniedError::Unauthenticated => {
                (axum::http::StatusCode::UNAUTHORIZED, "未登录或登录已过期".to_string())
            }
            PermissionDeniedError::Permission { user_id, permission } => {
                (axum::http::StatusCode::FORBIDDEN, format!("用户 {} 无权限: {}", user_id, permission))
            }
            PermissionDeniedError::Role { user_id, role } => {
                (axum::http::StatusCode::FORBIDDEN, format!("用户 {} 无角色: {}", user_id, role))
            }
            PermissionDeniedError::Roles { user_id, requirement, roles } => {
                (axum::http::StatusCode::FORBIDDEN, format!("用户 {} 无角色(需{}): {}", user_id, requirement, roles))
            }
        },
        _ => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("内部错误: {}", err)),
    }
}
```

### 五、与其他 crate 集成

#### 5.1 与 cmx-core 集成（权限注册表查询）

宏注册的权限元数据可通过 `cmx_core::model::iam::registry::PermissionRegistry` 在运行时查询：

```rust
use cmx_core::model::iam::registry::{PermissionRegistry, PermissionInfo};

/// 启动期：查询所有编译期注册的权限元数据
///
/// 应用场景：
/// 1. 漏写告警：对比路由总数与已登记 handler 数量，发现未加注解的路由
/// 2. DB 一致性校验：对比编译期注册的权限与数据库中的权限定义
/// 3. 前端管理界面：提供权限列表供管理员查看
pub fn print_all_registered_permissions() {
    // 获取所有通过 #[has_permission] 宏注册的权限
    let permissions: Vec<PermissionInfo> = PermissionRegistry::list_all();

    println!("编译期注册的权限列表（共 {} 项）：", permissions.len());
    for perm in &permissions {
        println!(
            "  - [{}] {} ({}): {} | 来源: {}",
            perm.group, perm.code, perm.display, perm.description, perm.source
        );
    }
}

/// 启动期：统计路由处理器登记情况
pub fn check_route_handler_coverage(total_routes: usize) {
    use cmx_core::model::iam::registry::all_registered_handlers;

    let handlers = all_registered_handlers();
    let public_count = handlers.iter().filter(|h| h.is_public).count();
    let protected_count = handlers.iter().filter(|h| !h.is_public).count();

    println!("路由处理器登记统计：");
    println!("  - 公开路由（permit_all）: {}", public_count);
    println!("  - 受保护路由: {}", protected_count);
    println!("  - 登记总数: {}", handlers.len());
    println!("  - 实际路由总数: {}", total_routes);

    if handlers.len() < total_routes {
        tracing::warn!(
            "发现 {} 个路由未添加权限注解，建议补全 #[has_permission] 或 #[permit_all]",
            total_routes - handlers.len()
        );
    }
}
```

#### 5.2 与 axum 路由集成

```rust
use axum::{routing::post, Router};
use cmx_api::app_state::CmxAppState;

/// 构建用户管理路由
///
/// 演示如何将带权限注解的 handler 注册到 axum 路由器。
pub fn build_user_routes() -> Router<CmxAppState> {
    Router::new()
        // 受保护路由：必须拥有 user:create 权限
        .route("/api/users/create", post(user_handlers::create_user))
        // 受保护路由：必须拥有 user:read 权限
        .route("/api/users/list", post(user_handlers::list_users))
        // 受保护路由：必须拥有 user:delete 权限
        .route("/api/users/delete", post(user_handlers::delete_user))
        // 公开路由：无需认证
        .route("/api/users/captcha", post(user_handlers::get_captcha))
}

mod user_handlers {
    use axum::Json;
    use cmx_api::middleware::CmxSvrContext;
    use cmx_api::Result;
    use cmx_core::model::data::dataset::DataSet;

    #[cmx_macros::has_permission(
        key = "user:create", group = "用户管理", display = "创建用户", description = "创建新用户"
    )]
    pub async fn create_user(
        CmxSvrContext(svr_ctx): CmxSvrContext,
        Json(p): Json<serde_json::Value>,
    ) -> Result<Json<DataSet>> { todo!() }

    #[cmx_macros::has_permission(
        key = "user:read", group = "用户管理", display = "查询用户", description = "查询用户列表"
    )]
    pub async fn list_users(
        CmxSvrContext(svr_ctx): CmxSvrContext,
        Json(p): Json<serde_json::Value>,
    ) -> Result<Json<DataSet>> { todo!() }

    #[cmx_macros::has_permission(
        key = "user:delete", group = "用户管理", display = "删除用户", description = "删除用户账户"
    )]
    pub async fn delete_user(
        CmxSvrContext(svr_ctx): CmxSvrContext,
        Json(p): Json<serde_json::Value>,
    ) -> Result<Json<DataSet>> { todo!() }

    #[cmx_macros::permit_all]
    pub async fn get_captcha(
        Json(p): Json<serde_json::Value>,
    ) -> Result<Json<DataSet>> { todo!() }
}
```

#### 5.3 与 utoipa OpenAPI 文档集成

```rust
use axum::Json;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::Result;
use cmx_core::model::data::dataset::DataSet;

/// utoipa 文档注解可与 cmx-macros 权限注解共存
///
/// 顺序建议：先写 utoipa::path，再写 cmx_macros 权限注解，
/// 这样宏注入的鉴权代码会在 utoipa 生成的文档注解之后执行。
#[utoipa::path(
    post,
    path = "/api/users/create",
    request_body = UserForCreate,
    responses(
        (status = 200, description = "创建成功"),
        (status = 401, description = "未认证"),
        (status = 403, description = "权限不足")
    ),
    tag = "用户管理"
)]
#[cmx_macros::has_permission(
    key = "user:create",
    group = "用户管理",
    display = "创建用户",
    description = "创建新用户账户"
)]
pub async fn create_user_with_doc(
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(payload): Json<UserForCreate>,
) -> Result<Json<DataSet>> {
    // ⬇️ 宏已注入权限检查
    let user = UserBmc::create(&svr_ctx, payload).await?;
    Ok(Json(DataSet::from(user)))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct UserForCreate {
    pub username: String,
    pub email: String,
}
```

#### 5.4 与 cmx-api 中间件链集成

```rust
use axum::Router;
use cmx_api::app_state::CmxAppState;
use cmx_api::middleware::{mw_context_resolver, mw_auth, mw_permission};

/// 构建带完整中间件链的路由器
///
/// 中间件执行顺序（请求流入方向）：
/// 1. mw_context_resolver — 创建 CmxSvrContext 并注入请求扩展
/// 2. mw_auth — 验证 Token，注入 AuthContext 到 CmxSvrContext
/// 3. mw_permission — 全局路由→权限码映射检查（可选，与宏注入的检查互补）
/// 4. handler — 宏注入的 require_permission 检查 + 业务逻辑
///
/// ⚠️ 重要：宏注入的权限检查依赖 mw_auth 已注入 AuthContext，
///          因此必须确保 mw_auth 中间件在 handler 之前执行。
pub fn build_app() -> Router<CmxAppState> {
    Router::new()
        .route("/api/users/create", axum::routing::post(create_user))
        .layer(axum::middleware::from_fn(mw_permission))
        .layer(axum::middleware::from_fn(mw_auth))
        .layer(axum::middleware::from_fn(mw_context_resolver))
}

#[cmx_macros::has_permission(
    key = "user:create", group = "用户管理", display = "创建用户", description = "创建新用户"
)]
pub async fn create_user(
    cmx_api::middleware::CmxSvrContext(svr_ctx): cmx_api::middleware::CmxSvrContext,
    axum::Json(p): axum::Json<serde_json::Value>,
) -> cmx_api::Result<axum::Json<cmx_core::model::data::dataset::DataSet>> {
    todo!()
}
```

---

## 常见问题解答（FAQ）

### Q1: 为什么所有宏都要求函数包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数？

**A**: 宏通过类型路径匹配（非参数名）识别 `CmxSvrContext` 类型的 binding，提取内部的 `svr_ctx` 用于调用 `require_permission` / `require_role` 等方法。这是鉴权检查的入口。若函数未包含此参数，编译期会报错：

```
权限注解宏要求函数包含 CmxSvrContext 类型参数(如 CmxSvrContext(svr_ctx): CmxSvrContext)
```

### Q2: `#[has_permission]` 与其他权限宏的区别是什么？

**A**: `#[has_permission]` 是**唯一**会注册 `RegisteredPermission` 元数据的宏，包含 `key/group/display/description` 四个字段，用于：
- 前端管理界面展示权限列表
- 启动期与数据库权限定义做一致性校验
- 生成权限文档

其他权限宏（`has_permissions` / `has_any_permission`）和角色宏仅注册 `RegisteredRouteHandler`（用于漏写统计），不注册权限元数据。因此**每个权限码首次出现时建议使用 `#[has_permission]`**，后续组合检查可用其他宏。

### Q3: 短路放行机制是什么？如何触发？

**A**: 拥有 `system:all` 权限或 `admin` 角色的用户会自动跳过所有权限/角色检查（在 `AuthContext::is_short_circuited` 中实现）。这是为了方便超级管理员调试和运维，无需为每个接口单独授权。

```rust
// cmx-core 中的实现：
fn is_short_circuited(&self) -> bool {
    self.has_permission("system:all") || self.has_role("admin")
}
```

### Q4: 宏注入的权限检查与 `mw_permission` 中间件有何区别？是否会重复检查？

**A**: 两者职责不同，互补共存：

| 机制 | 检查时机 | 配置方式 | 适用场景 |
|------|---------|---------|---------|
| 宏注入检查 | handler 函数体首行 | 声明式注解，与 handler 绑定 | 单个 handler 的细粒度权限检查 |
| `mw_permission` 中间件 | 路由分发前 | 全局路由→权限码映射表 | 全局统一的前置权限过滤 |

两者可能对同一权限进行检查，但由于权限检查是幂等的（`require_permission` 返回 `Result`），重复检查不会产生副作用，性能影响可忽略。

### Q5: 为什么生成的代码使用绝对路径 `::cmx_core::...`？

**A**: proc-macro 生成的代码在调用方 crate 中展开，若使用相对路径（如 `cmx_core::...`），可能因调用方 crate 的 `use` 语句或模块结构导致路径解析失败。使用绝对路径 `::cmx_core::...` 可确保路径始终指向 crate 根，避免路径污染。

### Q6: 如何查看宏展开后的代码？

**A**: 使用 `cargo expand` 工具查看宏展开结果：

```bash
# 安装 cargo-expand
cargo install cargo-expand

# 查看指定 crate 的宏展开
cargo expand -p cmx-iam

# 查看指定模块
cargo expand -p cmx-iam module_name
```

### Q7: `#[permit_all]` 与不加任何注解有何区别？

**A**: 

- **不加注解**：路由不会登记到 `RegisteredRouteHandler`，启动期漏写告警会将其视为"未加注解的路由"
- **`#[permit_all]`**：路由登记为 `RegisteredRouteHandler { is_public: true }`，明确标记为公开访问，不会触发漏写告警

建议所有路由都显式添加注解（`#[permit_all]` 或其他权限注解），便于启动期统计与审计。

### Q8: 宏是否支持在 `impl` 块内的方法上使用？

**A**: 支持。宏作用于 `ItemFn`，无论是自由函数还是 `impl` 块内的方法均可使用，只要函数签名包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数即可。

```rust
impl UserHandlers {
    #[cmx_macros::has_permission(
        key = "user:update", group = "用户管理", display = "更新用户", description = "更新用户信息"
    )]
    pub async fn update_user(
        CmxSvrContext(svr_ctx): CmxSvrContext,
        Json(p): Json<serde_json::Value>,
    ) -> cmx_api::Result<Json<DataSet>> {
        todo!()
    }
}
```

### Q9: 如何在测试中绕过权限检查？

**A**: 在单元测试中，可构造包含 `system:all` 权限或 `admin` 角色的 `AuthContext`，利用短路放行机制跳过权限检查：

```rust
#[cfg(test)]
mod tests {
    use cmx_core::model::iam::AuthContext;
    use cmx_core::model::service::SVRContext;

    fn build_admin_svr_ctx() -> SVRContext {
        let mut ctx = SVRContext::default();
        ctx.auth_context = Some(AuthContext {
            user_id: "test-admin".to_string(),
            username: "admin".to_string(),
            roles: vec!["admin".to_string()],  // admin 触发短路放行
            permissions: vec![],
        });
        ctx
    }

    #[tokio::test]
    async fn test_create_user_bypass_permission() {
        let svr_ctx = build_admin_svr_ctx();
        // 调用 handler，权限检查会被短路放行
        // ...
    }
}
```

### Q10: 宏注册的权限元数据在何时被收集？

**A**: `inventory` crate 在**编译期**通过 `inventory::submit!` 将权限定义收集到全局静态集合中，在**运行时**通过 `inventory::iter::<RegisteredPermission>()` 即可遍历所有注册项。无需手动调用注册函数，只要宏被使用，权限就会自动注册。
