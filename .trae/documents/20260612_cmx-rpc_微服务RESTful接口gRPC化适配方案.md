# 微服务 RESTful 接口 gRPC 化适配方案

> 日期：2026-06-12 | 模块：cmx-rpc, cmx-api, cmx-traits | 状态：方案设计中

---

## 一、现状问题分析

### 1.1 当前模块依赖关系

```
web-server (组装层)
  ├── cmx-api (axum handler + 路由 + 中间件)
  │     ├── cmx-database (GenericCrudService)    ← 标准 CRUD 直接调用
  │     ├── cmx-service (Orchestrator)           ← 服务编排调用
  │     ├── cmx-plugin (GlobalPluginManager)     ← 插件管理全局单例
  │     ├── cmx-rpc                               ← 使用 RpcClient
  │     └── handlers/domain/service.rs           ← 自定义 Service 定义在 cmx-api 内部
  │
  ├── cmx-rpc (gRPC 框架，位于 cmx-infra/ 下)
  │     ├── cmx-traits (RpcClient trait)
  │     ├── cmx-registry-config (服务发现)
  │     └── cmx-rpc-gen (proto 代码生成)
  │
  └── cmx-service / cmx-plugin / cmx-runtime ...
```

### 1.2 核心矛盾

| 问题 | 说明 |
|------|------|
| **业务逻辑耦合在 cmx-api** | `DomainService`、`SysDatasourceService` 等自定义 Service 定义在 `cmx-api/src/handlers/xxx/service.rs`，gRPC 无法直接调用 |
| **cmx-rpc 不应依赖 cmx-api** | 当前依赖方向是 `cmx-api → cmx-rpc`，反向依赖会导致循环依赖 |
| **handler 和 gRPC 存在逻辑重复** | `service/handler.rs` 的 `service_call` 和 `cmx-rpc/server.rs` 的 `call_function` 做了几乎相同的事情 |
| **cmx-rpc 位于 infra 层级** | 如果要承载业务 gRPC 服务，是否应提升为独立 crate？ |

### 1.3 业务逻辑分层现状

| 层级 | 标准 CRUD | 自定义 Service | 跨模块调用 |
|------|----------|---------------|-----------|
| handler 层 (cmx-api) | 薄层，委托 GenericCrudService | 薄层，委托 DomainService 等 | 薄层，委托 trait 对象 |
| service 层 | GenericCrudService (cmx-database) | DomainService (cmx-api 内) | Orchestrator (cmx-service) |
| 数据层 | DatabaseManager (cmx-database) | DatabaseManager (cmx-database) | DatabaseManager (cmx-database) |

**关键**：标准 CRUD 的 `GenericCrudService` 在 `cmx-database` 中，gRPC 可直接调用。自定义 Service 在 `cmx-api` 中，是阻碍点。

---

## 二、架构决策

### 决策 1：cmx-rpc 目录位置 — 保持不动

**结论：cmx-rpc 保持在 `crates/libs/cmx-infra/cmx-rpc/` 不移动。**

理由：
1. **cmx-rpc 本质是基础设施**：它提供 gRPC 框架能力（客户端、服务端、服务发现、负载均衡），不包含业务逻辑
2. **业务 gRPC 实现不在 cmx-rpc 中**：新增的业务 gRPC Service 实现放在各自的业务 crate 或共享层中
3. **cmx-infra 目录定位正确**：infra 目录下的 crate 都是"与外部系统通信"的基础设施（database、buffer、nacos、registry-config、rpc、storage）
4. **移动会破坏大量 import 路径**：workspace 成员、依赖引用、文档都需要更新，收益不大

### 决策 2：避免 cmx-rpc 依赖 cmx-api — 将共享 Service 下沉

**结论：将 cmx-api 中的自定义 Service 迁移到 `cmx-service` crate，gRPC 和 axum handler 都通过 cmx-service 调用。**

理由：
1. `cmx-service` 已经是业务逻辑 crate，`DomainService` 等放在其中语义合理
2. `cmx-service` 只依赖 `cmx-database` + `cmx-core` + `cmx-traits`，不依赖 axum
3. gRPC server 实现可以直接调用 `cmx-service` 中的 Service，不需要经过 cmx-api

### 决策 3：业务 gRPC 实现的放置位置 — 在 cmx-rpc 中新增 service 模块

**结论：gRPC 服务端实现（将 protobuf 请求转换为业务 Service 调用）放在 `cmx-rpc/src/service/` 下。**

理由：
1. gRPC 服务端实现是"协议适配层"，职责薄，只做 protobuf ↔ 业务类型的转换
2. 与现有 `cmx-rpc/src/server.rs`（CmxOrchestratorServiceImpl）风格一致
3. 避免在业务 crate（cmx-service）中引入 gRPC 依赖（volo、protobuf 类型）
4. cmx-rpc 依赖 cmx-service（而非 cmx-api），依赖方向清晰

### 决策 4：依赖方向

**最终依赖方向**：

```
cmx-api ──depends──► cmx-rpc ──depends──► cmx-service
   │                      │                     │
   └──depends──► cmx-service              depends on cmx-database
                                              cmx-core
                                              cmx-traits
```

**不会产生循环依赖**：cmx-rpc 依赖 cmx-service，cmx-api 同时依赖 cmx-rpc 和 cmx-service，都是单向的。

---

## 三、目标架构

```
┌─────────────────────────────────────────────────────────┐
│                     web-server (组装层)                   │
│  初始化 cmx-service、cmx-plugin、cmx-rpc、cmx-api 等     │
└──────┬──────────────────────┬──────────────────────┬─────┘
       │                      │                      │
       v                      v                      v
┌─────────────┐      ┌───────────────┐      ┌───────────────┐
│   cmx-api   │      │    cmx-rpc    │      │  cmx-rpc-gen  │
│  (axum 层)  │      │  (gRPC 层)    │      │  (proto 生成)  │
│             │      │               │      │               │
│ handlers/   │      │ client.rs     │      │ idl/           │
│  薄层适配   │      │ server.rs     │      │  *.proto       │
│  ↓ 调用     │      │ service/      │      │                │
│  cmx-service│      │  DomainRpcSvc │      │ volo.yml       │
│  cmx-database│      │  PluginRpcSvc │      │ build.rs       │
│  cmx-plugin │      │  StorageRpcSvc│      └───────┬───────┘
└──────┬──────┘      │  ...          │              │
       │             │               │              │ 代码生成
       │             │ discover.rs   │              │
       │             │ load_balancer │              v
       │             └───────┬───────┘      ┌───────────────┐
       │                     │              │  生成的 Rust   │
       │                     │              │  gRPC 存根     │
       │                     v              └───────────────┘
       │             ┌───────────────┐
       └────────────►│  cmx-service  │◄─────┘
                     │               │
                     │ Orchestrator  │
                     │ DomainService │  ← 从 cmx-api 迁移
                     │ DatasourceSvc │  ← 从 cmx-api 迁移
                     │ CrudService   │  ← 封装 GenericCrudService
                     └───────┬───────┘
                             │
                    ┌────────┼────────┐
                    v        v        v
              ┌──────────┐ ┌────────┐ ┌──────────┐
              │cmx-traits│ │cmx-db  │ │cmx-plugin│
              └──────────┘ └────────┘ └──────────┘
```

---

## 四、实施步骤

### 阶段 1：Proto 定义与代码生成

#### 1.1 新增业务 Proto 文件

在 `cmx-rpc-gen/idl/` 下新增 proto 定义。按业务模块拆分 proto 文件：

**`cmx-rpc-gen/idl/cmx_crud.proto`** — 通用 CRUD gRPC 服务：

```protobuf
syntax = "proto3";
package cmx;

// 通用 CRUD 服务（domain, application, module 等标准实体复用）
service CmxCrudService {
  rpc Create(CrudRequest) returns (CrudResponse);
  rpc CreateMany(CrudRequest) returns (CrudResponse);
  rpc GetById(GetByIdRequest) returns (CrudResponse);
  rpc Update(CrudRequest) returns (CrudResponse);
  rpc UpdateMany(CrudRequest) returns (CrudResponse);
  rpc Delete(DeleteRequest) returns (CrudResponse);
  rpc List(ListRequest) returns (CrudResponse);
  rpc Page(PageRequest) returns (PageResponse);
}

message CrudRequest {
  string entity_type = 1;   // "domain" / "application" / "module" 等
  string db_id = 2;         // 数据源 ID
  string data = 3;          // JSON 格式的请求体
}

message GetByIdRequest {
  string entity_type = 1;
  string db_id = 2;
  string id = 3;
}

message DeleteRequest {
  string entity_type = 1;
  string db_id = 2;
  string data = 3;          // JSON: { ids: [...] }
}

message ListRequest {
  string entity_type = 1;
  string db_id = 2;
  string filter = 3;        // JSON 格式的 modql 过滤条件
}

message PageRequest {
  string entity_type = 1;
  string db_id = 2;
  int32 page = 3;
  int32 page_size = 4;
  string filter = 5;
}

message CrudResponse {
  bool success = 1;
  string data = 2;          // JSON 格式响应
  string error = 3;
}

message PageResponse {
  bool success = 1;
  string data = 2;          // JSON 格式 { items: [...], total: N }
  string error = 3;
}
```

**`cmx-rpc-gen/idl/cmx_domain.proto`** — Domain 特有接口：

```protobuf
service CmxDomainService {
  rpc GetTree(GetTreeRequest) returns (CrudResponse);
}

message GetTreeRequest {
  string db_id = 1;
  string parent_id = 2;     // 可选，筛选父节点
}
```

**`cmx-rpc-gen/idl/cmx_datasource.proto`** — 数据源特有接口：

```protobuf
service CmxDatasourceService {
  rpc TestConnection(TestConnectionRequest) returns (CrudResponse);
  rpc GetSchemas(GetSchemasRequest) returns (CrudResponse);
  rpc GetTables(GetTablesRequest) returns (CrudResponse);
  rpc GetColumns(GetColumnsRequest) returns (CrudResponse);
}
```

**`cmx-rpc-gen/idl/cmx_plugin.proto`** — 插件管理接口：

```protobuf
service CmxPluginService {
  rpc Deploy(DeployRequest) returns (CrudResponse);
  rpc Install(InstallRequest) returns (CrudResponse);
  rpc Uninstall(UninstallRequest) returns (CrudResponse);
  rpc Upgrade(UpgradeRequest) returns (CrudResponse);
  rpc ListPlugins(ListPluginsRequest) returns (CrudResponse);
  rpc GetPlugin(GetPluginRequest) returns (CrudResponse);
}

message DeployRequest { string db_id = 1; string data = 2; }
message InstallRequest { string db_id = 1; string data = 2; }
// ... 其他 message 定义
```

**`cmx-rpc-gen/idl/cmx_storage.proto`** — 文件存储接口：

```protobuf
service CmxStorageService {
  rpc Upload(StorageUploadRequest) returns (CrudResponse);
  rpc Download(StorageDownloadRequest) returns (CrudResponse);
  rpc Delete(StorageDeleteRequest) returns (CrudResponse);
  rpc GetInfo(StorageInfoRequest) returns (CrudResponse);
}
```

#### 1.2 更新 volo-build 配置

更新 `cmx-rpc-gen/volo.yml`，将新增的 proto 文件纳入代码生成：

```yaml
services:
  - idl/cmx_service.proto
  - idl/cmx_crud.proto
  - idl/cmx_domain.proto
  - idl/cmx_datasource.proto
  - idl/cmx_plugin.proto
  - idl/cmx_storage.proto
```

#### 1.3 更新 build.rs 和 lib.rs

确保新 proto 生成的代码被正确导出。

---

### 阶段 2：共享业务逻辑下沉

#### 2.1 将自定义 Service 从 cmx-api 迁移到 cmx-service

需要迁移的 Service（当前定义在 `cmx-api/src/handlers/xxx/service.rs`）：

| Service | 原位置 | 目标位置 |
|---------|--------|---------|
| DomainService | `cmx-api/src/handlers/domain/service.rs` | `cmx-service/src/domain_service.rs` |
| DomainBmc | `cmx-api/src/handlers/domain/bmc.rs` | `cmx-service/src/domain_bmc.rs` |
| DomainEntity 相关 | `cmx-api/src/handlers/domain/entity.rs` | `cmx-service/src/domain/entity.rs` |
| SysDatasourceService | `cmx-api/src/handlers/sys_datasource/service.rs` | `cmx-service/src/datasource_service.rs` |
| SysDatasourceBmc | `cmx-api/src/handlers/sys_datasource/bmc.rs` | `cmx-service/src/datasource_bmc.rs` |
| 其他自定义 Service | 按需迁移 | 对应模块 |

**迁移注意事项**：
- Service 只依赖 `cmx-database`（DatabaseManager）和 `cmx-core`（DataSet），不依赖 axum，迁移成本低
- 迁移后 `cmx-api` 的 handler 改为调用 `cmx-service` 中的 Service（import 路径变更）
- entity.rs / filter.rs 等类型定义也需要一并迁移或重新导出

#### 2.2 提取 service_call 重复逻辑

当前 `cmx-api/src/handlers/service/handler.rs` 中的 `service_call` 和 `cmx-rpc/src/server.rs` 中的 `call_function` 有大量重复。提取为共享函数：

**`cmx-service/src/function_invoker.rs`（新增）**：

```rust
/// 统一的插件函数调用入口
/// axum handler 和 gRPC server 都调用此函数
pub async fn invoke_plugin_function(
    runtime: &dyn RuntimeInvoker,
    plugin_query: &dyn PluginQuery,
    plugin_id: &str,
    function_name: &str,
    input: Value,
    svr_ctx: SVRContext,
    options: InvokeOptions,
) -> Result<FunctionCallResponse, ServiceError>
```

#### 2.3 封装通用 CRUD 调用入口

**`cmx-service/src/crud_service.rs`（新增）**：

```rust
/// 通用 CRUD 服务入口
/// 封装 GenericCrudService，提供按 entity_type 动态分发的能力
pub struct CrudService;

impl CrudService {
    pub async fn create(entity_type: &str, mm: &DatabaseManager, db_id: &str, data: Value) -> Result<Value> {
        match entity_type {
            "domain" => GenericCrudService::<DomainBmc>::create(mm, db_id, None, data).await,
            "application" => GenericCrudService::<ApplicationBmc>::create(mm, db_id, None, data).await,
            // ...
        }
    }
    // get, update, delete, list, page 类似
}
```

---

### 阶段 3：gRPC 服务端实现

#### 3.1 新增 gRPC Service 实现目录

```
cmx-rpc/src/
  ├── server.rs          (现有：CmxOrchestratorServiceImpl)
  ├── service/           (新增目录)
  │   ├── mod.rs
  │   ├── crud.rs        (CmxCrudServiceImpl)
  │   ├── domain.rs      (CmxDomainServiceImpl)
  │   ├── datasource.rs  (CmxDatasourceServiceImpl)
  │   ├── plugin.rs      (CmxPluginServiceImpl)
  │   └── storage.rs     (CmxStorageServiceImpl)
  └── ...
```

#### 3.2 gRPC Service 实现模式

每个 Service 实现遵循相同的薄层模式：

```rust
// cmx-rpc/src/service/crud.rs
pub struct CmxCrudServiceImpl;

impl CmxCrudService for CmxCrudServiceImpl {
    async fn create(&self, req: CrudRequest) -> Result<CrudResponse, Status> {
        // 1. 解析请求
        let mm = get_default_db_manager();
        // 2. 调用 cmx-service 的 CrudService
        let result = CrudService::create(&req.entity_type, &mm, &req.db_id, data).await;
        // 3. 封装响应
        to_crud_response(result)
    }
}
```

#### 3.3 更新 cmx-rpc 依赖

```toml
# cmx-rpc/Cargo.toml 新增依赖
# 业务逻辑层
cmx-service = { workspace = true }
```

#### 3.4 注册新 gRPC 服务到 server_runner

更新 `cmx-rpc/src/server_runner.rs`，在 `start_grpc_server` 中注册所有新增的 gRPC 服务。

---

### 阶段 4：gRPC 客户端扩展

#### 4.1 扩展 RpcClient trait

在 `cmx-traits/src/rpc_client.rs` 中扩展客户端能力：

```rust
#[async_trait]
pub trait RpcClient: Send + Sync {
    // 现有方法
    async fn call_service(...) -> Result<...>;
    async fn call_function(...) -> Result<...>;

    // 新增：通用 CRUD
    async fn crud_create(&self, service_name: &str, entity_type: &str, db_id: &str, data: Value) -> Result<Value>;
    async fn crud_get(&self, service_name: &str, entity_type: &str, db_id: &str, id: &str) -> Result<Value>;
    async fn crud_update(&self, service_name: &str, entity_type: &str, db_id: &str, data: Value) -> Result<Value>;
    async fn crud_delete(&self, service_name: &str, entity_type: &str, db_id: &str, ids: Vec<String>) -> Result<()>;
    async fn crud_list(&self, service_name: &str, entity_type: &str, db_id: &str, filter: Value) -> Result<Vec<Value>>;
    async fn crud_page(&self, service_name: &str, entity_type: &str, db_id: &str, page: i32, page_size: i32, filter: Value) -> Result<PageResult<Value>>;

    // 新增：Domain 特有
    async fn domain_get_tree(&self, service_name: &str, db_id: &str, parent_id: Option<&str>) -> Result<Value>;

    // 新增：Plugin 管理
    async fn plugin_install(&self, service_name: &str, db_id: &str, data: Value) -> Result<Value>;
    // ...
}
```

#### 4.2 VoloGrpcClient 实现新方法

在 `cmx-rpc/src/client.rs` 中实现所有新增的 trait 方法。

---

### 阶段 5：认证安全（可选，后续迭代）

在 gRPC 层增加内部调用认证，防止未授权的外部 gRPC 调用：

- 通过 gRPC metadata 传递内部调用标识（共享密钥）
- 在 server 端添加 volo interceptor 验证

---

## 五、涉及文件变更清单

### 新增文件

| 文件 | 说明 |
|------|------|
| `cmx-rpc-gen/idl/cmx_crud.proto` | 通用 CRUD gRPC 定义 |
| `cmx-rpc-gen/idl/cmx_domain.proto` | Domain 特有 gRPC 定义 |
| `cmx-rpc-gen/idl/cmx_datasource.proto` | 数据源 gRPC 定义 |
| `cmx-rpc-gen/idl/cmx_plugin.proto` | 插件管理 gRPC 定义 |
| `cmx-rpc-gen/idl/cmx_storage.proto` | 文件存储 gRPC 定义 |
| `cmx-rpc/src/service/mod.rs` | gRPC Service 实现模块 |
| `cmx-rpc/src/service/crud.rs` | 通用 CRUD gRPC 实现 |
| `cmx-rpc/src/service/domain.rs` | Domain gRPC 实现 |
| `cmx-rpc/src/service/datasource.rs` | 数据源 gRPC 实现 |
| `cmx-rpc/src/service/plugin.rs` | 插件 gRPC 实现 |
| `cmx-rpc/src/service/storage.rs` | 存储 gRPC 实现 |
| `cmx-service/src/crud_service.rs` | 通用 CRUD 动态分发 |
| `cmx-service/src/function_invoker.rs` | 插件函数调用统一入口 |

### 修改文件

| 文件 | 变更内容 |
|------|---------|
| `cmx-rpc-gen/volo.yml` | 添加新 proto 文件到生成列表 |
| `cmx-rpc-gen/build.rs` | 更新代码生成配置 |
| `cmx-rpc-gen/src/lib.rs` | 导出新模块 |
| `cmx-rpc/Cargo.toml` | 新增 cmx-service 依赖 |
| `cmx-rpc/src/lib.rs` | 导出 service 模块 |
| `cmx-rpc/src/server_runner.rs` | 注册新 gRPC 服务 |
| `cmx-rpc/src/client.rs` | 实现新 RpcClient trait 方法 |
| `cmx-traits/src/rpc_client.rs` | 扩展 RpcClient trait |
| `cmx-service/Cargo.toml` | 确认/新增依赖 |
| `cmx-api/src/handlers/domain/service.rs` | 删除（迁移到 cmx-service） |
| `cmx-api/src/handlers/domain/handler.rs` | 改为调用 cmx-service |
| `cmx-api/src/handlers/domain/mod.rs` | 更新 import |
| 类似处理其他 handler 模块... | 改为调用 cmx-service |
| workspace `Cargo.toml` | 如需新增 workspace 依赖 |

---

## 六、实施优先级与分批建议

### 第一批（核心框架 + 通用 CRUD）

1. 新增 `cmx_crud.proto`，实现通用 CRUD gRPC 服务
2. 在 `cmx-service` 中创建 `CrudService`（动态分发）
3. 在 `cmx-rpc/src/service/crud.rs` 实现 gRPC 服务端
4. 扩展 `RpcClient` trait + `VoloGrpcClient` 实现
5. 验证：跨实例 CRUD 调用

### 第二批（自定义 Service 下沉 + 特有接口）

1. 将 DomainService、SysDatasourceService 等迁移到 cmx-service
2. 新增各模块特有 proto 和 gRPC 实现
3. 更新 cmx-api handler 调用路径
4. 验证：跨实例 Domain tree、数据源管理等

### 第三批（插件/存储等复杂接口 + 认证）

1. 新增插件管理 gRPC 接口
2. 新增文件存储 gRPC 接口
3. 提取 service_call 重复逻辑
4. 添加 gRPC 内部调用认证

---

## 七、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Service 迁移引入回归 bug | cmx-api handler 调用路径变更 | 迁移后保留 re-export，逐步切换 |
| volo-build 多 proto 生成兼容性 | 代码生成可能冲突 | 先单 proto 验证，再批量添加 |
| 通用 CRUD 的 entity_type 动态分发性能 | 每次调用需 match 分发 | match 是 O(1)，影响可忽略 |
| 大文件/二进制数据通过 gRPC 传输 | protobuf 对大文件不友好 | 文件存储接口考虑用 chunked streaming |
| cmx-rpc 依赖 cmx-service 可能导致编译时间增加 | 新增依赖链 | cmx-service 本身轻量，影响有限 |

---

## 八、验证步骤

1. **编译验证**：`rtk cargo check` 确保所有 crate 编译通过
2. **单元测试**：`CrudService` 的动态分发逻辑
3. **集成测试**：启动两个 web-server 实例，通过 gRPC 客户端调用另一个实例的 CRUD 接口
4. **端到端测试**：模拟完整的服务编排场景，验证 gRPC 调用链路
5. **性能基准**：对比 gRPC 和 HTTP JSON 的吞吐量/延迟
