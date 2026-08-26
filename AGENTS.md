# cmx-container 项目开发规范

> AI 在本项目中开发时遵循的规范。

> **定位**：cmx-container 是**后端公用库 + 插件平台 workspace**——本 workspace **无可执行 server bin**。可执行微服务已拆到独立仓库：门户主应用 `cmx-portal-server`（在 `cmx-portalservice/`）、流程服务 `cmx-flow-server`（在 `cmx-flowengine/`）。它们经 `path = "../cmx-container/crates/..."` 跨 workspace 反向引用本 workspace 的公用库（`cmx-core` / `cmx-database-pg` / `cmx-apis/*` / `cmx-biz` 等）。改本仓库公用库 API / trait 时，注意这两个下游 workspace 的引用方，需分别 `cargo check` 验证。

---

## 一、Error 处理规则

### 1.1 必须使用 thiserror 库

所有自定义 Error 使用 `#[derive(thiserror::Error)]`：

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("操作失败: {0}")]
    OperationFailed(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### 1.2 禁止手写 Error 实现

- 禁止 `impl std::error::Error` 或 `impl Display`
- 禁止 `derive_more::From`（与 thiserror 冲突）
- 使用 `#[error(...)]` 属性定义错误消息

### 1.3 Error 定义位置

- 每个 crate 有独立 error 模块 (`src/error.rs`)
- 使用 `pub type Result<T> = core::result::Result<T, Error>;`

### 1.4 禁止裸用 unwrap()

在编写 Rust 代码时，禁止在生产环境中直接使用 `unwrap()` 方法。

**正确做法**：

1. **首选方案**：使用 `?` 操作符将错误优雅地向上抛出（`Result`）。
2. **兜底方案**：如果当前逻辑确实无法处理该错误且必须中断，请使用 `expect("...")`，并在括号内提供具有业务指导意义的错误提示（例如：`expect("Redis 连接池初始化失败，请检查配置")`）。

**例外情况**：

仅在 `#[test]` 单元测试函数内部，或者 100% 确信该值不可能为 `None` 的极特殊场景下，才允许使用 `unwrap()`。

---

## 二、日志处理规则

### 2.1 必须使用 tracing 库

```rust
// ✅ 正确
use tracing::{info, warn, error};
info!("操作成功");

// ❌ 错误
use log::info;
```

### 2.2 日志格式

**使用 tracing 结构化字段输出，禁止仅用占位符拼接。** 消息模板必须为静态字符串（便于日志聚合检索），动态上下文走 `key = value` 字段；这样下游（ELK / Loki / 检索）才能按字段过滤和聚合，而不是从一坨字符串里抠信息。

#### 静态消息模板 + 字段（✅ 正例）

```rust
// ✅ 正确：静态消息 + 结构化字段
info!(
    target: "cmx::payment",
    payment.id = %payment_id,
    user.id = %user_id,
    amount = amount,
    "开始处理支付"
);

warn!(
    target: "cmx::order",
    order.id = %order_id,
    retry.count = retry,
    error = %err,
    "订单处理失败，将重试"
);
```

要点：
- **消息模板是静态字符串**（编译期字面量），不要在里面塞 `{}` 拼接。
- **字段名用 `.` 分层命名空间**（`payment.id`、`user.id`），便于聚合与索引。
- **Display 类型用 `%` 前缀**（如 `%payment_id`），`Debug` 类型用 `?` 前缀（如 `?err`）。
- **必填业务字段全部上送**（ID、租户、关键数值），不要漏。
- **业务模块用 `target:`** 显式声明（`"cmx::payment"`），便于按模块过滤。

#### 常见反例（❌ 禁止）

```rust
// ❌ 错误：字符串里塞变量，无法按字段检索
info!("开始处理支付 payment_id={} user_id={}", payment_id, user_id);

// ❌ 错误：动态拼接的"消息模板"，失去静态检索能力
info!(format!("处理支付 {}", payment_id), user_id = user_id);

// ❌ 错误：把上下文塞进消息里、字段为空
info!("处理支付成功");

// ❌ 错误：Debug 整对象，泄漏结构且字段不可索引
info!("payment = {:?}", payment);

// ❌ 错误：把业务数据塞进 target 或 message
info!(target: &format!("cmx::payment::{}", payment_id), "处理中");
```

反例共同问题：**字段不可结构化检索、敏感数据混在消息里、聚合统计困难、日志解析性能差**。

---

## 三、依赖管理规则

### 3.1 Workspace 依赖集中管理

所有第三方依赖在 workspace `Cargo.toml` 统一定义，子 crate 禁止直接指定版本，`cmx-wasmdemo`除外。

### 3.2 Crate 引用方式

使用 `workspace = true` 引用，允许添加额外 features：

```toml
# ✅ 正确
uuid = { workspace = true, features = ["v4"] }

# ❌ 错误
uuid = "1.21"
```

### 3.3 依赖注释要求

每个依赖上方必须添加单独注释，禁止分组注释：

```toml
# ✅ 正确
# 序列化框架
serde = { workspace = true }

# ❌ 错误 - 禁止分组注释
# ============================================
serde = { workspace = true }
```

注释格式：`# <用途>` 或 `# <分类> - <用途>`，内部依赖：`# 内部依赖 - <模块>`

### 3.4 禁止使用 log crate

```toml
# ❌ 错误
log = "0.4"
```

### 3.5 新增依赖流程

1. 检查 workspace 是否已定义
2. 未定义则先在 workspace 添加并注释
3. 用 `workspace = true` 引用

### 3.6 未使用依赖

注释保留而非删除：

```toml
# [dependencies]
# unused_crate = "1.0"
```

---

## 四、Git 提交规则

### 4.1 禁止自动提交代码

AI 助手在完成任务后，**禁止主动执行 `git commit` 等提交操作**，必须由用户主动确认并提出提交请求后才能提交。

---

## 五、SQL 与配置维护规则

### 5.1 新建表结构

新建 PostgreSQL 表时，**推荐**使用 `pg-table-generator` 技能生成 DDL，确保表结构符合项目规范（标准审计字段、命名规则、注释规则等）。

### 5.2 SQL 文件维护

涉及 SQL 变更（新建表、新增/修改/删除字段、新增索引等）时，**必须**使用 `sql-guide` 技能同步维护（v2 双库结构，详见 `docs/sql/v2/README.md`）：

- **新建表必须先询问用户归属**：主库（`docs/sql/v2/platform/`）还是业务库（`docs/sql/v2/biz/`），得到明确答复后再写 SQL；下述前缀规则仅是用户未明示时的默认参考与事后校验
- 前缀默认规则：`cmx_` 前缀 → platform（主库），**两组例外前缀 → biz（业务库）**——`cmx_flow_*` 流程运行态（与流程引擎 `FLOW_DB_ID`=业务库一致）、`cmx_code_*` 编码引擎（运行时 code API 经 `resolve_db_id` 回退业务库）；其余前缀（`md_*`/`mdm_*`/`cf_*`/`cr_*` 等）→ 业务库；跨库变更拆两个文件
- 对应库 `migrations/` 下创建增量迁移文件（`YYYYMMDD_NNN.up.sql` + `.down.sql`）
- 对应库 `init_ddl.sql` / `init_dml.sql` 同步更新为最新完整状态（无损幂等风格，init_ddl 禁 ALTER）
- `docs/sql/init | migrations | seed/` 为历史归档，**只读不改**（引擎不再读取）

### 5.3 配置文档维护

新增或修改 TOML 配置项、环境变量时，**必须**使用 `config-sync` 技能同步维护：

- TOML 配置 → `config/config_template.toml` + `config/CONFIG_MANUAL.md`
- 环境变量 → `config/.env.template` + `config/ENV_MANUAL.md`

> 模板文件的注释层级（L1 / L2 / L3）、配对规则等详见 `config-sync` 技能第三章。

### 5.4 表名与字段硬约束

- **系统基础表名必须以 `cmx_` 前缀**（如 `cmx_file_detail`、`cmx_account`、`cmx_plugin`）；业务模块 / 插件自建的表可不加（由模块自行命名）
- **禁止外键约束**（`FOREIGN KEY`），保留关联字段并用 `CREATE INDEX` 替代
- 标准审计字段（8 项 = id + 7 审计）、树形 / 层级字段（5 项）、DDL 幂等性、`COMMENT` 格式等完整规范见 `pg-table-generator` 与 `sql-guide` 技能

### 5.5 迁移文件命名

```
<日期>_<3位序号>_<描述>.up.sql
<日期>_<3位序号>_<描述>.down.sql
```

- 序号 001 起递增；新日期重置；**禁止**跳跃；version（日期_序号）在各自库内唯一
- INSERT 迁移数据**必须**用 `ON CONFLICT DO NOTHING` / `ON CONFLICT DO UPDATE`
- DDL 一律 `IF NOT EXISTS`；up 文件**禁止** `DROP TABLE`（破坏性，废弃表走归档说明）

### 5.6 init_ddl.sql 维护原则

`docs/sql/v2/{platform,biz}/init_ddl.sql` 始终保持**最新完整状态 + 无损幂等 +
表定义即终态**（`CREATE TABLE/INDEX IF NOT EXISTS`，禁 DROP TABLE，**禁任何 ALTER**——
字段变更直接改表定义并按 §5.5 同步新增迁移）；每表区块布局为
「CREATE TABLE → COMMENT（紧跟建表）→ 索引」。存量库升级走基线迁移
（基线内含结构对齐 ALTER 区，由迁移链增量积累，init_ddl 不含）。

---

## 六、app_id 与 module_code 关系约束

### 6.1 app_id 取值规则（按 `[deploy] mode` 切换）

> 本节规则自 2026-07-21 起更新，对应方案：[单体与分体部署模式统一方案](documents/20260721_cmx-container_单体与分体部署模式统一方案.md)

`app_id` 的取值由 `[deploy] mode` 决定，不再无条件等于 `module_code`：

| 部署模式 | `[deploy] mode` | `get_app_id()` 返回 | `[app]` 块 | 数据源加载 |
|---|---|---|---|---|
| **单体（默认）** | `mono` | 固定 `"default"`（不读 `[app].module_code`） | 整体不生效 | 加载全部 `status=1/archived=0` 的记录 |
| **微服务** | `micro` | `[app].module_code`（维持原查找顺序） | 必需，不能为 `default` | 按 `[app]` 三元组精确过滤 |

**历史约束（仅适用于 micro 模式）**：`app_id ≡ module_code`。证据：

1. **配置同源**：`ConfigManager::global().get_app_id()`（`cmx-utils/src/config/config_impl.rs`）micro 模式下第一优先来源是配置键 `app.module_code`。
2. **导入强制相等**：`ModuleInstallService::install_module_package`（`cmx-plugin/src/service/module_install.rs`）micro 模式下校验 `manifest.module.code == get_app_id()`，不一致则拒绝导入；mono 模式下放宽此守卫。
3. **导出/查询冗余**：`cmx_plugin`、`cmx_meta_table_define` 等表同时带 `app_id` 和 `module_code` 列，micro 下二者值相同，SQL 中 `WHERE module_code = $1 AND app_id = $2` 是冗余的；mono 下所有 `app_id='default'`。

### 6.2 AI 开发规则

1. **禁止硬编码 `"default"` 作为 app_id 兜底**。必须使用 `cmx_utils::ConfigManager::global().get_app_id()` 取配置值。该方法内部按 `DeployMode::from_config()` 分支：mono 模式固定返回 `"default"`，micro 模式读 `[app].module_code`。
2. **不要把 `application_code` 当作 `app_id` 传参**。`application_code`（应用编码）与 `app_id`（隔离标识）是不同概念，尽管当前值可能相同。
3. **携带 `module_code` 的表**（`cmx_plugin`、`cmx_meta_table_define`、`cmx_meta_table_define_version`、`cmx_service_define`）：`app_id` 列功能冗余，但**保留**（为未来多租户演进预留），查询时优先用 `module_code` 过滤。
4. **不带 `module_code` 的表**（`cmx_plugin_versions`、`cmx_audit_log`、`cmx_model_*`）：`app_id` 是唯一隔离键，**必须**带上过滤。
   - **mono 模式下**：所有表数据 `app_id='default'`（全局共享），过滤自动命中全部数据。
   - **micro 模式下**：`app_id` 按模块隔离，是真正的过滤维度。
5. `cmx_permission` 表使用 `app_code` 列（命名与全局 `app_id`/`application_code` 不一致），注意区分。
6. **新增部署模式相关逻辑**：若需根据部署模式分支，使用 `cmx_utils::config::DeployMode::from_config()` 读取，禁止靠 `app_id == "default"` 隐式判断模式。

---

## 七、Service 列表 / 分页查询契约

> 来源技能：`axum-handler-generator` + `cmx-sql-execution`；同时是 `project_memory` 的硬约束。

- Service 的 `list` / `page` 方法**必须**接收 `filters: Option<Vec<XxxFilter>>` + `list_options: ListOptions` 两个结构化参数，**禁止**平铺 `(page, page_size, keyword, ...)`。
- utoipa 的 `#[utoipa::path(request_body = ...)]` **必须**用 `ListParams<serde_json::Value>` / `PageParams<serde_json::Value>`（`FilterNodes` 不支持 `ToSchema`）；**但函数签名必须用具体 Filter 类型**（`Json<ListParams<XxxFilter>>`）——`Value` 仅用于注解，禁止扩散到签名。
- Handler 端三步提取（`to_list_options()` / `get_page()` / `get_size()`）、多表 JOIN 的 `#[modql(rel = "表别名")]`、多租户 `app_id` 注入位置等实现细节见技能。
- 编写 list / page 相关代码**前**先调上述技能。

---

## 八、cmx-*-api Handler 规范

> 来源技能：`axum-handler-generator`。

### 8.1 职责边界（理想目标）

**各 `*-api` crate（cmx-apis/ 下的 cmx-common-api / cmx-biz-api / cmx-iam-api 等）应保持为纯 HTTP 适配层**：Entity / BMC / Filter / Service 归业务 crate，`*-api` 通过 `use` 引用，**禁止**重新定义；跨 crate 共享 DTO 下沉到 `cmx-core/src/model/`。

> ⚠️ 现状有违规渗入（`portal/model_center`、`auth/api_key` 等 handler 内手写 SQL），列为 backlog，新增代码不得沿用。

### 8.2 两种 Service 模式

| 模式 | 调用方式 | 典型 crate |
|------|---------|-----------|
| 静态 | `XxxService::create(mm, db_id, ...)` | cmx-biz |
| 注入式 | `cmx_state.<业务>()?.<子>_service.create_<子>(...)` | cmx-iam |

### 8.3 硬约束

- 除 `get_by_id`（GET）外，CRUD 一律 **POST + application/json**；每个操作独立路径，**禁止**共享路径。
- `ForCreate` 不含 `id` / `create_time` / `update_time`；`ForUpdate` 全 `Option`。
- `declare_crud_handlers!` 宏（`cmx_api_core::routes::macros`）**仅限各 `*-api` crate 内部使用**。
- 所有 handler 模块实现 `ModuleRoutes` trait（`cmx_api_core::routes::traits`）。
- 编写 handler / Service **前**先调 `axum-handler-generator`。

---

## 九、Entity / Filter / BMC 定义规范

> 来源技能：`modql`（Filter / Fields）+ `axum-handler-generator`（Entity）。

- **Entity 必须** `#[derive(modql::field::Fields)]`（GenericCrudService 依赖它构建 SQL，不可省略）。
- **Filter 必须** `#[derive(modql::filter::FilterNodes)]`；字段用 `Option<OpValsXxx>`，**禁止**原始 `String` / `i64`（列类型 → OpVals 映射查技能）。
- 多表 JOIN 时字段加 `#[modql(rel = "表别名")]`。
- BMC 实现 `DbBmc` trait（`cmx_database::crud`），提供 `TABLE` / `PK_COLUMN` / `has_timestamps()` / `has_owner_id()`。
- 设计 Entity / Filter / BMC **前**先调 `modql` 技能。

---

## 十、SQL 执行规范

> 来源技能：`cmx-sql-execution`。API 实际是 `cmx_database::DatabaseManager`（cmx-database-pg 镜像）的方法；`ParamsBuilder` / `DataValue` / `dv!` 在 cmx-core。

- 新代码**必须**用 `execute_sql_with_datavalues` / `query_sql_with_datavalues`；**禁止** `execute_sql_with_json`（整型 NULL 退化，仅维护旧代码时保留）。
- 参数构造优先 `cmx_core::dv!` 宏或 `.into()` 糖；**禁止** `.map(DataValue::X).unwrap_or(DataValue::Null)`（冗长且丢类型）。
- 动态 UPDATE **必须**用 `cmx_core::ParamsBuilder`；**禁止**手动 `format!("$1, $2, ...")` 拼 idx。
- **None→0 vs None→NULL 必须逐处核对**：`unwrap_or(0).into()` 表示 0，`.into()` 表示 `NullTyped`；不盲目改（注：`NullTyped` 仅针对整型/时间/Uuid 等非字符串列）。
- 事务内传 `txn_id: Some(&txn_id)`，非事务 `None`。
- Service 层手写 SQL **前**先调 `cmx-sql-execution`。

---

## 十一、WASM 插件开发规范

> 来源技能：`wasm-plugin-developer` + `plugin-fn-doc` + `service-orchestration-generator`。标准范例：`crates/libs/cmx-plugin-demo`。

- **plugin_id 只能用下划线 `_`**，禁止连字符 `-`（如 `cmx_account`）。
- 目录分 `models/` / `handlers/`（泛型 `H: HostFunctions`）/ `extism/`（薄适配层），handlers **禁止**感知 Extism；完整结构查技能。
- 数据库参数 `data_values` 优先于 `params`（JSON），防整型 NULL 跨边界退化。
- `#[plugin_fn]` 函数摘要**不以句号结尾**（cmx-cli 解析需要，是第十三章的显式例外）；字段类型用 JSON Schema 类型（`string`/`integer`/...），**禁止** Rust 类型。
- 编排图（5 种节点）、事务框与 edges 关系等见 `service-orchestration-generator`。
- 编写插件 / 函数注释 / 编排**前**先调上述技能。

---

## 十二、插件元数据与种子数据规范

> 来源技能：`plugin-metadata-generator`。

- 插件通过 `config/{name}_config.json`（入口）注册 `metadata/*_tables.json`（表定义）+ `seeddata/*_seed.json`（种子数据），加载链：`manifest.json → table_config_files → config → 建表 + 插数据`。
- 多配置按 `depends_on`（拓扑）+ `priority`（同级）排序，**先建表后插数据**。
- metadata：`ordinal` 从 1 连续不跳跃，`db_type` 匹配 `field_type`。
- 种子数据 `conflict_columns` 选业务唯一列生成 UPSERT；**失败不阻断插件安装**。
- 创建 / 修改插件表定义、种子数据、config 配置前先调技能。

---

## 十三、Rust 注释规范

> 来源技能：`rust-comment-convention`。

- `///` 用于项文档，`//!` 仅模块/crate 级（放 `mod.rs` / `lib.rs` 顶部），`//` 用于函数体；**禁止** `////`。
- 文档摘要**必须以句号结尾** + 第三人称单数现在时；标准章节标题 `# Arguments` / `# Returns` / `# Examples`（**复数**）/ `# Panics` / `# Errors` / `# Safety`。
- `pub fn` **必须**含 `# Arguments` 和 `# Returns`。
- `// TODO:` / `// FIXME:` / `// HACK:` / `// SAFETY:` 用行注释，**禁止**出现在 `///` 中。
- **例外**：`#[plugin_fn]` 函数摘要不句号（见第十一章），其余一律遵守。
- 编写或审查注释前先调技能。

---

## 十四、cmx-core 依赖约束

> `cmx-core` 作为零业务基础层（被所有上层 crate + WASM 传递依赖），**必须**保持轻量。下表基于真实 `Cargo.toml` 核实。

- ✅ 允许且已用：`serde` / `serde_json` / `utoipa`(optional) / `chrono` / `uuid` / `thiserror` / **`modql`**（`ListOptions` 来源）。
- ❌ 禁止：`sea-query` / `axum` / `cmx-database` / 业务 crate（cmx-biz / cmx-iam 等）/ 重量级二进制依赖。
- 修改 cmx-core 依赖前**必须**核对真实 `Cargo.toml`。

---

## 十五、方案与文档规范

> 来源技能：`doc-generator` + 根目录 `../.agents/skills/plan-naming`（方案命名规范唯一真源，本仓不再存副本）。

- `/plan` 方案文档命名：`documents/<yyyyMMdd>[_<模块名>]_<中文标题>.md`（日期 6/8 位，标题**必须**中文）。
- 为 crate 生成 README → `doc-generator`（至少含简介 + 模块树 + **≥5 个使用场景** + 错误处理 + FAQ）。

---

## 十六、代码质量工具

> 来源技能：`clippy-fix` + `rust-arch-review`。

- `cargo clippy` **排除**三类告警：`too_many_arguments` / `unused_variables` / `unused_functions`；其余按 auto-fix → 简单 → 中等 → 重构四阶段处理，流程调 `clippy-fix`。
- 架构审查**五维度**（Crate 划分 / Trait 解耦 / 依赖管理 / 错误处理 / 异步模式）调 `rust-arch-review`，输出 `documents/rust-arch-review-YYYY-MM-DD.md`。

---

## 十七、全局初始化约束

所有 `init()` / `initialize()` / `setup_*()` 函数**必须**返回 `Result<()>`，**禁止**使用 `panic!` / `expect` / `unwrap`。错误通过 `thiserror` 定义，向上传播。

> 与第一章 1.4（禁裸 unwrap）配合：1.4 管所有代码路径的 unwrap，本章专门约束初始化函数的错误传播方式。

---

## 十八、旧接口/旧代码标记（禁止参考）

### 18.1 目录标记

以下目录对应的接口和代码为**旧实现**，基于 JSON 文件存储，不走数据库。开发过程中**禁止参考**这些代码，新增功能应基于数据库的新接口（`/api/dct/*`、`/api/doc/*` 等）：

| 目录 | 说明 | 对应旧接口 |
|------|------|-----------|
| `data/dict/` | 字典数据 JSON 文件存储（registry.json + entries/*.json） | `/api/dict/*`（cmx-model/src/dict/） |
| `data/fact/` | 业务凭证事实数据 JSON 文件存储 | 旧凭证接口 |
| `data/form-pages/` | 表单页面定义 JSON 文件存储 | 旧表单接口 |

### 18.2 开发约束

1. **新增字典功能** → 走 `/api/dct/*`（[cmx-dct-api](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-dct/cmx-dct-api/src/handlers.rs)），直读/写 PostgreSQL 表。
2. **禁止参考** `cmx-model/src/dict/` 下的 `schema.rs`、`repo.rs`、`api.rs`、`write.rs` 等文件存储代码。
3. **禁止参考** `data/dict/`、`data/fact/`、`data/form-pages/` 目录下的 JSON 文件结构。
4. 旧代码仅做**维护兼容**使用，新增功能不得沿用其模式。

---

## 十九、RPC 皮肤规范（cmx-rpcs/）

> 对标第八章 HTTP 层（`cmx-apis/*`）的三段式：**契约中心化 · 实现归域 · 装配显式**。

### 19.1 三层职责（禁止越界）

| 层 | crate | 职责 | 禁止 |
|----|-------|------|------|
| proto 契约 | `cmx-rpc-gen`（`idl/<域>/*.proto` + volo.yml + 别名模块） | 集中管理全部 proto、生成类型重导出 | 放任何运行时代码 |
| 基础设施 | `cmx-infra/cmx-rpc` | Bundle trait / GrpcInfrastructure / with_retry / apply_auth_metadata / AuthVerifier / factory / server_runner / GlobalRpcClient | **禁止出现任何具体服务的 client/server impl**（皮肤已全部迁出） |
| 皮肤 | `cmx-rpcs/cmx-<域>-rpc` | client 访问器 + server impl + Bundle（src/{lib,client,server}.rs） | **禁止依赖业务 service crate**（cmx-biz 等）——业务实现经 `ServerDeps` 由组装层注入 |

### 19.2 硬约束

- **主应用提供哪些 gRPC 服务 = cmx-platform-app `run_platform` 的 `rpc_bundles` 列表**——增删一行即增删服务，cmx-rpc 与皮肤 crate 零改动。裁剪部署形态（精简版/独立微服务）只改该列表。
- 新增 gRPC 服务按 `cmx-rpc/README.md` 的 SOP 九步执行（proto → volo.yml → 别名 → 新建皮肤 → workspace 注册 → 组装层注册 → 消费方调用）。
- 生成类型引用一律走便捷别名 `cmx_rpc_gen::orchestrator_proto::*` / `resource_data_proto::*`，不写四层深路径。
- 消费方调用访问器前**必须**先 `cmx_rpc::GlobalRpcClient::is_initialized()` 守卫（未初始化访问器直接 panic）。
- `with_retry` 闭包只返回原始 `volo_grpc::Status`，`into_inner`/proto 转换在重试返回后做一次。
- 配置：`[rpc]`（enabled/protocol/grpc.port/timeout/retry/warmup_services）与 `[service_auth].outgoing_api_key`，皮肤与基础设施 crate 均不读配置（装配层 cmx-service-base 统一读）。

### 19.3 关键路径

```
crates/libs/cmx-rpc-gen/                  # proto 契约（idl/orchestrator/、idl/resource/）
crates/libs/cmx-infra/cmx-rpc/            # RPC 基础设施（纯共享设施）
crates/libs/cmx-rpcs/cmx-orchestrator-rpc/  # 编排皮肤（OrchestratorBundle + orchestrator_client()）
crates/libs/cmx-rpcs/cmx-resource-rpc/      # 资源导入皮肤（ResourceDataBundle + resource_data_client()）
cmx-platform-app/src/lib.rs               # ★ rpc_bundles 显式装配点
```
