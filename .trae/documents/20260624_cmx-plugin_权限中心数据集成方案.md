# 权限中心数据集成方案 — center_client 实现

## 一、需求概述

将 `cmx-plugin/center_client` 中的 Mock 实现替换为真实实现，支持通过 **HTTP form-data** 和 **gRPC** 两种传输方式将插件 `permdata/` 目录下的权限数据发送到权限中心服务。权限中心接收 ZIP 后解压解析，自动判断新增/更新/移除（通过比对数据库与文件中的权限 code），在事务中完成所有数据库操作（包括清理角色-权限关联），返回导入结果。

### 核心约束

- 传输方式：同时支持 HTTP form-data + 服务发现 和 gRPC proto 扩展
- 接收端：当前应用新增 HTTP 端点 + gRPC 方法，复用 cmx-iam 已有 PermissionService
- 操作判断：接收端自动判断 — DB 中无 code 则新增，有 code 则更新，DB 中有但文件中无则删除
- 事务保证：所有数据库操作（新增/更新/删除权限 + 删除角色-权限关联）在单个事务中完成
- 文件携带：发送时携带 domain_code、application_code、module_code 等元数据
- 权限文件格式：需制定 JSON 格式规范

---

## 二、现状分析

### 2.1 当前 center_client 架构

| 组件 | 状态 | 说明 |
|------|------|------|
| `ServiceCenterSender` trait | ✅ 已定义 | `send_data` + `cleanup_data` 两个方法 |
| `MockServiceCenterSender` | ✅ 已实现 | 始终返回 success=true |
| `CenterDataDispatcher` | ✅ 已实现 | 并行分发到 4 个中心 |
| `CenterClientConfig` | ✅ 已定义 | 支持 mock/url/discovery 三种 mode |
| `manager.rs` 初始化 | ⚠️ 硬编码 Mock | 加载了配置但未按 mode 选择 sender |
| `HttpServiceCenterSender` | ❌ 未实现 | — |
| `GrpcServiceCenterSender` | ❌ 未实现 | — |

### 2.2 当前权限数据模型

**cmx_permission 表**：`id, code(唯一), name, resource_type, parent_id, sort_order, description, domain_code, app_code, module_code, extension, status, archived`

**cmx_role_permission 表**：`id, role_id, permission_id, archived`

**关键索引**：`uk_cmx_permission_code (code)`, `idx_cmx_permission_domain_code`, `idx_cmx_permission_app_code`, `idx_cmx_permission_module_code`

### 2.3 已有基础设施

- **事务模式**：`DatabaseManager::get_transaction_context()` → `begin_with_guard(db_id)` → `TransactionGuard` (RAII，Drop 自动回滚)
- **PermissionService**：已有 `create_permission`、`update_permission`、`delete_permission`（含事务：软删除权限 + 物理删除角色关联）
- **服务发现**：`GlobalServiceInstanceCache::get()` 返回 `&'static Arc<ServiceInstanceCache>`，纯内存 O(1) 读取；`GlobalServiceRegistry::get()` 返回 `&'static Arc<dyn ServiceRegistry>`，可查询实例
- **ServiceInstance 结构**：字段 `ip: String`、`port: u16`、`service_name: String`、`group_name: Option<String>`、`cluster_name: Option<String>`、`weight: f64`、`metadata: HashMap<String, String>`、`healthy: bool`、`ephemeral: bool`。无内置辅助方法，调用方需自行拼接 HTTP URL
- **gRPC 框架**：`VoloGrpcClient`（持有 `Arc<ServiceInstanceCache>` + `Arc<dyn ServiceRegistry>`，double-check locking 缓存客户端）+ `CmxOrchestratorServiceImpl`（依赖 `ServiceInvoker`/`RuntimeInvoker`/`PluginQuery`）+ `GlobalRpcClient`（`OnceLock<Arc<dyn RpcClient>>`）
- **init_rpc 最新实现**：预热服务使用 `registry.subscribe_instances()` 而非手动 query+update；当前未启动 `ServiceListSyncer`
- **HTTP 框架**：axum `Multipart` extractor，`CmxAppState` 注入模式
- **reqwest**：workspace 依赖 (0.12, json + rustls-tls)，cmx-plugin 已引用

### 2.4 缺失项

- `permdata/` 目录在所有模板和 demo 中均不存在，无示例权限数据文件
- `CenterCleanupRequest` 缺少 `domain_code`/`application_code`/`module_code` 字段（清理时无法定位数据）
- gRPC proto 仅有 `ExecuteService`/`CallFunction`，无文件上传方法
- `CmxOrchestratorServiceImpl` 无数据导入依赖

### 2.5 最新代码 API 参考（基于最新 cmx-rpc + cmx-registry-config）

| API | 最新签名 | 备注 |
|-----|---------|------|
| `GlobalServiceInstanceCache::get()` | `-> &'static Arc<ServiceInstanceCache>` | 返回静态 Arc 引用，未初始化时 panic |
| `GlobalServiceRegistry::get()` | `-> &'static Arc<dyn ServiceRegistry>` | 返回静态 Arc 引用（注意：类型名是 `GlobalServiceRegistry` 不是 `GlobalRegistry`） |
| `ServiceInstanceCache::get()` | `(&str) -> Option<Vec<ServiceInstance>>` | 纯内存 O(1) |
| `ServiceInstanceCache::get_or_fetch()` | `(&str, F) -> Result<Vec<ServiceInstance>, RegistryError>` | 懒加载 |
| `ServiceRegistry::query_instances()` | `(&str, Option<&str>, Vec<String>) -> Result<Vec<ServiceInstance>, RegistryError>` | 网络查询 |
| `ServiceRegistry::subscribe_instances()` | `(&str, InstanceChangeCallback) -> Result<(), RegistryError>` | 订阅变更 |
| `ServiceInstance` 字段 | `ip: String, port: u16, service_name: String, group_name: Option<String>, cluster_name: Option<String>, weight: f64, metadata: HashMap<String,String>, healthy: bool, ephemeral: bool` | 无内置方法 |
| `start_grpc_server()` | `(u16, Arc<dyn ServiceInvoker>, Arc<dyn RuntimeInvoker>, Arc<dyn PluginQuery>, oneshot::Sender<()>) -> Result<(), RpcFrameworkError>` | 预绑定端口模式 |
| `VoloGrpcClient::new()` | `(Arc<ServiceInstanceCache>, GrpcConfig, Arc<dyn ServiceRegistry>) -> Self` | 三参数构造 |
| `RpcConfig` | `{ enabled, protocol, grpc: GrpcConfig, http_rest: HttpRestConfig, warmup_services: Vec<String> }` | 注意：无 `service_sync_interval_secs` 字段 |
| `init_rpc()` | `(Arc<dyn ServiceInvoker>, Arc<dyn RuntimeInvoker>, Arc<dyn PluginQuery>) -> Result<Option<u16>>` | 预热用 `subscribe_instances`，未启动 `ServiceListSyncer` |

---

## 三、权限文件格式设计

### 3.1 文件结构

`permdata/` 目录下放置一个或多个 `.json` 文件，每个文件描述一组权限定义。ZIP 包内保持相对路径结构。

### 3.2 JSON 格式规范

```json
{
  "name": "用户管理插件权限定义",
  "version": "1.0.0",
  "description": "用户管理模块的权限定义",
  "permissions": [
    {
      "code": "user:list",
      "name": "用户列表",
      "resource_type": "api",
      "parent_code": null,
      "sort_order": 1,
      "description": "查看用户列表",
      "extension": null,
      "status": 1
    },
    {
      "code": "user:create",
      "name": "创建用户",
      "resource_type": "api",
      "parent_code": null,
      "sort_order": 2,
      "description": "创建新用户",
      "extension": null,
      "status": 1
    },
    {
      "code": "user:delete",
      "name": "删除用户",
      "resource_type": "button",
      "parent_code": "user:list",
      "sort_order": 3,
      "description": "删除指定用户（危险操作）",
      "extension": "{\"dangerous\":true,\"confirm_required\":true}",
      "status": 1
    },
    {
      "code": "user:export",
      "name": "导出用户",
      "resource_type": "button",
      "parent_code": "user:list",
      "sort_order": 4,
      "description": "导出用户数据",
      "extension": null,
      "status": 0
    },
    {
      "code": "user:detail",
      "name": "用户详情",
      "resource_type": "menu",
      "parent_code": "user:list",
      "sort_order": 5,
      "description": "查看用户详情页面",
      "extension": null,
      "status": 1
    }
  ]
}
```

> **ZIP 多文件说明**：`permdata/` 目录下可放置多个 JSON 文件（如 `user-perms.json` + `role-perms.json`），接收端合并所有文件的 `permissions` 列表后统一处理。`parent_code` 可跨文件引用。

### 3.3 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 文件描述名称（元数据，不入库） |
| `version` | string | 是 | 文件版本（元数据，不入库） |
| `description` | string | 否 | 文件描述（元数据，不入库） |
| `permissions` | array | 是 | 权限定义列表 |
| `permissions[].code` | string | 是 | 权限编码（**全局唯一**，必须使用 `{module}:action` 带模块前缀的命名规范，如 `user:list`） |
| `permissions[].name` | string | 是 | 权限名称 |
| `permissions[].resource_type` | string | 否 | 资源类型：`api`/`menu`/`button`，默认 `api` |
| `permissions[].parent_code` | string\|null | 否 | 父权限编码（用 code 引用，接收端解析为 parent_id） |
| `permissions[].sort_order` | int | 否 | 排序序号，默认 0 |
| `permissions[].description` | string | 否 | 权限描述 |
| `permissions[].extension` | string\|null | 否 | 扩展配置（JSON 字符串） |
| `permissions[].status` | int | 否 | 状态：1-启用，0-禁用，默认 1 |

### 3.4 设计决策

- **权限 code 全局唯一约定**：DDL 中 `uk_cmx_permission_code` 约束 code 全局唯一（不含三元组）。要求插件开发者使用 `{module}:action` 带模块前缀的命名规范（如 `user:list`、`billing:pay`），确保不同插件的 code 不冲突。导入时校验 code 格式，发现冲突返回明确错误。
- **比对按三元组，操作按 id**：导入时按 `domain_code + app_code + module_code` 查询当前插件作用域下的权限集合；所有 UPDATE/DELETE 操作**必须按 id 定位**（从查询结果取 id），禁止仅用 code 定位，避免误操作其他插件的记录。
- **用 `parent_code` 而非 `parent_id`**：文件中使用 code 引用父权限，接收端解析为 ID。避免插件开发者需要知道数据库生成的 ID。
- **不包含 `action` 字段**：接收端自动判断操作类型（新增/更新/删除）。
- **不包含 `domain_code`/`app_code`/`module_code`**：这些在发送时通过 form-data/proto 元数据携带，不在文件内重复。
- **ZIP 内可包含多个 JSON 文件**：接收端合并所有文件的 permissions 列表后统一处理。

---

## 四、架构设计

### 4.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    Sender Side (cmx-plugin)                      │
│                                                                  │
│  CenterClientConfig.mode:                                        │
│  ├── "mock"        → MockServiceCenterSender (已有)              │
│  ├── "http_url"    → HttpServiceCenterSender (新增)              │
│  ├── "http_discovery" → HttpServiceCenterSender (新增)           │
│  └── "grpc"        → GrpcServiceCenterSender (新增)              │
└──────────┬──────────────────────────┬───────────────────────────┘
           │ HTTP form-data           │ gRPC ImportPluginData
           ▼                          ▼
┌─────────────────────────┐  ┌────────────────────────────────────┐
│  HTTP Endpoint (cmx-api) │  │  gRPC Server (cmx-rpc)             │
│  POST /api/iam/          │  │  CmxOrchestratorServiceImpl        │
│    permissions/import    │  │  .import_plugin_data()             │
│  POST /api/iam/          │  │  .cleanup_plugin_data()            │
│    permissions/cleanup   │  │                                    │
└──────────┬───────────────┘  └──────────┬─────────────────────────┘
           │                              │
           │    ┌─────────────────────────┘
           │    │
           ▼    ▼
┌─────────────────────────────────────────────────────────────────┐
│              PluginDataImporter trait (cmx-traits)               │
│  import_data(request) → result                                   │
│  cleanup_data(request) → result                                  │
└──────────┬──────────────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────────┐
│         PluginDataImporterImpl (cmx-iam, 新增)                   │
│  实现 PluginDataImporter trait，按 PluginDataCategory 路由:       │
│  ├── Perm → PermissionServiceImpl.import_permissions()            │
│  │          PermissionServiceImpl.cleanup_permissions()            │
│  └── 其他  → 返回不支持错误                                         │
│  注入 Option<Arc<IamChecker>> 用于精准缓存失效                      │
└──────────┬──────────────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────────┐
│    PermissionServiceImpl (cmx-iam, 新增固有方法)                   │
│  import_permissions():                                           │
│    1. 解压 ZIP → 读取所有 JSON 文件                                │
│    2. 解析为 Vec<PermissionDefinition>                            │
│    3. 事务内查询 DB: WHERE domain_code + app_code + module_code  │
│    4. 比对: file_codes vs db_codes                               │
│    5. 事务内执行:                                                  │
│       - 新增: INSERT cmx_permission (code 不在 DB)                │
│       - 更新: UPDATE cmx_permission (code 在 DB 且有变化)          │
│       - 删除: 物理删除 cmx_permission + 物理删除 cmx_role_permission│
│         (code 在 DB 但不在 file)                                   │
│       - 两阶段解析 parent_code → parent_id                        │
│    6. 提交事务                                                     │
│    7. 返回 ImportResult {created, updated, deleted}               │
│                                                                  │
│  cleanup_permissions():                                          │
│    事务内: 物理删除所有匹配权限 + 物理删除角色关联                   │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 传输层设计

#### HTTP form-data 模式

**配置**：
```toml
[center_client]
mode = "http_url"  # 或 "http_discovery"

[center_client.urls]
perm = "http://localhost:8080/api/iam/permissions/import"

[center_client.discovery]
nacos_group = "DEFAULT_GROUP"
perm_service = "cmx-server"
```

**请求格式** (multipart form-data)：
- `file`: ZIP 二进制文件 (application/zip)
- `plugin_id`: 文本字段
- `app_id`: 文本字段
- `version`: 文本字段
- `domain_code`: 文本字段
- `application_code`: 文本字段
- `module_code`: 文本字段

**地址解析**：
- `http_url` 模式：直接使用 `config.urls.perm` 配置的 URL
- `http_discovery` 模式：通过 `GlobalServiceInstanceCache::get().get(service_name)` 获取实例，拼接 `http://{ip}:{port}/api/iam/permissions/import`

#### gRPC 模式

**Proto 扩展**：
```protobuf
message ImportPluginDataRequest {
  string category = 1;           // "perm", "menu", "form", "flow"
  string domain_code = 2;
  string application_code = 3;
  string module_code = 4;
  string plugin_id = 5;
  string app_id = 6;
  string version = 7;
  bytes zip_data = 8;
}

message ImportPluginDataResponse {
  bool success = 1;
  string message = 2;
  uint32 created_count = 3;
  uint32 updated_count = 4;
  uint32 deleted_count = 5;
}

message CleanupPluginDataRequest {
  string category = 1;
  string domain_code = 2;
  string application_code = 3;
  string module_code = 4;
  string plugin_id = 5;
  string app_id = 6;
}

service CmxServiceOrchestrator {
  rpc ExecuteService(ExecuteServiceRequest) returns (ExecuteServiceResponse);
  rpc CallFunction(CallFunctionRequest) returns (CallFunctionResponse);
  rpc ImportPluginData(ImportPluginDataRequest) returns (ImportPluginDataResponse);   // 新增
  rpc CleanupPluginData(CleanupPluginDataRequest) returns (ImportPluginDataResponse); // 新增
}
```

**客户端调用**：`GrpcServiceCenterSender` 通过 `GlobalRpcClient::get()` 调用 `import_plugin_data()` 方法。

**服务端处理**：`CmxOrchestratorServiceImpl.import_plugin_data()` 委托给 `PluginDataImporter` trait。

### 4.3 Trait 设计

#### cmx-traits 新增 `PluginDataImporter` trait

```rust
// crates/libs/cmx-traits/src/plugin/data_importer.rs

/// 数据类别枚举（与 cmx-plugin::DataCategory 对应，但定义在 cmx-traits 供跨 crate 使用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginDataCategory {
    Menu,
    Perm,
    Form,
    Flow,
}

/// 插件数据导入请求
pub struct PluginDataImportRequest {
    pub category: PluginDataCategory,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub plugin_id: String,
    pub app_id: String,
    pub version: String,
    pub zip_data: Vec<u8>,
}

/// 插件数据清理请求
pub struct PluginDataCleanupRequest {
    pub category: PluginDataCategory,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub plugin_id: String,
    pub app_id: String,
}

/// 插件数据导入结果
pub struct PluginDataImportResult {
    pub success: bool,
    pub message: String,
    pub created_count: u32,
    pub updated_count: u32,
    pub deleted_count: u32,
}

/// 插件数据导入器 trait
#[async_trait]
pub trait PluginDataImporter: Send + Sync {
    async fn import_data(&self, request: PluginDataImportRequest) 
        -> Result<PluginDataImportResult, TraitError>;
    async fn cleanup_data(&self, request: PluginDataCleanupRequest) 
        -> Result<PluginDataImportResult, TraitError>;
}
```

#### cmx-traits 扩展 `RpcClient` trait

```rust
// crates/libs/cmx-traits/src/rpc/client.rs — 新增方法

#[async_trait]
pub trait RpcClient: Send + Sync {
    // ... 已有方法 ...

    /// 导入插件数据到远程服务
    async fn import_plugin_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, RpcError>;

    /// 清理远程服务中的插件数据
    async fn cleanup_plugin_data(
        &self,
        request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, RpcError>;
}
```

### 4.4 导入逻辑详细流程

```
import_permissions(domain_code, app_code, module_code, zip_data)
│
├── 1. 解压 ZIP
│   └── 遍历 ZIP 内所有 .json 文件，读取内容
│       （损坏的 ZIP / 非 JSON 文件 → 返回错误，fail-fast）
│
├── 2. 解析 + 校验 JSON
│   ├── 反序列化为 PermissionFile { permissions: Vec<PermissionDefinition> }
│   ├── 合并所有文件的 permissions 列表
│   ├── 校验：code 非空且含 ":" 分隔符、resource_type ∈ {api,menu,button}
│   ├── 校验：同一文件内 code 不重复
│   └── 收集 file_codes: HashSet<String> (所有权限 code)
│
├── 3. 开启事务（查询和写入在同一事务内，避免并发竞态）
│   │
│   ├── 3.1 事务内查询 DB
│   │   ├── SELECT id, code FROM cmx_permission 
│   │   │   WHERE domain_code = $1 AND app_code = $2 AND module_code = $3
│   │   │   (使用 txn_id 执行，物理删除模式下不限定 archived)
│   │   └── 构建 db_map: HashMap<String(code), String(id)>
│   │
│   ├── 3.2 比对计算
│   │   ├── to_create: file_codes - db_codes (文件有、DB 无)
│   │   ├── to_update: file_codes ∩ db_codes (文件有、DB 有)
│   │   └── to_delete: db_codes - file_codes (DB 有、文件无)
│   │
│   ├── 3.3 第一阶段：INSERT/UPDATE（parent_id 暂置 None）
│   │   ├── 新增 (to_create):
│   │   │   ├── id = cmx_utils::id::snowflake_id_str()
│   │   │   ├── parent_id = None（第二阶段回填）
│   │   │   └── INSERT INTO cmx_permission (id, code, name, ..., parent_id=NULL, domain_code, app_code, module_code, ...)
│   │   │       （唯一约束冲突 → 事务回滚，返回错误："权限 code {code} 已被其他模块占用"）
│   │   └── 更新 (to_update):
│   │       └── UPDATE cmx_permission SET name=$, resource_type=$, ...
│   │           WHERE id = $1    （按 id 定位，非 code）
│   │
│   ├── 3.4 第二阶段：回填 parent_id
│   │   ├── 构建 code_to_id: 合并 db_map + 新增的 code→id 映射
│   │   ├── 遍历所有 permissions，解析 parent_code → parent_id
│   │   │   （parent_code 不在 code_to_id 中 → 降级为 None + warn 日志）
│   │   └── UPDATE cmx_permission SET parent_id = $1 WHERE id = $2
│   │       （仅更新 parent_id 有值的记录）
│   │
│   └── 3.5 删除 (物理删除权限 + 物理删除角色关联，按 id 定位):
│       ├── 删除前查询受影响角色: SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id IN (...)
│       ├── DELETE FROM cmx_permission WHERE id = $1
│       └── DELETE FROM cmx_role_permission WHERE permission_id = $1
│
├── 4. 提交事务 (guard.commit())
│
├── 5. 审计日志（事务提交后）
│   └── 记录: 操作者、domain/app/module、created/updated/deleted 的 code 列表
│
├── 6. 缓存失效
│   └── 触发 IamChecker 缓存失效（如有变更影响用户权限）
│
└── 7. 返回 ImportResult { created: N, updated: N, deleted: N }
```

**两阶段 parent_code 解析**：第一阶段所有 INSERT/UPDATE 时 `parent_id = NULL`；第二阶段构建完整的 `code_to_id` 映射后统一回填 `parent_id`。这避免了文件内排序依赖和跨文件引用问题。

### 4.5 清理逻辑

```
cleanup_permissions(domain_code, app_code, module_code)
│
├── 1. 开启事务
│   ├── 1.1 查询 DB 获取所有匹配权限 ID
│   │   └── SELECT id FROM cmx_permission 
│   │       WHERE domain_code = $1 AND app_code = $2 AND module_code = $3
│   │       (不限定 archived，物理删除所有匹配记录)
│   │
│   ├── 1.2 查询受影响角色（用于缓存失效）
│   │   └── SELECT DISTINCT role_id FROM cmx_role_permission 
│   │       WHERE permission_id IN (SELECT id FROM cmx_permission 
│   │       WHERE domain_code = $1 AND app_code = $2 AND module_code = $3)
│   │
│   ├── 1.3 物理删除角色关联（用子查询避免 IN 列表过长）
│   │   └── DELETE FROM cmx_role_permission WHERE permission_id IN (
│   │       SELECT id FROM cmx_permission 
│   │       WHERE domain_code = $1 AND app_code = $2 AND module_code = $3
│   │   )
│   │
│   └── 1.4 物理删除权限
│       └── DELETE FROM cmx_permission 
│           WHERE domain_code = $1 AND app_code = $2 AND module_code = $3
│
├── 2. 提交事务
│
├── 3. 精准缓存失效
│   └── 对受影响角色逐一调用 iam_checker.invalidate_role_cache(role_id)
│
└── 4. 返回 ImportResult { deleted: N }
```

### 4.6 错误处理策略

| 错误场景 | 处理策略 | 返回 |
|---------|---------|------|
| ZIP 解压失败（损坏的 ZIP） | fail-fast，立即返回错误 | `ImportResult { success: false, message: "ZIP 解压失败: ..." }` |
| JSON 解析失败（格式错误） | fail-fast，返回具体文件名和行号 | `success: false, message: "文件 xxx.json 解析失败: ..."` |
| 同一文件内 code 重复 | fail-fast，返回重复 code | `success: false, message: "重复的权限 code: user:list"` |
| code 不符合 `{module}:action` 命名规范 | fail-fast，返回不合规 code | `success: false, message: "权限 code 格式不合规: xxx"` |
| 唯一约束冲突（code 被其他模块占用） | 事务回滚，返回冲突 code | `success: false, message: "权限 code xxx 已被其他模块占用"` |
| 事务提交失败 | 自动回滚（TransactionGuard Drop） | `success: false, message: "事务提交失败: ..."` |
| parent_code 不存在 | 降级为 `parent_id = None` + warn 日志 | 不阻断导入 |

**策略**：**整体 fail-fast**。任何权限条目的导入失败都会回滚整个事务，保证数据一致性。不支持部分成功。

### 4.7 幂等性保证

- **重复导入同一文件**：天然幂等。比对逻辑使得重复导入只会产生 UPDATE（无实际变化），不会产生重复 INSERT。
- **并发导入同一三元组**：通过事务内查询（`SELECT ... WITH txn_id`）+ 事务隔离保证一致性。若两个请求并发导入相同三元组，后提交者可能因唯一约束冲突而失败（返回明确错误）。
- **安装重试**：`PluginOperationExecutor` 的安装流程在 center 分发失败时会先补偿卸载，再由调用方决定是否重试。

### 4.8 审计日志

导入/清理操作在事务提交成功后记录审计日志（复用 `PermissionServiceImpl` 已有的 `self.audit_write` 模式）：

```
audit_write(svr_ctx, "import_permissions", "permission", "batch", &json!({
    "domain_code": domain_code,
    "app_code": app_code,
    "module_code": module_code,
    "created_codes": [...],
    "updated_codes": [...],
    "deleted_codes": [...],
}))
```

### 4.9 缓存失效（精准失效）

导入/清理权限后，如有删除操作（影响角色-权限关联），应触发 `IamChecker` **精准缓存失效**。

**策略**：在删除权限前，先查询受影响的 `role_id` 列表，删除后逐一调用 `invalidate_role_cache(role_id)`。避免全局缓存失效的性能影响。

```rust
// 在删除权限前（事务内），收集受影响角色
let affected_role_ids: Vec<String> = self.query_affected_roles_txn(txn_id, &to_delete_ids).await?;
// SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id IN (...)

// ... 执行删除 ...

// 事务提交后，精准失效缓存
if let Some(ref checker) = self.iam_checker {
    for role_id in &affected_role_ids {
        checker.invalidate_role_cache(role_id).await;
    }
}
```

`PluginDataImporterImpl` 注入 `Option<Arc<cmx_iam::IamChecker>>`，在导入/清理完成后触发上述失效逻辑。

### 4.10 可观测性

- **结构化日志**：所有关键步骤使用 `tracing::info!/warn!/error!`，注入 `target = "cmx_iam_import"`、`domain_code`、`app_code`、`module_code`、`counts` 等字段
- **tracing span**：导入方法标注 `#[instrument(target = "cmx_iam_import", skip(self, zip_data), fields(domain, app, module))]`
- **HTTP 调用链**：`HttpServiceCenterSender` 的 reqwest 请求注入 `X-Request-ID` header

### 4.11 配置校验

启动时（`manager.rs` 加载 `CenterClientConfig` 后）进行校验：

| mode 值 | 校验项 | 失败行为 |
|---------|--------|---------|
| `"http_url"` | `urls.perm` 必须配置 | 启动时 panic + 明确错误信息 |
| `"http_discovery"` | `discovery.perm_service` 必须配置 + `GlobalServiceInstanceCache` 已初始化 | 启动时 panic |
| `"grpc"` | `GlobalRpcClient::is_initialized()` 必须为 true | warn 日志，运行时报错 |
| `"mock"` | 无校验 | — |

### 4.12 gRPC 消息大小限制

gRPC 默认消息上限 4MB。权限数据通常较小（JSON 文件几 KB~几十 KB，ZIP 后更小），在 proto 注释中标注此限制。若 ZIP 超过 4MB，gRPC 模式会返回错误，建议改用 HTTP 模式。

---

## 五、变更清单

### 5.1 新增文件

| 文件 | 用途 |
|------|------|
| `crates/libs/cmx-traits/src/plugin/data_importer.rs` | `PluginDataImporter` trait + 请求/响应类型 |
| `crates/libs/cmx-plugin/src/center_client/http_sender.rs` | `HttpServiceCenterSender` 实现 |
| `crates/libs/cmx-plugin/src/center_client/grpc_sender.rs` | `GrpcServiceCenterSender` 实现 |
| `crates/libs/cmx-iam/src/permission/import_handler.rs` | `PluginDataImporterImpl` — 实现 `PluginDataImporter` trait，按 category 路由 |
| `crates/libs/cmx-api/src/handlers/iam/permission/import_handler.rs` | HTTP 端点 handler (multipart 接收 ZIP) |
| `crates/libs/cmx-dev/templates/wasm-plugin-template/permdata/sample-perm.json` | 示例权限数据文件 |

### 5.2 修改文件

| 文件 | 修改内容 |
|------|----------|
| `crates/libs/cmx-rpc-gen/idl/cmx_service.proto` | 新增 `ImportPluginData`/`CleanupPluginData` 方法和消息 |
| `crates/libs/cmx-traits/src/plugin/mod.rs` | 导出 `data_importer` 模块 |
| `crates/libs/cmx-traits/src/rpc/client.rs` | `RpcClient` trait 新增 `import_plugin_data`/`cleanup_plugin_data` 方法，提供**默认实现**返回 `Err(RpcError::UnsupportedProtocol(...))` 减少破坏性 |
| `crates/libs/cmx-infra/cmx-rpc/src/client.rs` | `VoloGrpcClient` 实现新 trait 方法 |
| `crates/libs/cmx-infra/cmx-rpc/src/server.rs` | `CmxOrchestratorServiceImpl` 新增 `data_importer` 字段 + 实现新 gRPC 方法 |
| `crates/libs/cmx-infra/cmx-rpc/src/server_runner.rs` | `start_grpc_server` 新增 `data_importer` 参数 |
| `crates/libs/cmx-infra/cmx-rpc/src/lib.rs` | re-export `PluginDataImporter` trait |
| `crates/libs/cmx-plugin/src/center_client/types.rs` | `CenterCleanupRequest` 新增 domain_code/application_code/module_code 字段 |
| `crates/libs/cmx-plugin/src/center_client/config.rs` | `CenterClientConfig` mode 新增 `"http_url"`/`"http_discovery"`/`"grpc"` 值 |
| `crates/libs/cmx-plugin/src/center_client/mod.rs` | 导出 `HttpServiceCenterSender`/`GrpcServiceCenterSender` |
| `crates/libs/cmx-plugin/src/center_client/dispatcher.rs` | `dispatch_cleanup` 传递 domain_code/application_code/module_code |
| `crates/libs/cmx-plugin/src/core/manager.rs` | 按 config.mode 选择 sender 实现 |
| `crates/libs/cmx-iam/src/permission/service.rs` | `PermissionServiceImpl` 新增 `import_permissions`/`cleanup_permissions` 方法（**不修改 `PermissionService` trait**，方法为 `PermissionServiceImpl` 固有方法，由 `PluginDataImporterImpl` 直接调用） |
| `crates/libs/cmx-iam/src/permission/mod.rs` | 导出 `import_handler` 模块 |
| `crates/libs/cmx-iam/src/lib.rs` | 导出 `PluginDataImporterImpl` |
| `crates/libs/cmx-api/src/handlers/iam/permission/mod.rs` | 注册 import/cleanup 路由 |
| `crates/libs/cmx-api/src/app_state.rs` | `CmxAppState` 新增 `plugin_data_importer: Option<Arc<dyn PluginDataImporter>>` 字段 + builder/accessor 方法 |
| `crates/web/web-server/src/config/rpc.rs` | `init_rpc` 传入 `data_importer` |
| `crates/web/web-server/src/config/iam.rs` | 创建 `PluginDataImporterImpl` 并注入 |
| `dev.toml` | 更新 `[center_client]` 配置注释 |
| `config/config_template.toml` | 同步更新配置模板 |

### 5.3 详细变更说明

#### 5.3.1 Proto 扩展 (`cmx_service.proto`)

新增 `ImportPluginData` 和 `CleanupPluginData` 两个 RPC 方法及对应消息。使用 `bytes` 字段传输 ZIP 二进制数据。

#### 5.3.2 cmx-traits 新增 PluginDataImporter trait

定义跨 crate 的数据导入抽象。`PluginDataImportRequest`/`PluginDataCleanupRequest`/`PluginDataImportResult` 为普通 Rust 结构体（带 Serialize/Deserialize），不依赖 proto 类型。

#### 5.3.3 RpcClient trait 扩展

新增 `import_plugin_data` 和 `cleanup_plugin_data` 方法。`VoloGrpcClient` 实现时将 `PluginDataImportRequest` 转换为 proto 的 `ImportPluginDataRequest`，调用生成的 gRPC 客户端方法，再将响应转回 `PluginDataImportResult`。

#### 5.3.4 CmxOrchestratorServiceImpl 扩展

新增 `data_importer: Option<Arc<dyn PluginDataImporter>>` 字段。`import_plugin_data` gRPC 方法检查 `data_importer` 是否存在，存在则委托调用，不存在则返回 `success=false`。`start_grpc_server` 函数签名新增 `data_importer: Option<Arc<dyn PluginDataImporter>>` 参数。

#### 5.3.5 HttpServiceCenterSender

```rust
pub struct HttpServiceCenterSender {
    http_client: reqwest::Client,
    config: CenterClientConfig,
}

impl HttpServiceCenterSender {
    /// 解析目标 URL
    async fn resolve_url(&self, category: DataCategory) -> Result<String, CenterError> {
        match self.config.mode.as_str() {
            "http_url" => {
                // 从 config.urls 获取直接 URL
                self.config.resolve_urls().get(&category).cloned()
                    .ok_or_else(|| CenterError::Config(format!("{} URL 未配置", category.center_name())))
            }
            "http_discovery" => {
                // 通过服务发现获取实例地址
                let service_name = self.config.discovery.get_service_name(category)
                    .ok_or_else(|| CenterError::Config(format!("{} 服务名未配置", category.center_name())))?;
                let cache = cmx_registry_config::GlobalServiceInstanceCache::get();
                let instances = cache.get(&service_name)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| CenterError::Unavailable {
                        center: category.center_name().to_string(),
                        url: service_name.clone(),
                    })?;
                // 过滤健康实例后随机选择（避免请求集中到单个实例）
                let healthy: Vec<_> = instances.iter().filter(|i| i.healthy).collect();
                let pool = if healthy.is_empty() { &instances } else { &healthy };
                let instance = &pool[rand::thread_rng().gen_range(0..pool.len())];
                // ServiceInstance 无内置 URL 方法，需手动拼接
                // 优先 metadata["http_port"]，回退 instance.port
                let port = instance.metadata.get("http_port")
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or(instance.port);
                Ok(format!("http://{}:{}/api/iam/permissions/import", instance.ip, port))
            }
            _ => Err(CenterError::Config(format!("不支持的 HTTP 模式: {}", self.config.mode)))
        }
    }
}
```

`send_data` 构建 multipart form-data 请求发送 ZIP。`cleanup_data` 发送 DELETE/POST 请求到 cleanup 端点。

#### 5.3.6 GrpcServiceCenterSender

```rust
pub struct GrpcServiceCenterSender;

#[async_trait]
impl ServiceCenterSender for GrpcServiceCenterSender {
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError> {
        if !cmx_rpc::GlobalRpcClient::is_initialized() {
            return Err(CenterError::Unavailable {
                center: request.category.center_name().to_string(),
                url: "grpc://GlobalRpcClient".to_string(),
            });
        }
        let rpc_client = cmx_rpc::GlobalRpcClient::get();
        let import_request = PluginDataImportRequest {
            category: match request.category {
                DataCategory::Perm => PluginDataCategory::Perm,
                DataCategory::Menu => PluginDataCategory::Menu,
                DataCategory::Form => PluginDataCategory::Form,
                DataCategory::Flow => PluginDataCategory::Flow,
            },
            domain_code: request.domain_code,
            application_code: request.application_code,
            module_code: request.module_code,
            plugin_id: request.plugin_id,
            app_id: request.app_id,
            version: request.version,
            zip_data: request.zip_data,
        };
        let result = rpc_client.import_plugin_data(import_request).await
            .map_err(|e| CenterError::CallFailed {
                center: request.category.center_name().to_string(),
                message: e.to_string(),
            })?;
        Ok(CenterResponse {
            success: result.success,
            message: result.message,
            center_id: None,
        })
    }
    // cleanup_data 类似
}
```

#### 5.3.7 PermissionServiceImpl 新增方法

在 `PermissionServiceImpl` 中新增 `import_permissions` 和 `cleanup_permissions` **固有方法**（非 trait 方法），使用 `DatabaseManager` 事务模式：

```rust
#[instrument(target = "cmx_iam_import", skip(self, zip_data), fields(domain = %domain_code, app = %app_code, module = %module_code))]
async fn import_permissions(
    &self,
    svr_ctx: &SVRContext,
    domain_code: &str,
    app_code: &str,
    module_code: &str,
    zip_data: &[u8],
) -> Result<PermissionImportResult, TraitError> {
    // 1. 解压 + 解析 + 校验 JSON
    let definitions = parse_and_validate_permission_zip(zip_data)?;
    let file_codes: HashSet<String> = definitions.iter().map(|d| d.code.clone()).collect();
    
    // 2. 开启事务（查询和写入在同一事务内）
    let txn_ctx = self.mm.get_transaction_context();
    let guard = txn_ctx.begin_with_guard(&self.db_id).await?;
    let txn_id = guard.txn_id();
    
    // 2.1 事务内查询 DB 已有权限（按三元组）
    let db_map = self.query_permission_ids_by_scope_txn(
        txn_id, domain_code, app_code, module_code
    ).await?;  // HashMap<code, id>
    let db_codes: HashSet<String> = db_map.keys().cloned().collect();
    
    // 2.2 比对
    let to_create: Vec<_> = definitions.iter().filter(|d| !db_codes.contains(&d.code)).collect();
    let to_update: Vec<_> = definitions.iter().filter(|d| db_codes.contains(&d.code)).collect();
    let to_delete: Vec<String> = db_codes.difference(&file_codes).cloned().collect();
    
    // 3. 第一阶段：INSERT/UPDATE（parent_id 暂置 NULL）
    let mut code_to_id: HashMap<String, String> = db_map.clone();
    let mut created_count = 0u32;
    for def in &to_create {
        let id = cmx_utils::id::snowflake_id_str();
        // INSERT INTO cmx_permission (id, code, name, resource_type, parent_id=NULL, ...)
        //     VALUES ($1, $2, $3, ...) 
        // 唯一约束冲突 → guard Drop 自动回滚，返回错误
        code_to_id.insert(def.code.clone(), id);
        created_count += 1;
    }
    let mut updated_count = 0u32;
    for def in &to_update {
        let id = db_map.get(&def.code).unwrap();  // 按 id 定位
        // UPDATE cmx_permission SET name=$1, resource_type=$2, parent_id=NULL, ...
        //     WHERE id = $3
        updated_count += 1;
    }
    
    // 4. 第二阶段：回填 parent_id
    for def in &definitions {
        if let Some(parent_code) = &def.parent_code {
            if let (Some(id), Some(parent_id)) = (code_to_id.get(&def.code), code_to_id.get(parent_code)) {
                // UPDATE cmx_permission SET parent_id = $1 WHERE id = $2
            } else {
                tracing::warn!(code = %def.code, parent_code = %parent_code, "parent_code 未找到，降级为无父节点");
            }
        }
    }
    
    // 5. 删除前查询受影响角色（用于缓存失效）
    let to_delete_ids: Vec<String> = to_delete.iter().filter_map(|c| db_map.get(c).cloned()).collect();
    let affected_roles = self.query_affected_roles_txn(txn_id, &to_delete_ids).await?;
    // SELECT DISTINCT role_id FROM cmx_role_permission WHERE permission_id IN (...)

    // 5.1 物理删除权限 + 物理删除角色关联（按 id 定位）
    let mut deleted_count = 0u32;
    for code in &to_delete {
        let id = db_map.get(code).unwrap();
        // DELETE FROM cmx_permission WHERE id = $1
        // DELETE FROM cmx_role_permission WHERE permission_id = $1
        deleted_count += 1;
    }
    
    // 6. 提交事务
    guard.commit().await?;
    
    // 7. 审计日志（事务提交后）
    self.audit_write(svr_ctx, "import_permissions", "permission", "batch", &serde_json::json!({
        "domain_code": domain_code, "app_code": app_code, "module_code": module_code,
        "created": created_count, "updated": updated_count, "deleted": deleted_count,
    })).await;

    // 8. 精准缓存失效（仅受影响角色）
    if deleted_count > 0 {
        if let Some(ref checker) = self.iam_checker {
            for role_id in &affected_roles {
                checker.invalidate_role_cache(role_id).await;
            }
        }
    }
    
    Ok(PermissionImportResult {
        success: true,
        message: format!("导入完成: 新增 {} / 更新 {} / 删除 {}", created_count, updated_count, deleted_count),
        created_count, updated_count, deleted_count,
    })
}
```

#### 5.3.8 HTTP Import Handler

**关键**：HTTP handler 通过 `PluginDataImporter` trait 调用（与 gRPC 路径统一），不直接调用 `PermissionService` 固有方法。

```rust
// crates/libs/cmx-api/src/handlers/iam/permission/import_handler.rs

pub async fn import_permissions(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ImportResultDto>>> {
    let mut file_data: Option<Bytes> = None;
    let mut domain_code = String::new();
    let mut application_code = String::new();
    let mut module_code = String::new();
    let mut plugin_id = String::new();
    // ... 其他字段

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        match field.name() {
            Some("file") => { file_data = Some(field.bytes().await?); }
            Some("domain_code") => { domain_code = field.text().await?; }
            // ... 其他字段
            _ => {}
        }
    }

    // 通过 PluginDataImporter trait 调用（与 gRPC 路径统一）
    let importer = cmx_state.plugin_data_importer()
        .ok_or_else(|| Error::business_error("PluginDataImporter 未初始化".into()))?;

    let request = PluginDataImportRequest {
        category: PluginDataCategory::Perm,
        domain_code,
        application_code,
        module_code,
        plugin_id,
        app_id: svr_ctx.auth_context.as_ref().map(|a| a.user_id.clone()).unwrap_or_default(),
        version: String::new(),
        zip_data: file_data.ok_or_else(|| Error::business_error("缺少 file 字段".into()))?.to_vec(),
    };

    let result = importer.import_data(request).await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(ImportResultDto::from(result))))
}
```

**`CmxAppState` 新增字段**：
```rust
// crates/libs/cmx-api/src/app_state.rs
impl CmxAppState {
    pub fn with_plugin_data_importer(mut self, importer: Arc<dyn PluginDataImporter>) -> Self {
        self.plugin_data_importer = Some(importer);
        self
    }
    pub fn plugin_data_importer(&self) -> Option<&Arc<dyn PluginDataImporter>> {
        self.plugin_data_importer.as_ref()
    }
}
```

#### 5.3.9 manager.rs sender 选择逻辑

```rust
let center_config = crate::center_client::CenterClientConfig::load();
let center_sender: Arc<dyn ServiceCenterSender> = match center_config.mode.as_str() {
    "http_url" | "http_discovery" => {
        Arc::new(crate::center_client::HttpServiceCenterSender::new(center_config.clone()))
    }
    "grpc" => {
        Arc::new(crate::center_client::GrpcServiceCenterSender)
    }
    _ => Arc::new(crate::center_client::MockServiceCenterSender),
};
```

#### 5.3.10 CenterCleanupRequest 扩展

```rust
pub struct CenterCleanupRequest {
    pub plugin_id: String,
    pub app_id: String,
    pub version: Option<String>,
    pub category: DataCategory,
    pub domain_code: String,          // 新增
    pub application_code: String,     // 新增
    pub module_code: String,          // 新增
}
```

`dispatcher.rs` 的 `dispatch_cleanup` 方法同步修改，从 `DispatchContext` 传递这些字段。

#### 5.3.11 配置更新

```toml
[center_client]
# 模式：mock | http_url | http_discovery | grpc
mode = "mock"
timeout_ms = 30000

[center_client.urls]
# http_url 模式下各中心直连地址
# perm = "http://localhost:8080/api/iam/permissions/import"

[center_client.discovery]
# http_discovery 模式下各中心服务名
# nacos_group = "DEFAULT_GROUP"
# perm_service = "cmx-server"
```

---

## 六、假设与决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 导入逻辑位置 | `PermissionServiceImpl` 固有方法 | 复用已有 `DatabaseManager` 和事务模式，**不修改 `PermissionService` trait** 避免破坏接口一致性 |
| HTTP/gRPC 桥接 trait | `PluginDataImporter` in cmx-traits | cmx-rpc 不能依赖 cmx-iam，用 trait 解耦 |
| gRPC 客户端调用方式 | 通过 `GlobalRpcClient::get()` 调用 | 复用已有全局单例和服务发现基础设施 |
| 文件格式 parent 引用 | 使用 `parent_code` 而非 `parent_id` | 插件开发者无需知道数据库 ID |
| parent_code 解析 | 两阶段：先 INSERT 全部（parent_id=NULL），再回填 | 避免顺序依赖和跨文件引用问题 |
| 操作判断方式 | 接收端自动比对 DB 与文件 | 无需在文件中声明 action，更简洁 |
| SQL 定位方式 | 所有 UPDATE/DELETE 按 `id` 定位 | 禁止仅用 code，避免跨插件误操作 |
| HTTP 发现端口 | 优先 `metadata["http_port"]`，回退 `instance.port` | 与 gRPC 发现端口选择逻辑一致 |
| `data_importer` 注入方式 | `Option<Arc<dyn PluginDataImporter>>` | 向后兼容，未配置时 gRPC 方法返回错误 |
| mode 值统一 | `"mock"` / `"http_url"` / `"http_discovery"` / `"grpc"` | **Breaking change**，旧值 `"url"`/`"discovery"` 不再支持 |
| category 类型 | `PluginDataCategory` 枚举（非字符串） | 类型安全，避免字符串匹配出错 |
| HTTP discovery 负载均衡 | 过滤 `healthy=true` 后随机选择 | 避免请求集中到单个实例 |
| 错误处理策略 | 整体 fail-fast | 任何错误回滚整个事务，保证数据一致性 |
| 删除策略 | **物理删除**（DELETE FROM）权限和角色-权限关联 | 用户要求，不使用 archived 软删除 |
| HTTP/gRPC 路径统一 | 两路径都走 `PluginDataImporter` trait | 避免 HTTP 绕过 category 路由和缓存失效 |
| 缓存失效 | 精准失效（查受影响角色，逐一 invalidate） | 避免全局失效的性能影响 |
| RpcClient trait 扩展 | 新增方法提供默认实现返回 Err | 减少对其他实现的破坏性 |
| 大批量 DELETE | 用子查询替代 IN 列表 | 避免 IN 参数过多 |
| ID 生成 | `cmx_utils::id::snowflake_id_str()` | 项目统一雪花算法 |

---

## 七、实施步骤

### 步骤 1：定义跨 crate Trait 和类型

1. 新建 `crates/libs/cmx-traits/src/plugin/data_importer.rs`：定义 `PluginDataImporter` trait + `PluginDataImportRequest`/`PluginDataCleanupRequest`/`PluginDataImportResult`
2. `crates/libs/cmx-traits/src/plugin/mod.rs` 导出新模块
3. `crates/libs/cmx-traits/src/rpc/client.rs`：`RpcClient` trait 新增 `import_plugin_data`/`cleanup_plugin_data` 方法

### 步骤 2：扩展 Proto 和 gRPC 代码生成

1. 修改 `crates/libs/cmx-rpc-gen/idl/cmx_service.proto`：新增 `ImportPluginData`/`CleanupPluginData` 方法和消息
2. 运行 `rtk cargo build -p cmx-rpc-gen` 验证 proto 编译

### 步骤 3：实现 gRPC 服务端

1. `crates/libs/cmx-infra/cmx-rpc/src/server.rs`：`CmxOrchestratorServiceImpl` 新增 `data_importer` 字段 + 实现 `import_plugin_data`/`cleanup_plugin_data` 方法
2. `crates/libs/cmx-infra/cmx-rpc/src/server_runner.rs`：`start_grpc_server` 签名新增 `data_importer` 参数
3. `crates/libs/cmx-infra/cmx-rpc/src/lib.rs`：re-export `PluginDataImporter` trait

### 步骤 4：实现 gRPC 客户端

1. `crates/libs/cmx-infra/cmx-rpc/src/client.rs`：`VoloGrpcClient` 实现 `import_plugin_data`/`cleanup_plugin_data`，将 Rust 类型与 proto 类型互转

### 步骤 5：实现权限导入核心逻辑

1. `crates/libs/cmx-iam/src/permission/service.rs`：`PermissionServiceImpl` 新增 `import_permissions`/`cleanup_permissions` **固有方法**（不修改 `PermissionService` trait），包含：事务内查询、两阶段 parent_code 解析、按 id 操作、**物理删除**、审计日志、精准缓存失效
2. 新建 `crates/libs/cmx-iam/src/permission/import_handler.rs`：`PluginDataImporterImpl` 实现 `PluginDataImporter` trait
   - **构造方式**：`PluginDataImporterImpl::new(Arc<PermissionServiceImpl>, Option<Arc<IamChecker>>)`
   - 持有 `PermissionServiceImpl` 的**具体类型**引用（非 trait 对象），以便调用固有方法
   - 按 `PluginDataCategory` 枚举路由：`Perm` → 调用 `PermissionServiceImpl.import_permissions()`
   - 在导入/清理完成后触发精准缓存失效
3. `crates/libs/cmx-iam/src/permission/mod.rs` 和 `lib.rs` 导出新模块和类型

### 步骤 6：实现 HTTP 端点

1. 新建 `crates/libs/cmx-api/src/handlers/iam/permission/import_handler.rs`：multipart 接收 ZIP + 元数据，调用 `PermissionService`
2. `crates/libs/cmx-api/src/handlers/iam/permission/mod.rs`：注册 `/iam/permissions/import` 和 `/iam/permissions/cleanup` 路由

### 步骤 7：实现 Sender

1. 新建 `crates/libs/cmx-plugin/src/center_client/http_sender.rs`：`HttpServiceCenterSender` 实现
2. 新建 `crates/libs/cmx-plugin/src/center_client/grpc_sender.rs`：`GrpcServiceCenterSender` 实现
3. 修改 `crates/libs/cmx-plugin/src/center_client/types.rs`：`CenterCleanupRequest` 新增字段
4. 修改 `crates/libs/cmx-plugin/src/center_client/config.rs`：mode 新增值，`CenterDiscoveryConfig` 新增 `get_service_name` 方法
5. 修改 `crates/libs/cmx-plugin/src/center_client/dispatcher.rs`：`dispatch_cleanup` 传递新字段
6. 修改 `crates/libs/cmx-plugin/src/center_client/mod.rs`：导出新 sender
7. 修改 `crates/libs/cmx-plugin/src/core/manager.rs`：按 mode 选择 sender

### 步骤 8：Web-server 集成

1. `crates/web/web-server/src/config/iam.rs`：在 `init_iam_services` 中创建 `PermissionServiceImpl` 时保留**具体类型** `Arc<PermissionServiceImpl>`（在 wrap 成 `Arc<dyn PermissionService>` 之前 clone），用于构造 `PluginDataImporterImpl::new(perm_impl_arc, iam_checker_arc)`，产出 `Arc<dyn PluginDataImporter>`
2. `crates/web/web-server/src/main.rs`：将 `Arc<dyn PluginDataImporter>` 同时注入到 `CmxAppState`（通过 `.with_plugin_data_importer(importer)`）和传入 `init_rpc`（供 gRPC server 使用）
3. `crates/web/web-server/src/config/rpc.rs`：`init_rpc` 新增 `data_importer: Option<Arc<dyn PluginDataImporter>>` 参数，透传给 `start_grpc_server`

### 步骤 9：配置和示例数据

1. 更新 `dev.toml` 和 `config/config_template.toml` 的 `[center_client]` 配置注释
2. 新建 `crates/libs/cmx-dev/templates/wasm-plugin-template/permdata/sample-perm.json` 示例文件

### 步骤 10：编译验证

```bash
rtk cargo check
rtk cargo clippy
```

---

## 八、验证方案

### 8.1 单元测试

- `PermissionServiceImpl::import_permissions`：测试新增/更新/删除三种场景
- `parse_permission_zip`：测试多文件 ZIP 解析、JSON 格式校验
- 比对逻辑：file_codes 与 db_codes 的差集/交集计算

### 8.2 集成测试

1. **Mock 模式**：验证现有流程不受影响
2. **HTTP 模式**：
   - 启动应用，配置 `mode = "http_url"`，`perm = "http://localhost:8080/api/iam/permissions/import"`
   - 安装含 `permdata/` 的插件
   - 验证 `cmx_permission` 表数据正确写入
   - 卸载插件，验证权限被物理删除 + 角色关联被清理
3. **gRPC 模式**：
   - 启用 RPC (`[rpc] enabled = true`)
   - 配置 `mode = "grpc"`
   - 安装/卸载插件，验证同上

### 8.3 事务验证

- 导入过程中模拟错误（如重复 code），验证事务回滚
- 验证删除权限时 `cmx_role_permission` 关联记录同步清理

---

## 九、后续扩展

本方案以权限中心（Perm）为首个实现，架构设计支持后续扩展：

| 中心 | 扩展方式 |
|------|----------|
| 门户中心 (Menu) | `PluginDataImporterImpl` 新增 `"menu"` 分支，委托给 MenuService |
| 表单中心 (Form) | `PluginDataImporterImpl` 新增 `"form"` 分支 |
| 流程中心 (Flow) | `PluginDataImporterImpl` 新增 `"flow"` 分支 |

每个新中心只需：
1. 定义对应的文件格式 JSON 规范
2. 在对应 Service 中实现 import/cleanup 方法
3. 在 `PluginDataImporterImpl` 中新增 category 路由分支