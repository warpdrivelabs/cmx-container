# cmx-iam

> cmx-iam — 用户权限角色管理（IAM）业务 crate，提供服务端 User/Role/Permission/RoleGroup/ExclusionRule 的 Entity/BMC/Filter 定义、Service 层业务逻辑、`UserAuthQuery` 与 `PermissionChecker` trait 实现，以及临时授权、互斥规则校验、熔断降级、审计日志等企业级能力。

[![Version](https://img.shields.io/badge/version-0.1.9-blue.svg)](https://crates.io)
[![Edition](https://img.shields.io/badge/edition-2021-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-workspace-green.svg)]()

本 crate 为服务端专用（WASM 不可达），基础数据模型（`User`/`Role`/`Permission`/`RoleGroup`）定义在 `cmx-core` 中，本 crate 通过 re-export 暴露。所有自定义错误使用 `thiserror`，日志使用 `tracing`，依赖通过 `workspace = true` 引用。

---

## 快速开始

### 安装指南

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
# IAM 业务库（默认不含 OpenAPI 文档生成）
cmx-iam = { path = "../libs/cmx-iam" }

# 如需 OpenAPI Schema 自动生成（utoipa::ToSchema），启用 openapi feature
# cmx-iam = { path = "../libs/cmx-iam", features = ["openapi"] }
```

### 核心示例

以下示例展示如何初始化 IAM 各 Service 并完成一次「创建用户 → 分配角色 → 校验权限」的完整流程：

```rust
use std::sync::Arc;
use cmx_iam::{
    IamConfig, IamChecker,
    RoleServiceImpl, UserServiceImpl, PermissionServiceImpl, RoleGroupServiceImpl,
    user::UserForCreate, role::RoleForCreate, permission::PermissionForCreate,
};
use cmx_core::SVRContext;
use cmx_database::DatabaseManager;
use cmx_traits::auth::AuthService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化数据库管理器与认证服务（由其他 crate 提供）
    let mm: Arc<DatabaseManager> = /* 初始化 DatabaseManager */;
    let auth: Arc<dyn AuthService> = /* 初始化 AuthService */;
    let config = IamConfig::default();

    // 2. 构造各 Service（Builder 模式注入可选依赖）
    let user_svc = UserServiceImpl::new(mm.clone(), auth, config.clone()).await;
    let role_svc = RoleServiceImpl::new(mm.clone(), config.clone()).await;
    let perm_svc = PermissionServiceImpl::new(mm.clone(), config.clone()).await;

    // 3. 构造权限校验器（实现 cmx_traits::iam::PermissionChecker）
    let checker = Arc::new(IamChecker::new(mm.clone(), config.clone()).await);

    // 4. 业务调用示例：创建角色 → 创建用户 → 分配角色
    let svr_ctx = SVRContext::default();
    let role = role_svc.create_role(&svr_ctx, RoleForCreate {
        code: "viewer".into(),
        name: "只读访客".into(),
        ..Default::default()
    }).await?;

    let user = user_svc.create_user(&svr_ctx, UserForCreate {
        username: "alice".into(),
        password: "P@ssw0rd".into(),
        ..Default::default()
    }).await?;

    user_svc.assign_roles(&svr_ctx, "alice", &[role.id.clone()]).await?;

    // 5. 权限校验
    let has = checker.has_role(&user.id, "viewer").await?;
    println!("用户是否拥有 viewer 角色: {}", has);

    Ok(())
}
```

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 用户管理 | 用户的 CRUD、分页/列表查询、密码哈希（Argon2id）、状态启停 |
| 角色管理 | 角色 CRUD、内置角色保护、分页查询、角色组归属 |
| 权限管理 | 权限 CRUD、权限树（递归）、域/应用/模块多维过滤、使用统计 |
| 角色组管理 | 角色组 CRUD、树形结构（递归）、父子层级、删除保护 |
| 用户角色关联 | 永久角色分配（全量替换）、按 username 查询角色列表 |
| 角色权限关联 | 权限分配（全量替换）、按 role_id 查询权限列表 |
| 临时授权 | 带有效期的角色授权、撤销/批量撤销/延期、状态过滤查询 |
| 互斥规则 | 功能权限互斥 + 角色互斥（SoD），规则 CRUD、校验测试 |
| 权限校验 | `PermissionChecker` trait 实现，`system:all` 超级权限短路 |
| 缓存与熔断 | 可选 Redis 缓存（带抖动 TTL 防雪崩）、熔断器降级（FailOpen/FailClose） |
| 审计日志 | 通过 `AuditLogger` trait 写入操作审计，批量阈值聚合 |
| 认证查询 | `UserAuthQuery` trait 实现，供 `cmx-auth` 查询用户/角色/权限 |
| 一致性校验 | 代码声明权限 vs DB 存在性比对，生成缺失权限的 INSERT DDL |
| 定时清理 | 临时授权过期清理任务，按用户分组写审计 |

### 可选 Features

| Feature | 默认启用 | 说明 |
|---------|---------|------|
| `default` | ✅ | 基础功能（无额外依赖） |
| `openapi` | ❌ | 启用 `utoipa::ToSchema` 派生，用于 OpenAPI 文档自动生成 |

---

## 模块结构

```text
cmx-iam
├── src/
│   ├── lib.rs                    # crate 入口，模块导出与 re-export
│   ├── config.rs                 # IamConfig 配置定义（含 FailureMode）
│   ├── error.rs                  # IamError 错误类型（thiserror）与转换
│   ├── service_traits.rs         # UserService/RoleService/PermissionService/RoleGroupService trait
│   ├── iam_checker.rs            # IamChecker — PermissionChecker 实现（缓存+熔断）
│   ├── circuit_breaker.rs        # 熔断器（closed/open 两态）
│   ├── scheduler.rs              # 临时授权过期清理定时任务
│   ├── audit_helper.rs           # AuditHelper trait — 审计日志写入辅助
│   ├── user_auth_query_impl.rs   # UserAuthQueryImpl — UserAuthQuery trait 实现
│   ├── user/                     # 用户管理模块
│   │   ├── mod.rs                # 模块导出
│   │   ├── entity.rs             # UserForCreate/Update/Insert 等 DTO
│   │   ├── filter.rs             # UserFilter（modql 过滤器）
│   │   ├── bmc.rs                # UserBmc/UserRoleBmc 业务模型组件
│   │   └── service.rs            # UserServiceImpl 服务实现
│   ├── role/                     # 角色管理模块
│   │   ├── mod.rs                # 模块导出
│   │   ├── entity.rs             # RoleForCreate/Update、AssignPermissionsRequest
│   │   ├── filter.rs             # RoleFilter
│   │   ├── bmc.rs                # RoleBmc/RolePermissionBmc
│   │   └── service.rs            # RoleServiceImpl 服务实现
│   ├── permission/               # 权限管理模块
│   │   ├── mod.rs                # 模块导出
│   │   ├── entity.rs             # PermissionForCreate/Update
│   │   ├── filter.rs             # PermissionFilter
│   │   ├── bmc.rs                # PermissionBmc
│   │   ├── service.rs            # PermissionServiceImpl 服务实现
│   │   └── consistency_check.rs  # 代码声明权限 vs DB 一致性校验
│   ├── role_group/               # 角色组管理模块
│   │   ├── mod.rs                # 模块导出
│   │   ├── entity.rs             # RoleGroupForCreate/Update
│   │   ├── filter.rs             # RoleGroupFilter
│   │   ├── bmc.rs                # RoleGroupBmc
│   │   └── service.rs            # RoleGroupServiceImpl 服务实现
│   └── rule/                     # 互斥规则模块（功能权限互斥 + 角色互斥）
│       ├── mod.rs                # 模块导出
│       ├── entity.rs             # ExclusionRule/Item、ValidateRule 请求响应
│       ├── bmc.rs                # ExclusionRuleBmc/ExclusionRuleItemBmc
│       ├── service.rs            # ExclusionRuleService trait + Impl
│       └── enforcer.rs           # RuleEnforcer trait + RuleEnforcerImpl
└── Cargo.toml
```

### 主要模块说明

#### `service_traits`

定义四个核心 Service trait：`UserService`、`RoleService`、`PermissionService`、`RoleGroupService`，以及临时授权相关的 `UserRoleAssignment`、`TempAssignmentStatusFilter` 和审计查询响应结构（`EffectivePermissionsResponse`、`PermissionDiffResponse`、`PermissionUsageStat` 等）。所有 trait 方法为 `async`，返回 `Result<T, TraitError>`。

#### `iam_checker`

`IamChecker` 实现 `cmx_traits::iam::PermissionChecker` trait，通过数据库 `EXISTS` 查询进行权限/角色校验。支持 `system:all` 超级权限短路、可选 Redis 缓存（带随机抖动 TTL 防雪崩、空结果短 TTL 防穿透）、熔断器降级（FailOpen 仅放行 `system:all` 用户 / FailClose 全部拒绝）。

#### `rule`

互斥规则模块，核心模型为「1 主对象 + N 互斥对象」。仅当用户集合同时包含主对象和任一互斥对象时判定违反，互斥对象之间不互斥。`RuleEnforcer` 在 `assign_permissions` 和 `assign_roles` 时进行 SoD 校验。

#### `user_auth_query_impl`

`UserAuthQueryImpl` 实现 `cmx_traits::auth::UserAuthQuery` trait，供 `cmx-auth` 在认证流程中查询用户/角色/权限。支持超管创建、OAuth2 自动注册（事务保证原子性）、密码哈希更新、最后登录信息更新。

---

## 使用指南

### 一、用户管理 CRUD

#### 1.1 创建用户

```rust
use std::sync::Arc;
use cmx_iam::{IamConfig, UserServiceImpl, user::UserForCreate};
use cmx_core::SVRContext;
use cmx_database::DatabaseManager;
use cmx_traits::auth::AuthService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化依赖
    let mm: Arc<DatabaseManager> = /* DatabaseManager */;
    let auth: Arc<dyn AuthService> = /* AuthService */;
    let config = IamConfig::default();

    // 构造用户服务
    let user_svc = UserServiceImpl::new(mm, auth, config).await;

    // 构造创建请求（password 为明文，Service 层会哈希后丢弃）
    let create_req = UserForCreate {
        username: "bob".into(),
        nickname: Some("鲍勃".into()),
        email: Some("bob@example.com".into()),
        phone: Some("13800000000".into()),
        password: "P@ssw0rd123".into(),
        avatar: None,
        org_id: Some("org_001".into()),
        description: Some("测试账号".into()),
        status: Some(1), // 1 启用 / 0 禁用
    };

    let svr_ctx = SVRContext::default();
    let user = user_svc.create_user(&svr_ctx, create_req).await?;
    println!("创建成功: id={}, username={}", user.id, user.username);

    Ok(())
}
```

#### 1.2 查询、更新、删除用户

```rust
use cmx_iam::{UserServiceImpl, user::{UserForUpdate, UserFilter}};
use cmx_core::SVRContext;

async fn user_crud_demo(user_svc: &UserServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // 按 username 查询单个用户
    let user = user_svc.get_user("bob").await?;
    println!("查询到用户: {}", user.username);

    // 更新用户（全 Option 字段，未提供不更新；提供 password 触发改密）
    let updated = user_svc.update_user(&svr_ctx, &user.id, UserForUpdate {
        nickname: Some("鲍勃-更新".into()),
        password: Some("NewP@ssw0rd".into()), // 触发密码修改
        status: Some(0), // 禁用
        ..Default::default()
    }).await?;
    println!("更新后状态: {}", updated.status);

    // 分页查询用户（modql 过滤器）
    let filter = UserFilter::default(); // 可填充 username/nickname/status 等条件
    let (users, total) = user_svc.page_users(filter, 1, 20).await?;
    println!("分页结果: 共 {} 条，当前页 {} 条", total, users.len());

    // 列表查询（不分页）
    let all = user_svc.list_users(UserFilter::default()).await?;
    println!("列表查询: {} 条", all.len());

    // 批量删除用户
    let user_ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
    user_svc.delete_user(&svr_ctx, &user_ids).await?;
    println!("已删除 {} 个用户", user_ids.len());

    Ok(())
}
```

### 二、角色管理

#### 2.1 角色 CRUD 与内置角色保护

```rust
use cmx_iam::{RoleServiceImpl, role::{RoleForCreate, RoleForUpdate, RoleFilter}};
use cmx_core::SVRContext;

async fn role_demo(role_svc: &RoleServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // 创建角色（code 业务唯一，用于策略/权限绑定）
    let role = role_svc.create_role(&svr_ctx, RoleForCreate {
        code: "editor".into(),
        name: "编辑".into(),
        role_group_id: None,
        data_scope: Some(2), // 1 全部 / 2 本部门 / 3 本人
        sort_order: Some(10),
        description: Some("内容编辑角色".into()),
    }).await?;
    println!("创建角色: code={}, id={}", role.code, role.id);

    // 查询单个角色
    let fetched = role_svc.get_role(&role.id).await?;

    // 更新角色
    let updated = role_svc.update_role(&svr_ctx, &role.id, RoleForUpdate {
        name: Some("高级编辑".into()),
        sort_order: Some(20),
        ..Default::default()
    }).await?;

    // 分页查询
    let (roles, total) = role_svc.page_roles(RoleFilter::default(), 1, 50).await?;

    // 删除角色（批量）。注意：builtin_role_codes 中的角色不可删除，会返回 CannotDeleteBuiltinRole
    role_svc.delete_role(&svr_ctx, &[role.id]).await?;

    Ok(())
}
```

#### 2.2 角色组管理

```rust
use cmx_iam::{RoleGroupServiceImpl, role_group::{RoleGroupForCreate, RoleGroupForUpdate}};
use cmx_core::SVRContext;

async fn role_group_demo(group_svc: &RoleGroupServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // 创建根角色组
    let root = group_svc.create_role_group(&svr_ctx, RoleGroupForCreate {
        name: "业务角色".into(),
        parent_id: None, // 根节点
        sort_order: Some(1),
        description: Some("业务线角色分组".into()),
    }).await?;

    // 创建子角色组（父子层级）
    let child = group_svc.create_role_group(&svr_ctx, RoleGroupForCreate {
        name: "订单角色".into(),
        parent_id: Some(root.id.clone()),
        sort_order: Some(1),
        description: None,
    }).await?;

    // 获取角色组树（递归结构）
    let tree = group_svc.get_role_group_tree().await?;
    println!("角色组树节点数: {}", tree.len());

    // 更新角色组
    let updated = group_svc.update_role_group(&svr_ctx, &child.id, RoleGroupForUpdate {
        name: Some("订单角色-更新".into()),
        ..Default::default()
    }).await?;

    // 删除角色组（若存在子组或关联角色会返回 RoleGroupInUse 错误）
    group_svc.delete_role_group(&svr_ctx, &[child.id]).await?;

    Ok(())
}
```

### 三、权限管理

#### 3.1 权限 CRUD 与权限树

```rust
use cmx_iam::{PermissionServiceImpl, permission::{PermissionForCreate, PermissionForUpdate}};
use cmx_core::SVRContext;

async fn permission_demo(perm_svc: &PermissionServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // 创建权限（code 如 system:user:add，业务唯一，用于鉴权匹配）
    let perm = perm_svc.create_permission(&svr_ctx, PermissionForCreate {
        code: "system:user:add".into(),
        name: "新增用户".into(),
        resource_type: Some("button".into()), // menu / button / api
        parent_id: None, // 根权限
        sort_order: Some(1),
        description: Some("新增用户按钮权限".into()),
        domain_code: Some("platform".into()),
        app_code: Some("user-center".into()),
        module_code: Some("user".into()),
        extension: None,
    }).await?;
    println!("创建权限: code={}", perm.code);

    // 创建子权限（树形结构）
    let child = perm_svc.create_permission(&svr_ctx, PermissionForCreate {
        code: "system:user:delete".into(),
        name: "删除用户".into(),
        parent_id: Some(perm.id.clone()),
        ..Default::default()
    }).await?;

    // 获取权限树（支持按域/应用/模块过滤）
    let tree = perm_svc.get_permission_tree(
        Some("platform"),     // domain_code
        Some("user-center"),  // app_code
        Some("user"),         // module_code
    ).await?;
    println!("权限树节点数: {}", tree.len());

    // 更新权限
    let updated = perm_svc.update_permission(&svr_ctx, &perm.id, PermissionForUpdate {
        name: Some("新增用户-更新".into()),
        sort_order: Some(2),
        ..Default::default()
    }).await?;

    // 权限使用统计（每个权限被多少角色使用）
    let stats = perm_svc.get_permission_usage_stat().await?;
    for s in stats.iter().take(5) {
        println!("权限 {} 被角色使用数: {}", s.permission_code, s.role_count);
    }

    // 删除权限（批量）
    perm_svc.delete_permission(&svr_ctx, &[perm.id, child.id]).await?;

    Ok(())
}
```

### 四、用户角色关联

#### 4.1 永久角色分配

```rust
use cmx_iam::UserServiceImpl;
use cmx_core::SVRContext;

async fn assign_roles_demo(user_svc: &UserServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // assign_roles 为全量替换：传入的 role_ids 完全覆盖用户原有角色
    // 空数组表示清空所有角色
    user_svc.assign_roles(&svr_ctx, "alice", &[
        "role_id_001".into(),
        "role_id_002".into(),
    ]).await?;
    println!("角色分配完成");

    // 查询用户的角色列表（按 username 查询）
    let roles = user_svc.get_user_roles("alice").await?;
    for r in &roles {
        println!("用户拥有角色: {} ({})", r.name, r.code);
    }

    Ok(())
}
```

#### 4.2 临时角色授权（带有效期）

```rust
use chrono::{Duration, Utc};
use cmx_iam::{UserServiceImpl, TempAssignmentStatusFilter};
use cmx_core::SVRContext;

async fn temp_role_demo(user_svc: &UserServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    let now = Utc::now();
    let until = now + Duration::days(7); // 7 天有效期

    // 分配临时角色（带有效期、原因、来源）
    let assignment = user_svc.assign_temp_role(
        &svr_ctx,
        "user_id_001",       // user_id
        "role_id_auditor",   // role_id
        now,                 // effective_from
        until,               // effective_until
        Some("季度审计临时授权"), // reason
        "manual",            // source
    ).await?;
    println!("临时授权 ID: {}", assignment.id);

    // 查询用户的临时授权列表（按状态过滤）
    let active = user_svc.get_user_temp_assignments(
        "user_id_001",
        TempAssignmentStatusFilter::Active, // 生效中
    ).await?;
    println!("生效中临时授权: {} 条", active.len());

    let expired = user_svc.get_user_temp_assignments(
        "user_id_001",
        TempAssignmentStatusFilter::Expired, // 已过期
    ).await?;

    // 延长临时授权有效期
    let new_until = until + Duration::days(3);
    user_svc.extend_temp_role(
        &svr_ctx,
        &assignment.id,
        new_until,
        Some("审计延期"),
    ).await?;

    // 撤销临时角色（逻辑撤销 status=0）
    user_svc.revoke_temp_role(&svr_ctx, &assignment.id, Some("任务结束")).await?;

    // 批量撤销临时角色
    let assignment_ids = vec![assignment.id.clone()];
    let affected = user_svc.revoke_temp_roles_batch(&svr_ctx, &assignment_ids, None).await?;
    println!("批量撤销: {} 条", affected);

    // 查询角色被授权的用户列表（临时授权）
    let assigned_users = user_svc.get_role_temp_assigned_users(
        "role_id_auditor",
        TempAssignmentStatusFilter::All,
    ).await?;

    Ok(())
}
```

### 五、角色权限关联

```rust
use cmx_iam::RoleServiceImpl;
use cmx_core::SVRContext;

async fn role_permission_demo(role_svc: &RoleServiceImpl) -> Result<(), Box<dyn std::error::Error>> {
    let svr_ctx = SVRContext::default();

    // assign_permissions 为全量替换：传入的 permission_ids 完全覆盖角色原有权限
    // 空数组表示清空所有权限
    role_svc.assign_permissions(
        &svr_ctx,
        "role_id_editor", // role_id
        &[
            "perm_id_view".into(),
            "perm_id_edit".into(),
            "perm_id_publish".into(),
        ],
    ).await?;
    println!("权限分配完成");

    // 查询角色的权限列表
    let perms = role_svc.get_role_permissions("role_id_editor").await?;
    for p in &perms {
        println!("角色拥有权限: {} ({})", p.name, p.code);
    }

    // 比较两个角色的权限差异（审计查询）
    let diff = role_svc.get_permission_diff("role_id_editor", "role_id_viewer").await?;
    println!("仅 editor 拥有的权限: {} 条", diff.only_in_role_1.len());
    println!("仅 viewer 拥有的权限: {} 条", diff.only_in_role_2.len());
    println!("共同权限: {} 条", diff.common.len());

    Ok(())
}
```

### 六、错误处理

`IamError` 使用 `thiserror` 定义，覆盖业务错误、数据库错误、规则违反等场景，并提供到 `cmx_api_types::Error` 和 `cmx_traits::error::TraitError` 的转换。

```rust
use cmx_iam::{IamError, UserServiceImpl, user::UserForCreate};
use cmx_core::SVRContext;

async fn error_handling_demo(user_svc: &UserServiceImpl) {
    let svr_ctx = SVRContext::default();

    // 尝试创建已存在的用户名
    let result = user_svc.create_user(&svr_ctx, UserForCreate {
        username: "existing_user".into(),
        password: "P@ssw0rd".into(),
        ..Default::default()
    }).await;

    match result {
        Ok(user) => println!("创建成功: {}", user.id),
        Err(e) => {
            // e 为 cmx_traits::error::TraitError（Service trait 返回类型）
            // 可通过模式匹配区分错误类型
            match &e {
                cmx_traits::error::TraitError::Business(msg) => {
                    // 用户名已存在等业务错误
                    eprintln!("业务错误: {}", msg);
                }
                cmx_traits::error::TraitError::NotFound(msg) => {
                    eprintln!("资源不存在: {}", msg);
                }
                cmx_traits::error::TraitError::Forbidden(msg) => {
                    // 如删除内置角色
                    eprintln!("禁止操作: {}", msg);
                }
                cmx_traits::error::TraitError::Internal(msg) => {
                    eprintln!("内部错误: {}", msg);
                }
                _ => eprintln!("其他错误: {}", e),
            }
        }
    }

    // IamError 的主要变体说明：
    // - UserNotFound(String): 用户不存在
    // - RoleNotFound(String): 角色不存在
    // - PermissionNotFound(String): 权限不存在
    // - RoleGroupNotFound(String): 角色组不存在
    // - UsernameExists(String): 用户名已存在
    // - RoleCodeExists(String): 角色编码已存在
    // - PermissionCodeExists(String): 权限编码已存在
    // - RoleGroupInUse: 角色组下存在子组或关联角色，无法删除
    // - CannotDeleteBuiltinRole: 不能删除系统内置角色
    // - PasswordHashError(String): 密码哈希失败
    // - RuleViolation { rule_code, message }: 权限规则违反（SoD）
    // - Crud(ServiceError): 数据库操作错误
    // - Business(String): 其他 IAM 业务错误
}
```

### 七、与 cmx-auth 集成（通过 trait）

`cmx-iam` 通过实现 `cmx_traits::auth::UserAuthQuery` 和 `cmx_traits::iam::PermissionChecker` 两个 trait，与 `cmx-auth` 解耦集成。`cmx-auth` 在认证流程中通过 trait 对象查询用户数据和校验权限。

```rust
use std::sync::Arc;
use cmx_iam::{IamConfig, IamChecker, UserAuthQueryImpl};
use cmx_database::DatabaseManager;
use cmx_traits::auth::UserAuthQuery;
use cmx_traits::iam::PermissionChecker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mm: Arc<DatabaseManager> = /* DatabaseManager */;
    let config = IamConfig::default();

    // 1. 构造 UserAuthQueryImpl（实现 UserAuthQuery trait）
    //    供 cmx-auth 在登录流程中查询用户、角色、权限
    let auth_query = UserAuthQueryImpl::new(mm.clone(), &config).await?;

    // 通过 trait 对象调用
    let user_data = auth_query.get_user_by_username("alice").await?;
    if let Some(data) = user_data {
        println!("用户ID: {}, 密码哈希存在: {}",
            data.user_id, data.password_hash.is_some());
    }

    // 查询用户角色码（合并永久 + 临时有效角色）
    let role_codes = auth_query.get_user_role_codes("user_id_001").await?;
    println!("角色码: {:?}", role_codes);

    // 查询用户权限码（合并永久 + 临时角色权限）
    let perm_codes = auth_query.get_user_permissions("user_id_001").await?;
    println!("权限码: {:?}", perm_codes);

    // 更新密码哈希（改密后调用）
    auth_query.update_password_hash("user_id_001", "new_hash_value").await?;

    // 更新最后登录信息
    auth_query.update_last_login("user_id_001", "192.168.1.1").await?;

    // 2. 构造 IamChecker（实现 PermissionChecker trait）
    //    供 cmx-auth 在鉴权中间件中校验权限/角色
    let checker = IamChecker::new(mm.clone(), config).await;

    // 校验用户是否拥有某权限（system:all 超级权限短路）
    let has_perm = checker.has_permission("user_id_001", "system:user:add").await?;
    println!("是否有 system:user:add 权限: {}", has_perm);

    // 校验用户是否拥有某角色
    let has_role = checker.has_role("user_id_001", "admin").await?;
    println!("是否有 admin 角色: {}", has_role);

    // 获取用户完整权限列表（合并永久 + 临时）
    let all_perms = checker.get_user_permissions("user_id_001").await?;

    Ok(())
}
```

#### 集成 OAuth2 自动注册与超管创建

```rust
use std::sync::Arc;
use cmx_iam::UserAuthQueryImpl;
use cmx_database::DatabaseManager;
use cmx_traits::auth::{OAuth2UserInfo, UserAuthQuery};

async fn oauth2_demo(
    auth_query: &UserAuthQueryImpl,
) -> Result<(), Box<dyn std::error::Error>> {
    // 创建超管账号（事务保证原子性：创建用户 + 关联角色）
    auth_query.create_super_admin(
        "superadmin",
        "$argon2id$hash_value", // 密码哈希
        Some("admin@example.com"),
        &["admin".into()], // 角色 code 列表
    ).await?;

    // OAuth2 自动注册用户（事务保证原子性：创建用户 + 关联默认角色）
    let user_info = OAuth2UserInfo {
        provider_user_id: "github_12345".into(),
        username: Some("octocat".into()),
        display_name: Some("The Octocat".into()),
        email: Some("octocat@github.com".into()),
        default_role: Some("viewer".into()), // 自动关联的默认角色 code
    };
    let user_id = auth_query.create_user_from_oauth2("github", &user_info).await?;
    println!("OAuth2 注册成功: user_id={}", user_id);

    Ok(())
}
```

### 八、事务处理

`cmx-iam` 在涉及多表原子操作的场景（如超管创建、OAuth2 注册、临时授权清理）使用 `DatabaseManager` 的事务上下文保证原子性。以下示例展示事务模式（摘自 `UserAuthQueryImpl::create_super_admin` 的核心逻辑）：

```rust
use std::sync::Arc;
use cmx_core::model::cell::DataValue;
use cmx_database::DatabaseManager;
use cmx_utils::snowflake_id_str;

async fn transaction_demo(
    mm: &DatabaseManager,
    db_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 获取事务上下文
    let txn_ctx = mm.get_transaction_context();

    // 2. 开启事务（带 guard，自动管理提交/回滚）
    let guard = txn_ctx.begin_with_guard(db_id).await
        .map_err(|e| format!("开启事务失败: {}", e))?;
    let txn_id = guard.txn_id(); // 后续所有 SQL 需携带此 txn_id

    // 3. 在事务内执行多条 SQL（任一失败则整体回滚）
    let user_id = snowflake_id_str();
    let insert_user_sql = "INSERT INTO cmx_user (id, username, password_hash, status, archived) \
                           VALUES ($1, $2, $3, 1, 0)";
    let params: Vec<DataValue> = vec![
        DataValue::String(user_id.clone()),
        DataValue::String("new_user".into()),
        DataValue::String("hash_value".into()),
    ];
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), insert_user_sql, params).await?;

    // 关联角色（同事务内）
    let insert_ur_sql = "INSERT INTO cmx_user_role (id, user_id, role_id, archived) \
                         VALUES ($1, $2, $3, 0)";
    let ur_params: Vec<DataValue> = vec![
        DataValue::String(snowflake_id_str()),
        DataValue::String(user_id),
        DataValue::String("role_id_001".into()),
    ];
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), insert_ur_sql, ur_params).await?;

    // 4. 提交事务（失败时 guard 析构会自动回滚）
    guard.commit().await
        .map_err(|e| format!("提交事务失败: {}", e))?;

    Ok(())
}
```

### 九、互斥规则校验（SoD）

```rust
use std::sync::Arc;
use cmx_iam::{
    IamConfig, RuleEnforcerImpl,
    rule::{
        ExclusionRuleServiceImpl, CreateExclusionRuleRequest,
        ValidateRuleRequest,
    },
};
use cmx_core::SVRContext;
use cmx_database::DatabaseManager;

async fn rule_demo(
    mm: Arc<DatabaseManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = IamConfig { enable_sod_check: true, ..Default::default() };
    let svr_ctx = SVRContext::default();

    // 1. 构造规则服务与校验引擎
    let rule_svc = ExclusionRuleServiceImpl::new(mm.clone(), config.clone()).await;
    let enforcer = Arc::new(RuleEnforcerImpl::new(mm.clone(), config).await);

    // 2. 创建互斥规则（功能权限互斥：付款审批 与 提交付款 不能同时拥有）
    let rule = rule_svc.create_rule(&svr_ctx, CreateExclusionRuleRequest {
        code: "payment_sod".into(),
        name: "付款职责分离".into(),
        subject_type: "permission".into(), // permission | role
        primary_subject_id: "perm_pay_approve".into(),
        violation_message: Some("付款审批与付款提交不能由同一人担任".into()),
        priority: Some(100),
        description: Some("SoD 职责分离规则".into()),
        excluded_subject_ids: vec!["perm_pay_submit".into()],
    }).await?;

    // 3. 校验测试：给定权限组合，测试是否违反互斥规则
    let validate_resp = rule_svc.validate_rule(ValidateRuleRequest {
        permission_ids: vec!["perm_pay_approve".into(), "perm_pay_submit".into()],
        role_ids: vec![],
        user_id: None,
    }).await?;

    if validate_resp.passed {
        println!("校验通过，无违反");
    } else {
        for v in &validate_resp.violations {
            println!("违反规则: {} - {}", v.rule_code, v.violation_message);
        }
    }

    // 4. 通过 RuleEnforcer 在分配时校验（UserServiceImpl/RoleServiceImpl 内部调用）
    //    若启用 enable_sod_check，assign_roles/assign_permissions 会自动调用 enforcer
    enforcer.check_role_permissions(&["perm_pay_approve".into(), "perm_pay_submit".into()]).await?;
    enforcer.check_user_roles("user_id_001", &["role_a".into(), "role_b".into()]).await?;

    Ok(())
}
```

### 十、权限校验器与缓存失效

```rust
use std::sync::Arc;
use cmx_iam::{IamConfig, IamChecker};
use cmx_buffer::cache::CacheManager;
use cmx_database::DatabaseManager;

async fn checker_demo(
    mm: Arc<DatabaseManager>,
    cache: Arc<CacheManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = IamConfig {
        permission_cache_ttl_secs: 300,        // 缓存 TTL 300 秒
        circuit_breaker_threshold: 5,          // 连续失败 5 次熔断
        circuit_breaker_reset_secs: 60,        // 60 秒后尝试恢复
        failure_mode: cmx_iam::config::FailureMode::FailClose, // 故障封闭
        ..Default::default()
    };

    // 构造校验器并注入 Redis 缓存（Builder 模式）
    let checker = IamChecker::new(mm, config).await.with_cache(cache);

    // 校验权限（优先查缓存，未命中查 DB 并回填，带随机抖动 TTL 防雪崩）
    let has = checker.has_permission("user_id_001", "system:user:add").await?;

    // 当用户角色/权限变更时，主动失效缓存
    checker.invalidate_user_cache("user_id_001").await;

    // 当某角色变更时，失效所有关联该角色的用户缓存
    checker.invalidate_role_cache("role_id_editor").await;

    Ok(())
}
```

### 十一、临时授权清理任务

```rust
use std::sync::Arc;
use cmx_iam::scheduler::start_assignment_cleanup;
use cmx_audit::AuditLogger;
use cmx_database::DatabaseManager;

async fn scheduler_demo(
    mm: Arc<DatabaseManager>,
    audit: Option<Arc<dyn AuditLogger>>,
) {
    let db_id = "default_db_id".to_string();

    // 启动后台清理任务（tokio::spawn）
    // 首次延迟 60 秒，之后按间隔执行；将过期的临时授权 status 置为 0
    let handle = start_assignment_cleanup(
        mm.clone(),
        db_id,
        3600,   // 执行间隔 3600 秒
        100,    // 审计批量阈值：超过则只记统计
        audit,
    );

    // handle 可用于取消任务（handle.abort()）

    // 也可直接执行一次清理（供测试或手动触发）
    let affected = cmx_iam::scheduler::run_cleanup_once(
        &mm,
        "default_db_id",
        100,
        None,
    ).await.map_err(|e| eprintln!("清理失败: {}", e)).unwrap_or(0);
    println!("本次清理失效记录数: {}", affected);
}
```

### 十二、权限一致性校验

```rust
use std::sync::Arc;
use cmx_iam::permission::consistency_check::{run_consistency_check, ConsistencyReport};
use cmx_database::DatabaseManager;

async fn consistency_demo(
    mm: &DatabaseManager,
    db_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 比对代码中声明的权限（inventory）与 DB cmx_permission 表的记录
    // 缺失时按配置 panic/warn，不自动写 DB
    let report: ConsistencyReport = run_consistency_check(mm, db_id).await?;

    if report.missing_in_db.is_empty() && report.orphan_in_db.is_empty() {
        println!("权限一致性校验通过");
    } else {
        // 代码有、DB 无（会导致权限检查失效）
        for code in &report.missing_in_db {
            eprintln!("DB 缺失权限: {}", code);
        }
        // DB 有、代码无（冗余，可清理）
        for code in &report.orphan_in_db {
            eprintln!("DB 冗余权限: {}", code);
        }

        // 生成缺失权限的 INSERT DDL（开发者 review 后手动执行）
        let ddl = report.to_insert_sql();
        println!("待执行 DDL:\n{}", ddl);
    }

    Ok(())
}
```

---

## 常见问题解答（FAQ）

### Q1: 为什么创建用户时传入的是明文密码，而数据库存的是哈希？

**A**: `UserForCreate.password` 为明文密码（API 层入参），Service 层在 `create_user` 内部会调用 `cmx_traits::auth::AuthService::hash_password`（Argon2id）进行哈希，生成 `UserForInsert.password_hash` 后丢弃明文。`UserForInsert` 才是与数据库列一一对应的结构（derive `Fields`，可直接用于 `GenericCrudService`）。**切勿将明文密码直接写入 `UserForInsert`**。

### Q2: `assign_roles` 和 `assign_permissions` 是增量还是全量替换？

**A**: 全量替换。传入的 `role_ids` / `permission_ids` 会完全覆盖用户/角色的原有关联。传入空数组表示清空所有关联。如需增量操作，请先查询现有关联，合并后再调用。

### Q3: 临时角色授权与永久角色授权有什么区别？

**A**: 永久角色授权存储在 `cmx_user_role` 表，无有效期；临时角色授权存储在 `cmx_user_role_assignment` 表，带 `effective_from` / `effective_until` 有效期和 `status` 状态。权限校验时会合并查询两类授权（临时授权需满足 `NOW() BETWEEN effective_from AND effective_until` 且 `status=1`）。过期临时授权由 `scheduler` 定时清理（`status` 置 0）。

### Q4: `system:all` 超级权限是如何工作的？

**A**: `IamChecker::has_permission` 和 `UserAuthQueryImpl::get_user_permissions` 在校验时会检查用户是否拥有 `system:all` 权限码。若拥有，则短路放行所有权限校验。在熔断器打开的 FailOpen 模式下，也会直查 DB 验证用户是否拥有 `system:all`，仅放行超管用户。

### Q5: 熔断器的 FailOpen 和 FailClose 有什么区别？

**A**: 当数据库/缓存连续失败达到阈值（`circuit_breaker_threshold`）时熔断器打开：
- **FailOpen（故障开放）**：仅放行拥有 `system:all` 的超管用户（直查 DB 验证），其他用户拒绝。适用于「宁可放行也不阻断核心业务」的场景。
- **FailClose（故障封闭，默认）**：全部拒绝，不查 DB，保护系统。适用于「安全优先」的场景。

熔断器经过 `circuit_breaker_reset_secs` 后自动进入半开状态，允许请求通过验证是否恢复。

### Q6: 内置角色为什么不能删除？

**A**: `IamConfig.builtin_role_codes`（默认 `["admin"]`）中的角色编码受保护，`delete_role` 会检查并返回 `IamError::CannotDeleteBuiltinRole`（转换为 `Forbidden`）。这是为了防止误删系统核心角色导致管理失控。如需修改保护列表，调整 `IamConfig.builtin_role_codes`。

### Q7: 互斥规则（SoD）的「1 主对象 + N 互斥对象」模型是什么意思？

**A**: 一条互斥规则包含 1 个主对象（`primary_subject_id`）和 N 个互斥对象（`excluded_subject_ids`）。仅当用户/角色的权限或角色集合**同时包含主对象和任一互斥对象**时才判定违反。互斥对象之间不互斥。例如「付款审批（主）+ 付款提交（互斥）」违反，但「付款提交 + 付款确认」不违反（除非另有规则定义）。

### Q8: 如何启用 OpenAPI 文档生成？

**A**: 在 `Cargo.toml` 中启用 `openapi` feature：`cmx-iam = { features = ["openapi"] }`。启用后，所有 DTO 结构（如 `UserForCreate`、`RoleForCreate`、`UserRoleAssignment` 等）会派生 `utoipa::ToSchema`，可通过 `#[derive(ToSchema)]` 注册到 OpenAPI 文档。

### Q9: 权限一致性校验会自动写 DB 吗？

**A**: 不会。`run_consistency_check` 仅比对代码声明（`inventory`）与 DB 记录，缺失时按配置 `panic`/`warn`，并通过 `ConsistencyReport::to_insert_sql()` 生成 INSERT DDL 供开发者 review 后手动执行。这是为了避免自动写入导致权限污染。

### Q10: 如何在 Service 中注入审计日志？

**A**: 通过 Builder 模式的 `with_audit` 方法注入 `Arc<dyn AuditLogger>`。注入后，所有写操作（create/update/delete/assign）会通过 `AuditHelper::audit_write` 写入审计日志（尽力而为，失败仅 `warn!` 不阻塞业务）。当批量操作超过 `IamConfig.audit_batch_size` 阈值时，聚合为统计记录而非逐条记录。
