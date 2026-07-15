# 项目级可复用资产清单（Reuse Catalog）

> 适用范围：`cmx-container` workspace 全量。
> 维护者：rust-arch-review 技能。
> 作用：作为「代码复用」维度的扫描标尺。AI 在审查时，必须先用本清单核对目标代码是否复用了项目已有资产；未复用的需要标注「复用偏离」。

---

## 〇、如何使用本清单

1. **审查前**：通读一遍本清单，对项目已有资产建立整体认知。
2. **审查中**：对每段目标代码，按本清单的「锚点关键词」逐项 Grep 比对：
   - `dv!` 宏 → 是否用了 `cmx_core::dv!` 批量构造 `DataValue`
   - `ParamsBuilder` → 是否复用了动态 SET 子句构造器
   - `GenericCrudService` → Service 层是否复用
   - `DbBmc` → BMC 结构体是否实现
   - `declare_crud_handlers!` → Handler 路由是否复用
   - `modql::Fields` / `modql::FilterNodes` → Entity / Filter 是否 derive
   - `cmx-traits::*` → 是否复用了跨模块 trait
3. **审查后**：将「未复用」的项目列入报告的「复用偏离度」一节。

---

## 一、cmx-core（核心领域模型 + 域构造工具）

**位置**：[`crates/libs/cmx-core/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-core/src)

| 资产 | 路径 / 锚点 | 用途 | 适用场景 | 误用反例 |
|------|------------|------|---------|----------|
| **`dv!` 宏** | `model/cell.rs:686` | 批量构造 `Vec<DataValue>`，自动类型推断 | SQL 参数构造 | ❌ `vec![id.into(), name.into()]` 链式 `.into()`（冗长且丢类型） |
| **`ParamsBuilder`** | `model/builder.rs` | 动态 SET 子句构造器，自动管理 `$N` 占位符 | 动态 UPDATE（字段可选） | ❌ `format!("$1, $2, ...")` 手写占位符（漂移风险） |
| **`DataValue` / `SqlParam`** | `model/cell.rs` | 类型安全的 SQL 参数容器 | 跨数据库执行 SQL | ❌ `serde_json::Value` 整型 NULL 退化 |
| **`CoreError`** | `error.rs` | 基础层 Error 类型 | 跨 crate 错误定义 | ❌ 自定义 `enum FooError` 不派生 thiserror |
| **`DataSet`** | `model/data/dataset` | 行结果集封装 | SQL 查询结果 | ❌ 自定义 `Vec<HashMap>` |
| **`PermissionDeniedError` / `RoleRequirement`** | `model/iam` | 权限相关基础类型 | Handler 鉴权失败 | ❌ 手写 `PermissionError` |

**审查锚点（Grep 关键词）**：

```bash
# 应复用 dv! 宏的代码位置
grep -rn "vec!\[.*\.into()\]" crates/libs/

# 应复用 ParamsBuilder 的代码位置
grep -rn 'format!("\$\d' crates/libs/

# 应复用 DataValue 的代码位置
grep -rn "serde_json::json!" crates/libs/cmx-biz/src  # 仅作为初筛
```

---

## 二、cmx-utils（工具库 + 配置管理 + ID 生成 + 加密 + ZIP）

**位置**：[`crates/libs/cmx-utils/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-utils/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`ConfigManager`** | `config/` | 全局配置单例 | 应用启动后读取配置 | ❌ 直接 `std::env::var("...")` 散落各模块 |
| **`Config::builder()`** | `config/` | 多源配置构造器 | 启动时配置加载 | ❌ 手写文件 IO + 解析 |
| **`UuidGenerator` / `snowflake_id` / `next_pk_id`** | `id/` | 各种 ID 生成器 | 主键、请求 ID | ❌ `rand::random()` 或 `uuid::Uuid::new_v4()` 散落调用 |
| **`Pk52Generator`** | `id/pk52.rs` | 短码 ID（52 进制） | 短链、对外编码 | ❌ 自实现 base64 编码 ID |
| **`ZipCompressor` / `ZipExtractor`** | `zip/` | ZIP 压缩/解压 | 插件包、报表导出 | ❌ 直接调 `zip` crate API |
| **`crypto::*`** | `crypto/` | AES-GCM 对称加密 | 敏感字段存储 | ❌ 手写 XOR 加密或裸密码 |
| **`b64::encode/decode`** | `b64.rs` | Base64 编码 | 短文本编码 | ❌ `base64::encode(...)` 直接调 |
| **`time::*`** | `time/` | 时间工具函数 | 时间格式化、解析 | ❌ 手写 `chrono::Utc::now().format(...)` 链 |
| **`read_lock` / `write_lock`** | `sync_utils.rs` | `RwLock` 守卫 | 并发读多写少 | ❌ `let _g = lock.read().unwrap();` 裸用 |
| **`json::*`** | `json.rs` | JSON 工具 | 简单 JSON 操作 | ❌ `serde_json::from_str(...)` 散落 |

**审查锚点**：

```bash
# 应复用 ConfigManager 的代码位置
grep -rn "std::env::var" crates/libs/

# 应复用 ID 生成器的代码位置
grep -rn "Uuid::new_v4\|rand::random" crates/libs/
```

---

## 三、cmx-database（数据库基础设施 + 通用 CRUD）

**位置**：[`crates/libs/cmx-infra/cmx-database/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`DatabaseManager`** | `manager/` | 数据库连接池 + 事务管理 | 任何 SQL 执行 | ❌ 直接创建 `PgPool` |
| **`GenericCrudService<MC>`** | `crud/crud_fns.rs:57` | 通用 CRUD（create/get/update/delete/list/page） | Service 层基础操作 | ❌ 手写 CRUD SQL |
| **`DbBmc` trait** | `crud/mod.rs:24` | BMC 必须实现 | 每个实体表对应一个 BMC | ❌ BMC 缺失或字段不全 |
| **`execute_sql_with_datavalues`** | transaction | 推荐 SQL 执行 API | 参数化 SQL | ❌ `execute_sql_with_json`（整型 NULL 退化） |
| **`with_transaction_by_id`** | transaction | 事务管理 | 多语句事务 | ❌ 手写 `pool.begin()` + 显式 commit/rollback |
| **`QueryBuilder` / `CompareOp` / `OrderDirection`** | `types/` | SQL 查询构造 | 动态查询 | ❌ 字符串拼接 SQL |
| **`MigrationRunner` / `MigrationLoader`** | `migration/` | 数据库迁移 | DDL 迁移 | ❌ 手写 `apply_sql` 一次性脚本 |
| **`check_long_running_transactions` / `cleanup_completed_transactions`** | transaction | 事务健康检查 | 监控 | ❌ 忽略事务泄漏 |
| **`ParamValue` / `ResultConverter`** | `executor/` | 类型转换 | 跨数据库兼容 | ❌ 直接 `from_row::<T>()` |

**审查锚点**：

```bash
# 应复用 GenericCrudService 的 Service 层
grep -rn "INSERT INTO\|UPDATE.*SET" crates/libs/cmx-biz/src  # 初筛

# 应使用 execute_sql_with_datavalues 而非 with_json
grep -rn "execute_sql_with_json" crates/libs/

# 应使用 with_transaction_by_id 而非裸事务
grep -rn "pool\.begin()" crates/libs/
```

---

## 三-B、cmx-database-pg（PG-only 性能优化分支）

**位置**：[`crates/libs/cmx-infra/cmx-database-pg/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-infra/cmx-database-pg/src)

> **核心定位**：cmx-database 的 PG-only 性能优化分支，API 高度对齐，但**默认不使用**。无任何 crate 独家依赖它（4 个消费方同时挂着 cmx-database）。
>
> **选择规则**：默认用 cmx-database；仅在需要下方 4 项独有能力时引入 cmx-database-pg。

### 独有资产（cmx-database 没有的）

| # | 资产 | 路径 | 用途 | 误用反例 |
|---|------|------|------|----------|
| ① | **`query_sql_zmc_stream_chunks`** | `manager/mod.rs:374` + `connection/mod.rs:207` | 真·分帧流式：基于 `mpsc::Sender<Bytes>`，逐行编码为长度分帧发送，峰值内存 O(单行)，16KB 攒批刷写，header 帧先发、空结果容错 | ❌ 小结果集用此（无收益，徒增依赖） |
| ② | **数组列读取还原** | `executor/mod.rs:435-452`（`PgResultConverter::convert_rows`） | 读取阶段支持 TEXT_ARRAY / INT8_ARRAY / UUID_ARRAY -> `DataValue::Array`。cmx-database 读取方向**不还原数组**（只在绑定时支持写入） | ❌ 误以为 cmx-database 也能读取数组列 |
| ③ | **`get_conn()`** | `connection/mod.rs:112` | 返回 `deadpool_postgres::Object`，供事务层跨 await 手动驱动 BEGIN/COMMIT | ❌ cmx-database 用 sqlx 的 `pool.begin()`，无需此方法 |
| ④ | **4 个 ToSql 适配器** | `executor/mod.rs:24-123` | `PgInt` / `PgDateTime` / `PgDateTimeNull` / `PgIntNull`。tokio-postgres 类型校验严格（i64 绑 INT4 列会 WrongType），需宽度/时区自适应包装 | ❌ 这是 tokio-postgres 驱动刚需，sqlx 隐式协调不需要 |

### 与 cmx-database 的 API 对齐关系

| API | cmx-database | cmx-database-pg | 说明 |
|-----|:---:|:---:|------|
| `execute_sql_with_datavalues` | ✅ | ✅ | 完全对齐 |
| `query_sql_with_datavalues` | ✅ | ✅ | 完全对齐 |
| `query_sql_zmc` | ✅（sqlx PgRow） | ✅（tokio-postgres Row） | 返回类型不同但语义一致 |
| `query_sql_zmc_with_datavalues` | ✅ | ✅ | 同上 |
| `query_zmc_streaming`（写入 Vec<u8>） | ✅ | ✅ | **两者都有**，非独占 |
| `query_sql_zmc_stream_chunks`（mpsc 通道） | ❌ | ✅ | **pg 独占** |
| `get_conn()` | ❌ | ✅ | pg 独占（tokio-postgres 驱动刚需） |
| 数组列读取还原 | ❌（仅写入支持） | ✅ | pg 独占 |
| ToSql 适配器（PgInt 等） | ❌ | ✅ | pg 独占（tokio-postgres 驱动刚需） |
| `execute_sql_with_json` | ⚠️ 不推荐 | ⚠️ 不推荐 | 两 crate 均不推荐新代码用 |

> ⚠️ **注意区分**：`query_zmc_streaming`（写入 `Vec<u8>`）**两者都有**；唯独 `*_stream_chunks`（mpsc 通道）是 pg 独有。

### 依赖现状与替换指南

**依赖现状**（无任何 crate 独家依赖 cmx-database-pg）：

| 情形 | crate 数 | 具体 |
|------|---------|------|
| 同时依赖两者 | 4 | cmx-api、cmx-biz、cmx-database-test、web-server |
| 只依赖 cmx-database-pg | **0** | 无 |
| 只依赖 cmx-database | 9 | cmx-api-types、cmx-iam、cmx-audit、cmx-auth、cmx-storage、cmx-metadata、cmx-plugin、cmx-portal、cmx-service |

🟢 **可以无痛替换为 cmx-database** 的场景：
- 只用到 `execute_sql*` / `query_sql*` / `query_sql_zmc` / `query_sql_zmc_with_datavalues` / `crud::*` / `transaction::*` / `migration::*` / `host_functions` / `ZmcDataSet`
- 注意 `SqlParams::SeaValues` -> `SqlxValues` 枚举变体替换
- 具体使用点：cmx-api 的 `dct.rs`/`doc.rs`、web-server 的 `datasource.rs`、cmx-biz 的 `zmc_loader.rs`

🔴 **不能简单替换** 的场景（需迁移实现）：
- 依赖 `query_sql_zmc_stream_chunks`（如 `mem_bench.rs`、O(单行) 内存流式消费）
- 依赖数组列读取还原（`DataValue::Array` 从数据库读取）
- 直接依赖 `TokioPgRowSource` 全路径（如 `cmx-database-test` 的 `e2e_server.rs:338`、`mem_bench.rs`）需改为 `SqlxPgRowSource`

> **导出对称性缺口**（不影响功能）：cmx-database 把 `SqlxPgRowSource` 提升到了 crate 根（`lib.rs:29`），而 pg 侧的 `TokioPgRowSource` 只能走全路径 `cmx_database_pg::zmcdataset::TokioPgRowSource`。

### 审查锚点

```bash
# 检查是否滥用 cmx-database-pg（非独占能力场景应改回 cmx-database）
grep -rn "cmx_database_pg\|get_default_pg_db_manager" crates/libs/ | grep -v "stream_chunks\|zmc"

# 检查 cmx-database-pg 的 with_json 使用（不推荐）
grep -rn "cmx_database_pg.*with_json" crates/libs/

# 检查事务内误用 query_sql_zmc（应走 query_sql_with_datavalues）
grep -rn "query_sql_zmc" crates/libs/ | grep -i "txn"

# 检查是否在用 TokioPgRowSource 全路径（可考虑改回 SqlxPgRowSource）
grep -rn "TokioPgRowSource" crates/libs/
```

> 详细 SQL 执行 API 选择规则见 [cmx-sql-execution 技能 §2.0](../../cmx-sql-execution/SKILL.md)。

---

## 四、cmx-traits（跨模块 trait 接口抽象层）

**位置**：[`crates/libs/cmx-traits/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-traits/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`AuthService` / `AuthPolicy` / `UserAuthQuery` / `AuthStorageQuery`** | `auth/` | 认证域 trait | 登录、Token 验证、用户信息查询 | ❌ 直接 import `cmx_iam` 具体类型 |
| **`PermissionChecker` / `DataScope`** | `iam/` | 权限域 trait | 鉴权检查 | ❌ 手写权限校验 |
| **`PluginQuery` / `PluginLifecycleListener`** | `plugin/` | 插件域 trait | 插件状态查询、生命周期事件 | ❌ 强依赖 `cmx_plugin` 具体实现 |
| **`ResourceDataImporter` / `DefinitionImporterBundle`** | `resource/` | 资源数据导入 | Form/Menu/Perm 导入 | ❌ 手写 JSON 解析导入 |
| **`RuntimeInvoker` / `HostFunctionProvider` / `InvokeContext`** | `runtime/` | WASM 运行时域 trait | 插件函数调用 | ❌ 直接 `cmx_runtime::WasmEngine` 强依赖 |
| **`ServiceQuery` / `ServiceStorage` / `ServiceInvoker`** | `service/` | 服务域 trait | 服务注册、调用 | ❌ 直接 import `cmx_service` 类型 |
| **`ServiceOrchestrationClient` / `ResourceDataClient`** | `rpc/` | RPC 客户端 | 跨服务调用 | ❌ 直接调 `cmx_rpc` 强依赖 |
| **`EventBus` / `GlobalEventBus`** | `event_bus/` | 事件总线 | 模块间解耦通信 | ❌ 直接 `Arc<Mutex<HashMap>>` 实现事件 |
| **`FunctionInvoker` / `FunctionInvokeResult`** | `function_invoker/` | 插件函数调用 | 跨模块调用插件函数 | ❌ 直接调运行时 |
| **`step_status::*`** | `step_status.rs` | StepStatus 字符串编解码 | 编排状态显示 | ❌ 字符串硬编码 |
| **`TraitError` / `HostFuncError`** | `error.rs` | 跨模块错误 | 错误传播 | ❌ 自定义错误类型不一致 |

**审查锚点**：

```bash
# 应通过 cmx-traits 抽象依赖
grep -rn "use cmx_plugin::" crates/libs/cmx-service/src
grep -rn "use cmx_iam::" crates/libs/cmx-api/src

# 应使用 EventBus 而非自定义事件
grep -rn "Arc<Mutex<HashMap" crates/libs/  # 初筛
```

---

## 五、cmx-biz（业务抽象层）

**位置**：[`crates/libs/cmx-biz/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-biz/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **8 大业务模块** | `application/` `datasource/` `doc/` `domain/` `module/` `form/` `menu/` `validation/` | 业务实体的 Entity/BMC/Filter/Service 标准四件套 | 任何业务实体接入 | ❌ 在 cmx-api 中重定义 Entity |
| **`ResourceDataImporterImpl`** | `resource_importer/` | 多类别资源数据导入（Form/Menu/Perm） | 资源数据导入 | ❌ 手写 JSON 解析 + 单类实现 |
| **`function_invoker`** | `function_invoker/` | 插件函数调用核心逻辑 | 协议无关的函数调用 | ❌ 在 handler 中手写函数调用流程 |
| **`service_executor`** | `service_executor/` | 服务编排执行核心逻辑 | 协议无关的编排 | ❌ 在 handler 中手写编排执行 |
| **`dam_asset_service`** | `dam_asset_service.rs` | DAM 资产文件服务 | 资源目录创建/改名/校验 | ❌ 手写目录操作 |
| **`errcode::*`** | `errcode/` | 统一错误码 | 业务错误码管理 | ❌ 散落的 `const X: i32 = 1001` |
| **`validation::*`** | `validation/` | 落库前列级校验 | 字段规范校验 | ❌ 在 handler 中手写校验 |

**审查锚点**：

```bash
# cmx-api 中不应手写 SQL（应走 cmx-biz Service）
grep -rn "execute_sql\|query_sql" crates/libs/cmx-api/src/handlers/

# cmx-api 中不应重定义 Entity
grep -rn "pub struct.*Entity" crates/libs/cmx-api/src/handlers/
```

---

## 六、cmx-api（HTTP 适配层 + 通用 Handler 框架）

**位置**：[`crates/libs/cmx-api/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`declare_crud_handlers!` 宏** | `routes/macros.rs` | 一行声明 8 个 CRUD handler | 标准实体 Handler | ❌ 手写 8 个 `async fn` |
| **`register_crud_routes!` 宏** | `routes/macros.rs:21` | 路由注册（旧版，无 OpenAPI） | 简单内部 API | ❌ 散落 `.route("/create", post(...))` |
| **8 个通用 Handler** | `rest/handler.rs` | `create` / `get_by_id` / `update` / `delete` / `list` / `page` 等 | 任何 CRUD 路由 | ❌ 重新实现通用 CRUD 逻辑 |
| **`CmxAppState`** | `app_state.rs` | 全局应用状态 | 注入业务 Service | ❌ 手写 `Arc<Mutex<...>>` |
| **`ApiResp<T>` / `Pagination` / `UnitResp`** | re-export from `cmx-api-types` | 统一响应格式 | Handler 返回值 | ❌ 自定义 `Json<MyResp<T>>` |
| **`Error` / `ErrCode` / `Result`** | re-export from `cmx-api-types` | 统一错误 | Handler 错误 | ❌ `anyhow::Error` 直接返回 |
| **`ModuleRoutes` trait** | `routes/traits.rs` | 路由模块化注册 | 多模块路由 | ❌ 在 `main.rs` 中散落注册 |
| **`OpenAPI 注解工具`** | `routes/macros.rs` | utoipa 文档生成 | API 文档 | ❌ Handler 缺 `#[utoipa::path]` |

**审查锚点**：

```bash
# 应使用 declare_crud_handlers! 宏而非手写
grep -rn "async fn create\b\|async fn list\b" crates/libs/cmx-api/src/handlers/

# 应使用统一响应类型
grep -rn "Json<.*>" crates/libs/cmx-api/src/handlers/  # 初筛自定义响应

# 应使用 ModuleRoutes trait
grep -rn "impl.*Router\b" crates/libs/cmx-api/src/handlers/  # 初筛
```

---

## 七、cmx-macros（属性宏库）

**位置**：[`crates/libs/cmx-macros/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-macros/src)

| 资产 | 用途 | 适用场景 | 误用反例 |
|------|------|---------|----------|
| **`#[has_permission]`** | 单权限检查 + 元数据注册 | Handler 需要单权限 | ❌ 函数体内手写 `require_permission` |
| **`#[has_permissions(...)]`** | 多权限 AND 检查 | Handler 需要多权限 | ❌ 链式 `require_all_permissions` |
| **`#[has_any_permission(...)]`** | 多权限 OR 检查 | Handler 任一权限 | ❌ 链式 `require_any_permission` |
| **`#[has_role]`** | 单角色检查 | Handler 需要单角色 | ❌ `require_role` 散落 |
| **`#[has_roles(...)]`** | 多角色 AND 检查 | 多角色 | ❌ 链式 `require_all_roles` |
| **`#[has_any_role(...)]`** | 多角色 OR 检查 | 任一角色 | ❌ 链式 `require_any_role` |
| **`#[permit_all]`** | 公开访问标记 | 不需鉴权 | ❌ 注释说明 |

**审查锚点**：

```bash
# 应使用属性宏而非函数体手写
grep -rn "require_permission\|require_role" crates/libs/cmx-api/src/handlers/  # 初筛
```

---

## 八、cmx-iam（用户权限角色管理）

**位置**：[`crates/libs/cmx-iam/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-iam/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`PermissionService::import_permissions`** | `permission/service/import.rs` | 权限批量导入（含 diff + 事务） | 模块/插件安装导入权限 | ❌ 重新实现两阶段 upsert |
| **`RoleService` / `UserService` / `UserAuthQueryImpl`** | `service_traits.rs` | 用户/角色 CRUD | 业务接入 IAM | ❌ 在 cmx-biz 中重定义 |
| **`IamChecker`** | `iam_checker.rs` | 权限/角色/数据范围统一检查 | 鉴权入口 | ❌ 散落的权限检查 |
| **`RuleEnforcer` / `RuleEnforcerImpl`** | `rule/` | 互斥/冲突规则 | 角色互斥 | ❌ 手写规则引擎 |
| **`ExclusionRuleServiceImpl`** | `rule/` | 互斥规则 CRUD | 业务管理 | ❌ 直接 SQL |
| **`audit_helper`** | `audit_helper.rs` | 审计日志助手 | 业务审计 | ❌ 散落 `audit::log` |

**审查锚点**：

```bash
# 应使用 import_permissions 而非手写
grep -rn "parent_id = NULL" crates/libs/  # 出现即两阶段 upsert 反模式

# 应使用 IamChecker 而非散落
grep -rn "check_permission\|has_permission" crates/libs/  # 初筛
```

---

## 九、cmx-plugin（插件生命周期）

**位置**：[`crates/libs/cmx-plugin/src/`](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src)

| 资产 | 路径 | 用途 | 适用场景 | 误用反例 |
|------|------|------|---------|----------|
| **`DeployService`** | `service/` | 插件部署（安装/升级/降级/重装） | 部署插件 | ❌ 散落 deploy 逻辑 |
| **`ModuleInstallService`** | `service/module_install.rs` | 模块导入（含子插件） | 导入模块包 | ❌ 手写模块解析 |
| **`DeployService` 与 `ModuleInstallService` 共享的 `executor`** | `core/manager.rs` | 共享插件操作执行器 | 共享执行 | ❌ 重复实现 executor |
| **`audit::*`** | `audit/` | 插件审计日志 | 审计 | ❌ 散落 audit |

**审查锚点**：

```bash
# 应复用 DeployService 而非手写部署
grep -rn "fn deploy\b\|fn install\b" crates/libs/cmx-plugin/src/  # 初筛

# 应复用 ModuleInstallService
grep -rn "fn import\b\|fn export\b" crates/libs/cmx-plugin/src/
```

---

## 十、cmx-form / cmx-portal / cmx-metadata（其他业务域）

| 资产 crate | 关键导出 | 用途 | 误用反例 |
|-----------|---------|------|----------|
| **cmx-form** | `pages::form::Form` / `pages::html::HtmlPage` / `pages::native::NativePage` | 三种表单页面渲染 | ❌ 在 cmx-api 中直接拼 HTML |
| **cmx-portal** | `agent/` / `dam/` / `fact/` / `help/` / `launcher/` / `meta/` / `notify/` | 门户/设计器业务模块 | ❌ 把门户逻辑写进 cmx-api |
| **cmx-metadata** | `ddl::*` / `parser::*` / `seed::*` / `executor::*` | 表元数据 DDL 生成/解析/执行 | ❌ 手写 DDL 字符串 |
| **cmx-model** | `dict::*` | 字典元数据建模 | ❌ 在 cmx-api 中手写字典解析 |

---

## 十一、cmx-runtime / cmx-service / cmx-rpc

| 资产 crate | 关键导出 | 用途 | 误用反例 |
|-----------|---------|------|----------|
| **cmx-runtime** | `engine::*` / `config::*` | WASM 引擎 | ❌ 直接调 wasmtime API |
| **cmx-service** | `service::*` / `orchestrator::*` | 服务编排 | ❌ 在 handler 中手写编排 |
| **cmx-rpc** | `lib::*` | RPC 客户端/服务端 | ❌ 直接调 volo/reqwest API |

---

## 十二、其他工具与技能联动

| 工具 / 技能 | 位置 | 用途 | 误用反例 |
|------------|------|------|----------|
| **`axum-handler-generator`** | `.agents/skills/axum-handler-generator` | Handler 生成标准 | ❌ 手写 handler 模板 |
| **`modql`** | `.agents/skills/modql` | Filter / Entity 设计 | ❌ 用原始 `String`/`i64` 作 filter 字段 |
| **`cmx-sql-execution`** | `.agents/skills/cmx-sql-execution` | SQL 执行规范 | ❌ 用 `execute_sql_with_json` |
| **`pg-table-generator`** | `.agents/skills/pg-table-generator` | 表 DDL 生成 | ❌ 手写 DDL |
| **`sql-guide`** | `.agents/skills/sql-guide` | SQL 迁移规范 | ❌ 缺迁移文件 |
| **`config-sync`** | `.agents/skills/config-sync` | TOML/ENV 配置维护 | ❌ 改 TOML 不更新文档 |
| **`wasm-plugin-developer`** | `.agents/skills/wasm-plugin-developer` | WASM 插件开发 | ❌ 不用三层分离 |
| **`plugin-metadata-generator`** | `.agents/skills/plugin-metadata-generator` | 插件元数据 | ❌ 缺 metadata |
| **`service-orchestration-generator`** | `.agents/skills/service-orchestration-generator` | 编排图 | ❌ 手写编排 |
| **`plugin-fn-doc`** | `.agents/skills/plugin-fn-doc` | `#[plugin_fn]` 注释 | ❌ 缺函数注释 |
| **`rust-comment-convention`** | `.agents/skills/rust-comment-convention` | 注释规范 | ❌ 注释不规范 |
| **`clippy-fix`** | `.agents/skills/clippy-fix` | Clippy 警告 | ❌ 不跑 clippy |

---

## 十三、复用偏离度评分规则

审查时，按以下公式计算：

```
偏离率 = (应复用但未复用次数) / (应复用总次数) × 100%
```

- **偏离率 < 10%**：✅ 复用充分，无需单独列项
- **偏离率 10%–30%**：🟡 存在少量遗漏，列入"🟡 警告"区
- **偏离率 30%–60%**：🟠 复用不足，列入"🟠 中等问题"区
- **偏离率 > 60%**：🔴 严重偏离，列入"🔴 严重问题"区

> **"应复用总次数"如何估算**：根据审查目标范围人工估测。例如审查一个 5 个实体的 cmx-biz 模块，`GenericCrudService` 至少应被复用 5 次（每个实体 1 次），缺一次算一次偏离。

---

## 十五、关联文件

- 上级技能：[SKILL.md](../SKILL.md)
- 项目总规范：[AGENTS.md 18 章](../../../../AGENTS.md)
- 同级引用：[checklist.md §B2](./checklist.md#b2-代码复用核心新增) / [anti-patterns.md §一 重复造轮子](./anti-patterns.md#一重复造轮子reinventing-the-wheel) / [report-template.md §3.1 复用偏离度表](./report-template.md#三模块设计b)

## 十六、本清单维护规则

1. **新增 crate** 时：本清单"十、cmx-form / ... " 章节同步添加。
2. **新增核心 trait / 宏** 时：本清单"四 / 七" 章节同步添加。
3. **发现新的误用反例** 时：在对应章节"误用反例"列追加。
4. **审查中发现的复用空白**：列入"十七、复用空白（待补）"清单，作为下一轮迭代的素材。

## 十七、复用空白（待补）

> 审查过程中发现但尚未纳入正式清单的资产，由维护者在下次迭代时归类。

- （暂无）

> 条目格式：`- 资产名 — 位置 — 用途 — 备注`
