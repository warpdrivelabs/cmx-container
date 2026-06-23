# cmx-server-demo 权限系统设计分析

## 一、整体架构

项目采用 **RBAC (Role-Based Access Control)** 模型，结合 **编译期权限注册** 和 **运行期权限检查**，并通过 Valkey(Redis) 进行权限缓存。

```
用户 (user) ──┐
             ├──► user_role ──► role ──► role_permission ──► permission
角色 (role)──┘
```

权限数据流：

```
HTTP 请求 → 认证 (token) → 加载用户权限 (DB/缓存) → 注入 Ctx
                                                        │
                                                        ▼
                                         路由 handler ──► require_permission(key)
                                                        │
                                              通过(根用户/管理员) / 拒绝
```

---

## 二、核心数据模型

权限相关表结构都定义在 `crates/libs/lib-core/src/model/acs/`：

| 表 | 说明 | 关键字段 |
|---|---|---|
| `permission` | 权限定义表 | `key`(唯一)、`group_name`、`display_name`、`description` |
| `role` | 角色表 | `name`(唯一)、`display_name` |
| `user_role` | 用户-角色关联表 | `user_id`、`role_id` |
| `role_permission` | 角色-权限关联表 | `role_id`、`permission_id` |

### 权限 key 命名规范

采用 `资源:动作` 格式：

- `user:create` / `user:read` / `user:update` / `user:delete` / `user:list`
- `role:create` / `role:read` / ...
- `permission:list` / `role_permission:set_for_role` / `user_role:set_for_user`
- `menu:system:user` / `menu:dashboard:overview` (前端菜单可见性控制)
- `agent:export` / `agent:archive` (自定义业务权限)

详见：
- [permission.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/permission.rs)
- [role.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/role.rs)
- [user_role.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/user_role.rs)
- [role_permission.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/role_permission.rs)

---

## 三、编译期权限注册

### 3.1 inventory 全局收集

通过 `inventory` crate 在编译期将所有权限注册到全局集合中，无需手动维护列表。

定义在 [mod.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/mod.rs#L48-L58)：

```rust
pub struct RegisteredPermission {
    pub key: &'static str,
    pub group: &'static str,
    pub display: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

inventory::collect!(RegisteredPermission);
```

### 3.2 注册方式

#### 方式一：过程宏（推荐）

在 RPC 处理器上使用 `#[lib_macros::permission(key, group, display, description)]`：

```rust
#[lib_macros::permission(
    key = "user:create",
    group = "用户管理",
    display = "创建用户",
    description = "创建新用户账户"
)]
pub async fn create_user(ctx: Ctx, mm: ModelManager, params: CreateUserParams) -> ... { ... }
```

宏实现：[permission.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-macros/src/permission.rs#L147-L181) 同时完成两件事：
1. `inventory::submit!` 注册 `RegisteredPermission`
2. 在函数体首行注入 `ctx.require_permission("user:create")?;`

#### 方式二：声明式 submit

在 [menu_permissions.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/menu_permissions.rs) 中直接 `inventory::submit!`，通常用于纯菜单/UI 权限：

```rust
::inventory::submit! {
    ::lib_core::model::acs::RegisteredPermission {
        key: "menu:system:user",
        group: "系统菜单",
        display: "用户管理菜单",
        description: "访问用户管理页面",
        source: module_path!(),
    }
}
```

#### 方式三：CRUD 一键注册宏

[sync.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/sync.rs#L180-L281) 中提供 `register_crud_permissions!`，一次性注册 `:create/:read/:update/:delete/:list` 五个权限：

```rust
lib_core::register_crud_permissions!("user", "用户", "用户管理", "用户账户管理");
```

### 3.3 路由处理器注册（启动期校验）

[mod.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/mod.rs#L137-L272) 中定义了 `RegisteredRouteHandler` 注册表，启动时强制每个路由都必须有权限注解，防止漏写：

```rust
pub fn ensure_all_handlers_registered(handler_names: &[&str]) {
    let unregistered = validate_route_handlers(handler_names);
    if !unregistered.is_empty() {
        panic!("没有权限注解的路由处理器: {:?}", unregistered);
    }
}
```

每个 `#[permission]` / `#[rest_permission]` / `#[public]` 宏都会在 `inventory` 中登记一个 `RegisteredRouteHandler`，并在 `kind: Protected` 时要求 `has_check: true`。

---

## 四、运行期权限检查

### 4.1 中间件栈（执行顺序）

见 [main.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/main.rs#L108-L141)：

```
1. mw_req_stamp_resolver      添加请求时间戳
2. CookieManagerLayer         处理 cookies
3. mw_ctx_resolver            从 token 解析用户身份，注入 CtxW
4. mw_permission_resolver     从 DB/缓存加载用户权限，注入 Ctx
5. mw_ctx_require             强制要求已认证（仅 RPC/REST 路由）
6. handler 内 #[permission]   在函数体首行执行 require_permission
```

### 4.2 上下文与权限注入

`Ctx` 结构定义在 [mod.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/ctx/mod.rs#L14-L23)：

```rust
pub struct Ctx {
    user_id: i64,
    conv_id: Option<i64>,
    permissions: Option<UserPermissions>,  // 由权限中间件注入
}
```

`UserPermissions` ([mod.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/mod.rs#L66-L133)) 持有 `roles: Vec<String>` 和 `permissions: HashSet<String>`，提供 `has_permission / has_role` 查询。

### 4.3 三种检查方式

Ctx 中实现了 [mod.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/ctx/mod.rs#L117-L254) 三种检查 API：

| 方法 | 行为 |
|---|---|
| `require_permission(key)` | 必须拥有指定权限 |
| `require_all_permissions(&[keys])` | 必须拥有所有指定权限 |
| `require_any_permission(&[keys])` | 拥有任一权限即可 |

**关键规则**（短路）：

```rust
pub fn require_permission(&self, key: &str) -> Result<()> {
    // 1. 根用户（user_id == 0）绕过所有检查
    if self.user_id == 0 { return Ok(()); }

    // 2. 权限未加载视为失败
    let permissions = self.permissions.as_ref()
        .ok_or(Error::PermissionsNotLoaded)?;

    // 3. admin 角色绕过所有检查
    if permissions.has_role("admin") { return Ok(()); }

    // 4. 业务权限检查
    if permissions.has_permission(key) { Ok(()) }
    else { Err(Error::PermissionDenied { user_id, permission }) }
}
```

---

## 五、Valkey 缓存层

### 5.1 缓存 Key 与 TTL

定义在 [cache_keys.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-valkey-core/src/cache_keys.rs)：

| Key 模式 | TTL | 用途 |
|---|---|---|
| `perm:user:{user_id}` | 300s | 用户完整权限集合（角色名 + 权限键） |
| `role:user:{user_id}` | 300s | 用户角色 |
| `session:user:{user_id}` | 24h | 用户会话 |
| `user:profile:{user_id}` | 600s | 用户资料 |
| `perm:role:{role_id}` | 600s | 角色权限 |
| `token:blacklist:{hash}` | 7d | 令牌黑名单 |
| `rate:limit:{key}` | 60s | 限流 |

### 5.2 缓存加载中间件

[mw_permission.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-web/src/middleware/mw_permission.rs) 提供两个版本：

- `mw_permission_resolver`：直连 DB
- `mw_permission_resolver_with_cache`：先查 Valkey，miss 时回源 DB 并回写

通过环境变量 `SERVICE_PERMISSION_CACHE_ENABLED` 切换（见 [config.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/config.rs)）。

### 5.3 缓存失效

[permission_cache.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-web/src/utils/permission_cache.rs) 提供两个失效函数：

| 触发时机 | 失效函数 | 调用点 |
|---|---|---|
| 修改用户角色（`set_roles_for_user`） | `invalidate_user_permissions_cache` | [user_role_rpc.rs#L80-L82](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/rpcs/user_role_rpc.rs#L80-L82) |
| 修改角色权限（`set_permissions_for_role`） | `invalidate_users_permissions_cache`（先查出所有用户） | [role_permission_rpc.rs#L88-L91](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/rpcs/role_permission_rpc.rs#L88-L91) |

`PermissionCachePool` ([routes_rpc.rs#L10-L12](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/routes_rpc.rs#L10-L12)) 通过 `rpc_router::FromResources` 注入到 RPC handler，handler 可选接收以执行失效。

---

## 六、启动期权限同步

[main.rs#L99-L102](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/main.rs#L99-L102) 在服务启动时调用 `PermissionBmc::sync_from_registry`：

- 收集 `inventory` 中所有 `RegisteredPermission`
- 与 DB 中已有权限对比：
  - 新增：代码有、DB 没有 → 插入
  - 更新：元数据变化（group/display/description）→ 更新
  - 删除：DB 有、代码没有 → 删除权限并级联清理 `role_permission`

实现见 [permission.rs#L343-L429](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/permission.rs#L343-L429)。

> 另一种非破坏式版本 `sync_permissions` ([sync.rs#L25-L89](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-core/src/model/acs/sync.rs#L25-L89)) 仅创建/警告孤儿，不删除。

---

## 七、RPC 权限管理接口

通过 [routes_rpc.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/routes_rpc.rs) 统一挂在 `/api/rpc` 路径下。

### 7.1 权限管理 RPC

| 方法 | 权限 key | 行为 |
|---|---|---|
| `list_all_permissions` | `permission:list` | 列出系统所有权限 |
| `list_permissions_for_role` | `role_permission:list_for_role` | 查询某角色拥有权限 |
| `set_permissions_for_role` | `role_permission:set_for_role` | 覆盖设置角色权限（事务） |

### 7.2 用户-角色管理 RPC

| 方法 | 权限 key | 行为 |
|---|---|---|
| `list_roles_for_user` | `user_role:list_for_user` | 查询某用户角色 |
| `set_roles_for_user` | `user_role:set_for_user` | 覆盖设置用户角色（事务 + 缓存失效） |

### 7.3 角色 CRUD

通过 `generate_common_rpc_fns!` 一键生成 5 个标准接口，权限由 `register_crud_permissions!` 注册到 `role:create/:read/...`。

### 7.4 用户 CRUD

[user_rpc.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/rpcs/user_rpc.rs) 自定义实现，权限手动标注。

---

## 八、REST 权限控制

REST 路由通过 [routes_rest.rs](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/routes_rest.rs) 挂在 `/api/rest` 下。`CtxW` 包装器（[mw_auth.rs#L119-L143](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/libs/lib-web/src/middleware/mw_auth.rs#L119-L143)）使用 `ctx.0` 访问 `Ctx`，因此 REST 专用宏前缀 `rest_*`：

| 宏 | 等价于 |
|---|---|
| `#[lib_macros::rest_permission(...)]` | `#[permission]` + 自动 `ctx.0.require_permission` |
| `#[lib_macros::rest_require_permission(...)]` | `#[require_permission]` |
| `#[lib_macros::rest_require_permissions(...)]` | `#[require_permissions]` |
| `#[lib_macros::rest_require_any_permission(...)]` | `#[require_any_permission]` |
| `#[lib_macros::public]` | 标记为公开，跳过权限检查（仍需注册用于启动校验） |

CRUD 通过 `generate_common_rest_fns!` 批量生成，自定义端点通过 `generate_custom_rest_routes!` 手动注册（同时强制校验权限注解），见 [agent_rest.rs#L108-L121](file:///media/yqs/工作/rustspace/cmx-server-demo/cmx-server-demo/crates/services/web-server/src/web/rests/agent_rest.rs#L108-L121)。

---

## 九、关键设计亮点

1. **编译期发现漏写权限**：`inventory` + `ensure_all_handlers_registered` 在启动时 panic，确保所有路由都有权限/公开注解。
2. **零运行时反射**：`require_permission` 是普通函数调用，宏在编译期生成代码。
3. **二级短路**：根用户（`user_id=0`）+ `admin` 角色绕过所有业务权限检查。
4. **缓存分层**：`mw_permission_resolver` vs `mw_permission_resolver_with_cache` 可通过环境变量切换，不影响业务代码。
5. **缓存一致性**：写接口（`set_roles_for_user` / `set_permissions_for_role`）在事务提交后主动失效对应用户的缓存。
6. **DB ↔ Code 同步**：`sync_from_registry` 在启动时把代码里的权限声明同步到 DB，支持删库重建。
7. **菜单权限与接口权限统一**：通过 `RegisteredPermission` 同时表达后端接口权限和前端菜单可见性权限，避免双轨制。

---

## 十、权限流转时序图

```
┌─────────┐  1.HTTP   ┌──────────────────┐
│ Client  │ ─────────►│ mw_ctx_resolver  │
└─────────┘           │  - parse token   │
                      │  - load UserForAuth
                      │  - validate token │
                      │  - CtxW(Ctx{user_id})
                      └────────┬─────────┘
                               │
                               ▼
                      ┌────────────────────────────┐
                      │ mw_permission_resolver     │
                      │  - check Valkey cache      │
                      │  - miss → DB JOIN query    │
                      │  - build UserPermissions   │
                      │  - CtxW(Ctx{user_id, perms})│
                      └────────┬───────────────────┘
                               │
                               ▼
                      ┌──────────────────┐
                      │ mw_ctx_require   │  强制要求 Ctx 解析成功
                      └────────┬─────────┘
                               │
                               ▼
                      ┌────────────────────────────┐
                      │ Handler                    │
                      │   #[permission(...)]       │
                      │   ├─ ctx.require_perm(key) │ ←─ 宏注入
                      │   └─ 业务逻辑              │
                      └────────────────────────────┘
```
