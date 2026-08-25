# 常见反模式与项目内真实案例（Anti-Patterns）

> 适用范围：rust-arch-review 技能全量审查。
> 案例来源：参考 2026-07-03 [cmx-plugin 安装升级与模块导入导出代码复用评审报告](../../../documents/20260703_cmx-plugin_安装升级与模块导入导出代码复用评审报告.md) 等项目内真实评审记录。
> 维护规则：发现新反模式时追加到对应类别；记录「典型症状 + 真实案例 + 修复方向」。

---

## 〇、如何使用本文件

1. **审查前**：快速浏览分类，建立反模式预警。
2. **审查中**：发现疑似症状时，定位到对应反模式条目，验证是否真命中。
3. **命中后**：直接套用「修复方向」段落的建议。

---

## 一、重复造轮子（Reinventing the Wheel）

### 1.1 权限树两阶段 upsert 三处平行实现

**典型症状**：
- 在 `INSERT INTO cmx_permission ... VALUES (... parent_id=NULL ...)` 后再 `UPDATE cmx_permission SET parent_id=?, full_code_path=?, level=?, is_leaf=? WHERE code=?`
- 出现"先插空 → 回填树形字段"的两阶段操作
- 多处手写 `full_code_path = parent_path || '/' || code` 字符串拼接

**项目内真实案例**：

| 位置 | 实现 | 风险 |
|------|------|------|
| `cmx-iam/src/permission/service/import.rs:171+` | `PermissionServiceImpl::import_permissions` 事务内 + diff（增/改/删） | 基准实现 |
| `cmx-iam/src/permission/service/crud.rs` | 普通 CRUD 路径计算 `full_code_path`/`level`/`parent_code` | 维护漂移 |
| `cmx-plugin/src/service/module_install.rs:532-733` | 手写 SQL 两阶段 upsert | ❌ 无事务、无 diff、无删除清理 |

**修复方向**：
- `module_install.rs` 改为调 `PermissionServiceImpl::import_permissions`（注入式）
- 抽取 `PermissionDefinition` 结构体到 `cmx-traits` 或独立 crate，消除 3 份副本
- 改造 `cmx-iam` 的 `import_permissions` 接受 `Vec<PermissionDefinition>` 而非 zip 输入

**审查锚点**：

```bash
grep -rn "parent_id = NULL" crates/libs/
grep -rn "full_code_path" crates/libs/
```

---

### 1.2 DDL + 元数据保存双路径

**典型症状**：
- "执行 DDL" 和 "保存 `cmx_meta_table_define` 记录" 出现在两套独立代码中
- 路径 A（插件）已失效但保留死代码；路径 B（模块）在用但有重复

**项目内真实案例**：

| 路径 | 入口 | 状态 |
|------|------|------|
| 旧（插件路径） | `utils.rs::execute_ddl_with_lock` → `create_plugin_tables` → `save_plugin_table_metadata` | ❌ 已失效（无活调用方） |
| 新（模块路径） | `module_install.rs:451-525 install_metadata` + `save_table_metadata:739-840` | ✅ 在用 |

**修复方向**：
- 删除旧 `utils.rs::execute_ddl_with_lock` / `create_plugin_tables` / `save_plugin_table_metadata`（`utils.rs:300-374` 起）
- 新路径通过 `PgTableDefineExecutor::new(biz_db_id, None)` 复用执行器
- `cmx_meta_table_define` upsert 统一走 `TableMetadataService` 抽象

**审查锚点**：

```bash
grep -rn "save_table_metadata\|save_plugin_table_metadata" crates/libs/
grep -rn "PgTableDefineExecutor" crates/libs/
```

---

### 1.3 80% 重复的 install/upgrade persist

**典型症状**：
- `install_persist` 和 `upgrade_persist` 主体逻辑相同，差异仅在前置校验
- 各自实现：fetch 包、extract、安全校验、解析元数据、依赖检查、目录创建、文件拷贝、事务、seed、upsert、版本历史

**项目内真实案例**：`cmx-plugin/src/service/persistence.rs:116-382`（install） vs `389-628`（upgrade）

**修复方向**：
- 抽取 `persist_common(PersistContext)` helper
- install/upgrade 只传入差异化的"前置校验闭包"和"是否记录 old_version"

**审查锚点**：

```bash
# 比较两函数行数
wc -l crates/libs/cmx-plugin/src/service/persistence.rs

# 找 80% 重复对
grep -c "fn install_persist\|fn upgrade_persist" crates/libs/cmx-plugin/src/service/persistence.rs
```

---

### 1.4 `delete_by_code` 在 form/menu 各写一份

**典型症状**：
- `delete_by_code` 函数在 `form/service.rs` 和 `menu/service.rs` 结构完全一致
- 期望提升为 `GenericCrudService::delete_by_field` 泛型 helper

**修复方向**：
- 提升到 `GenericCrudService::delete_by_field<T, F>(field_name, field_value)`
- 或抽 trait `DeletableByCode { fn delete_by_code(mm, db_id, code) -> Result<()> }`

---

### 1.5 JSONB "string-or-object" 强制转换重复 ~6 处

**典型症状**：
- `serde_json::Value` 可能是字符串或对象，统一用 `match` 强制转对象
- 出现 4-6 次类似代码

**项目内真实案例**：export forms / menus / metadata / menu tree

**修复方向**：抽 `cmx_utils::jsonb::coerce_to_object(Value)`

---

### 1.6 14 位时间戳格式 `%Y%m%d%H%M%S` 重复 3 处

**典型症状**：
- 模块版本号用 14 位时间戳（`yyyyMMddHHmmSS`）
- 多处手写 `chrono::Local::now().format("%Y%m%d%H%M%S").to_string()`

**修复方向**：抽常量 `MODULE_PACKAGE_VERSION_FORMAT` + 工具函数 `now_module_version()`

---

### 1.7 临时目录 + 时间戳格式重复 3 处

**典型症状**：
- `tempdir::TempDir::new("prefix_")` + 时间戳
- 出现位置：`module_export.rs:51-55` / `module_install.rs:190-194` / `migrate_to_module_packages.rs:239`

**修复方向**：收敛到 `PackageUtils::new_temp_dir(prefix)`

---

### 1.8 `install_forms` 与 `install_menus` 几乎逐行相同

**典型症状**：
- `module_install.rs:306-370`（forms）vs `376-444`（menus）几乎一致
- 读目录 → `.json` → `{module}:{stem}` code → delete_by_code → create

**修复方向**：泛型化 `install_definition_files<T: Serialize + Deserialize>(...)`

---

### 1.9 PermissionDefinition 契约 3 份副本

**典型症状**：
- 同一业务对象（8 字段）在 3 处独立定义
- 字段重命名/增删时 3 处必须手工同步

**项目内真实案例**：
- `cmx-iam/src/permission/service/import.rs:27-50`（规范结构体）
- `module_export.rs:395-404`（`serde_json::json!({...8 字段...})`）
- `module_install.rs:554-569`（私有内联结构体 `PermDef`）

**修复方向**：
- 抽到 `cmx-traits::resource::PermissionDefinition` 公共结构体
- 三处统一引用

---

## 二、违反依赖倒置（Inversion of Control Violation）

### 2.1 业务逻辑强依赖具体实现 crate

**典型症状**：
- `cmx-service` 直接 `use cmx_plugin::*`
- `cmx-api` 直接 `use cmx_iam::*`
- 上层通过具体类型耦合下层实现

**修复方向**：
- 在 `cmx-traits` 定义抽象 trait
- 业务层仅 `use cmx_traits::*`
- 实现 crate 在 `web-server` 注入

**审查锚点**：

```bash
grep -rn "use cmx_plugin::" crates/libs/cmx-service/src
grep -rn "use cmx_iam::" crates/libs/cmx-api/src
```

---

### 2.2 cmx-api 中手写 SQL 而非调 Service

**典型症状**：
- Handler 内直接 `database.execute_sql("SELECT * FROM ...")`
- 绕过 `cmx-biz` 中已实现的 Service
- 导致业务逻辑分散在 Handler

**修复方向**：
- 业务逻辑下沉到 `cmx-biz/src/<entity>/service.rs`
- Handler 仅 `cmx_biz::XxxService::method(mm, db_id, ...)`

**审查锚点**：

```bash
grep -rn "execute_sql\|query_sql" crates/libs/cmx-api/src/handlers/
```

---

### 2.3 cmx-api 中重定义业务 Entity

**典型症状**：
- `crates/libs/cmx-api/src/handlers/xxx/model.rs` 中有 `pub struct XxxEntity`
- 而 `cmx-biz/src/xxx/entity.rs` 已有同名 Entity

**修复方向**：
- 删除 cmx-api 中的 Entity
- 改 `use cmx_biz::xxx::entity::XxxEntity`

**审查锚点**：

```bash
grep -rn "pub struct.*Entity" crates/libs/cmx-api/src/handlers/
```

---

## 三、错误处理反模式

### 3.1 手动 `impl Error` / `impl Display`

**典型症状**：

```rust
// ❌ 反模式
impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "...")
    }
}
impl std::error::Error for MyError {}
```

**修复方向**：用 `thiserror` 派生宏。

```rust
// ✅ 正确
#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error("操作失败: {0}")]
    OperationFailed(String),
}
```

**审查锚点**：

```bash
grep -rn "impl.*Error for\|impl Display for" crates/libs/ --include="*.rs"
```

---

### 3.2 使用 `derive_more::From` 与 thiserror 冲突

**典型症状**：

```rust
// ❌ 反模式
#[derive(thiserror::Error, derive_more::From, Debug)]
pub enum MyError { ... }
```

**修复方向**：用 thiserror 的 `#[from]` 属性。

```rust
// ✅ 正确
#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

---

### 3.3 裸 `unwrap()` 在生产代码

**典型症状**：

```rust
// ❌ 反模式（生产代码）
let value = option.unwrap();
let result = func().unwrap();
```

**修复方向**：

```rust
// ✅ 正确：首选 ?
let value = option.ok_or(MyError::NotFound)?;

// ✅ 兜底：expect("有意义提示")
let value = global.expect("Redis 初始化失败，请检查配置");
```

**审查锚点**（排除 `tests/` 和 `#[cfg(test)]`）：

```bash
grep -rn "\.unwrap()" crates/libs/ --include="*.rs" | grep -v "/tests/" | grep -v "#\[cfg(test)\]"
```

---

### 3.4 init 函数 panic / unwrap

**典型症状**：

```rust
// ❌ 反模式
pub fn init_runtime() {
    let pool = PgPool::connect(&url).unwrap();  // 启动 panic
}
```

**修复方向**：

```rust
// ✅ 正确
pub fn init_runtime() -> Result<()> {
    let pool = PgPool::connect(&url)
        .map_err(|e| MyError::Init(format!("DB 连接失败: {e}")))?;
    Ok(())
}
```

**审查锚点**（来源：[AGENTS.md §17](../../../AGENTS.md)）：

```bash
grep -rn "fn init(" crates/libs/ --include="*.rs" -A 3
```

---

### 3.5 跨模块错误直接暴露

**典型症状**：

```rust
// ❌ 反模式：cmx-service 的 Error 直接暴露给 cmx-api
pub enum ServiceError {
    IamError(cmx_iam::IamError),  // 跨 crate 错误泄露
}
```

**修复方向**：

```rust
// ✅ 正确：顶层 Error 统一转换
#[derive(thiserror::Error, Debug)]
pub enum ServiceError {
    #[error(transparent)]
    Iam(#[from] cmx_iam::IamError),
}
```

---

## 四、并发与异步反模式

### 4.1 滥用 `Arc<Mutex<...>>`

**典型症状**：
- 读多写少场景用 `Mutex` 而非 `RwLock`
- 高并发共享状态用 `Mutex` 阻塞

**修复方向**：
- 读多写少：`Arc<RwLock<T>>`
- 短任务：用 `dashmap` 等并发容器
- 跨 await：用 channel（`mpsc` / `tokio::sync::broadcast`）

---

### 4.2 异步函数内 `block_in_place`

**典型症状**：

```rust
// ❌ 滥用
async fn handler() {
    tokio::task::block_in_place(|| {
        // 同步长任务
    });
}
```

**修复方向**：
- 用 `tokio::task::spawn_blocking` 把 CPU 密集任务移出运行时
- 用 `rayon::spawn` 处理并行 CPU 任务

---

### 4.3 异步资源未正确释放

**典型症状**：
- `let conn = pool.get().await?;` 后未确保归还
- `tokio::spawn` 后未 `.await` JoinHandle，错误吞掉

**修复方向**：
- 用 RAII（Drop）确保资源归还
- 重要 JoinHandle 必 `.await` 或显式 `.abort()`

---

## 五、API 设计反模式

### 5.1 过度暴露 `pub`

**典型症状**：
- struct 字段全 `pub`
- 模块内辅助 fn 也 `pub`
- 内部实现细节对外可见

**修复方向**：
- struct 字段 `pub(crate)`，构造器或 builder 暴露
- 内部 fn `pub(crate)` 或 `pub(super)`
- 仅对外 API 用 `pub`

**审查锚点**：

```bash
# pub 占比
find crates/libs/cmx-biz/src/ -name "*.rs" -exec grep -c "^\s*pub " {} \; | awk '{s+=$1; n++} END {print "pub 占比: " s/n}'
```

---

### 5.2 `pub fn` 缺文档注释

**典型症状**：

```rust
// ❌ 反模式
pub fn calculate_tax(amount: f64, rate: f64) -> f64 { ... }
```

**修复方向**（来源：[AGENTS.md §13](../../../AGENTS.md)）：

```rust
// ✅ 正确
/// 计算含税金额。
///
/// # Arguments
///
/// * `amount` - 不含税金额（元）。
/// * `rate` - 税率（如 0.13 表示 13%）。
///
/// # Returns
///
/// 含税金额（元）。
///
/// # Examples
///
/// ```
/// let total = calculate_tax(100.0, 0.13);
/// assert_eq!(total, 113.0);
/// ```
pub fn calculate_tax(amount: f64, rate: f64) -> f64 { ... }
```

**审查锚点**：

```bash
total=$(grep -rn "^\s*pub fn\b" crates/libs/cmx-biz/src/ | wc -l)
doced=$(grep -B1 "^\s*pub fn\b" crates/libs/cmx-biz/src/ -r | grep "///" | wc -l)
echo "覆盖率: $doced / $total"
```

---

### 5.3 文档摘要不以句号结尾

**典型症状**：

```rust
/// 计算含税金额       ← ❌ 缺句号
pub fn calculate_tax(...) { ... }
```

**修复方向**：

```rust
/// 计算含税金额。     ← ✅
pub fn calculate_tax(...) { ... }
```

---

### 5.4 `TODO` / `FIXME` 在 `///` 中

**典型症状**：

```rust
/// TODO: 重构此函数
pub fn foo() { ... }
```

**修复方向**：

```rust
// TODO: 重构此函数
pub fn foo() { ... }
```

---

### 5.5 块注释（`/* */`）代替行注释

**典型症状**：

```rust
/*
 * 计算含税金额
 */
pub fn calculate_tax(...) { ... }
```

**修复方向**：

```rust
// 计算含税金额。
pub fn calculate_tax(...) { ... }
```

---

## 六、依赖管理反模式

### 6.1 子 crate 硬编码依赖版本

**典型症状**：

```toml
# ❌ 反模式（cmx-biz/Cargo.toml）
[dependencies]
serde = "1.0"   # 应改为 serde = { workspace = true }
```

**修复方向**：

```toml
# ✅ 正确
[dependencies]
# 序列化框架
serde = { workspace = true }
```

**审查锚点**：

```bash
for f in crates/libs/*/Cargo.toml crates/libs/cmx-infra/*/Cargo.toml; do
  grep -E "^[a-z\-]+ = \"[0-9]" "$f" 2>/dev/null
done
```

---

### 6.2 使用 `log` crate

**典型症状**：

```toml
log = "0.4"
```

或

```rust
use log::info;
log::info!("...");
```

**修复方向**：用 `tracing`。

```toml
tracing = { workspace = true }
```

```rust
use tracing::info;
info!("...", key = value);
```

**审查锚点**（来源：[AGENTS.md §3.4](../../../AGENTS.md)）：

```bash
grep -rn "^log = " crates/libs/*/Cargo.toml crates/libs/cmx-infra/*/Cargo.toml
grep -rn "use log::" crates/libs/ --include="*.rs"
```

---

### 6.3 依赖无注释 / 分组注释

**典型症状**：

```toml
# ❌ 反模式：无注释
serde = { workspace = true }
anyhow = { workspace = true }
```

或

```toml
# ❌ 分组注释
# ===== 序列化 =====
serde = { workspace = true }
```

**修复方向**：

```toml
# ✅ 正确：每个依赖单独注释
# 序列化框架
serde = { workspace = true }
# 错误处理
anyhow = { workspace = true }
```

---

## 七、SQL 与数据库反模式

### 7.1 字符串拼接动态 SQL

**典型症状**：

```rust
// ❌ 反模式
let sql = format!("SELECT * FROM users WHERE id = {} AND status = '{}'", id, status);
```

**修复方向**：

```rust
// ✅ 正确：参数化
let sql = "SELECT * FROM users WHERE id = $1 AND status = $2";
query_sql_with_datavalues(mm, db_id, None, sql, dv![id, status]).await?;
```

---

### 7.2 动态 UPDATE 手写 `$N`

**典型症状**：

```rust
// ❌ 反模式
let mut set_clauses = Vec::new();
let mut params = Vec::new();
if let Some(name) = &req.name {
    params.push(name.clone().into());
    set_clauses.push(format!("name = ${}", params.len()));
}
if let Some(email) = &req.email {
    params.push(email.clone().into());
    set_clauses.push(format!("email = ${}", params.len()));
}
let sql = format!("UPDATE users SET {} WHERE id = $1", set_clauses.join(", "));
```

**修复方向**（来源：[AGENTS.md §10](../../../AGENTS.md)）：

```rust
// ✅ 正确：用 ParamsBuilder
let mut b = ParamsBuilder::new(1);  // WHERE id = $1 已占
b.set_opt("name", req.name.clone());
b.set_opt("email", req.email.clone());
let (set_clause, params) = b.build();
let sql = format!("UPDATE users SET {set_clause} WHERE id = $1");
```

---

### 7.3 用 `execute_sql_with_json`（整型 NULL 退化）

**典型症状**：

```rust
execute_sql_with_json(mm, db_id, None, sql, json!({ "id": id, "sort": null })).await?;
```

**修复方向**（来源：[AGENTS.md §10](../../../AGENTS.md)）：

```rust
execute_sql_with_datavalues(
    mm, db_id, None, sql,
    dv![id, null Int],  // 用 NullTyped 携带类型
).await?;
```

---

### 7.4 事务内不传 `txn_id`

**典型症状**：

```rust
// ❌ 反模式
query_sql(mm, db_id, None, sql, params).await?;  // 事务内不传 txn_id
```

**修复方向**：

```rust
// ✅ 正确
let txn_id = ctx.begin_txn().await?;
query_sql(mm, db_id, Some(&txn_id), sql, params).await?;
ctx.commit_txn(&txn_id).await?;
```

---

### 7.5 滥用 cmx-database-pg 替代 cmx-database

**典型症状**：

```rust
// ❌ 反模式：非独占能力场景引入 cmx-database-pg
use cmx_database_pg::get_default_pg_db_manager;
let mm = get_default_pg_db_manager();
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

**根因**：cmx-database-pg 是 PG-only 性能优化分支，API 与 cmx-database 高度对齐，但**无任何 crate 独家依赖它**（4 个消费方同时挂着 cmx-database）。非独占能力场景引入它徒增依赖复杂度。

**cmx-database-pg 真正独有的 4 项能力**（其余 API 两 crate 完全对齐）：

| # | 独有能力 | 位置 |
|---|---------|------|
| ① | `query_sql_zmc_stream_chunks`（mpsc 分帧流式，峰值内存 O(单行)） | `manager/mod.rs:374` |
| ② | 数组列读取还原（TEXT_ARRAY / INT8_ARRAY / UUID_ARRAY -> DataValue::Array） | `executor/mod.rs:435-452` |
| ③ | `get_conn()`（返回 `deadpool_postgres::Object`，供事务层手动驱动） | `connection/mod.rs:112` |
| ④ | 4 个 ToSql 适配器（PgInt / PgDateTime / PgDateTimeNull / PgIntNull） | `executor/mod.rs:24-123` |

> ⚠️ **注意区分**：`query_zmc_streaming`（写入 Vec<u8>）**两者都有**，不是独占；唯独 `*_stream_chunks`（mpsc 通道）是 pg 独有。

**修复方向**：

```rust
// ✅ 正确：默认用 cmx-database
use cmx_database::get_default_db_manager;
let mm = get_default_db_manager();
mm.execute_sql_with_datavalues(&db_id, None, sql, params).await?;
```

**替换指南**：

🟢 **可以无痛替换**（占大多数场景）：
- 只用到 `execute_sql*` / `query_sql*` / `query_sql_zmc` / `query_sql_zmc_with_datavalues` / `crud::*` / `transaction::*` / `migration::*` / `host_functions` / `ZmcDataSet` 的消费方
- 注意 `SqlParams::SeaValues` -> `SqlxValues` 枚举变体替换
- 具体使用点：cmx-api 的 `dct.rs`/`doc.rs`、web-server 的 `datasource.rs`、cmx-biz 的 `zmc_loader.rs`（cmx-biz 里甚至已有并行的 `zmc_loader_sqlx.rs` 佐证）

🔴 **不能简单替换**（需迁移实现）：
- 依赖 `query_sql_zmc_stream_chunks` 的场景（如 `mem_bench.rs`、O(单行) 内存流式消费）
- 依赖数组列读取还原（`DataValue::Array` 从数据库读取）的场景
- 直接依赖 `TokioPgRowSource` 全路径的代码（如 `cmx-database-test` 的 `e2e_server.rs:338`、`mem_bench.rs`）需改为 `SqlxPgRowSource`

**审查锚点**：

```bash
# 检查非独占能力场景是否误用 cmx-database-pg
grep -rn "cmx_database_pg\|get_default_pg_db_manager" crates/libs/ | grep -v "stream_chunks\|zmc"

# 检查是否在用 TokioPgRowSource 全路径（可考虑改回 SqlxPgRowSource）
grep -rn "TokioPgRowSource" crates/libs/
```

---

### 7.6 在事务内调 query_sql_zmc（ZmcDataSet 不参与事务）

**典型症状**：

```rust
// ❌ 反模式：query_sql_zmc 是只读连接池路径，不走事务
let txn_id = mm.get_transaction_context().begin(&db_id).await?;
let zmc_ds = mm.query_sql_zmc_with_datavalues(&db_id, sql, params, "ds").await?;
// ⚠️ zmc_ds 不在事务内，读到的是其他连接的快照
mm.commit_transaction(&txn_id).await?;
```

**根因**：`query_sql_zmc*` 系列走连接池、只读、不参与事务；事务内查询应用 `query_sql_with_datavalues`。

**修复方向**：

```rust
// ✅ 正确：事务内用 query_sql_with_datavalues（返回 DataSet）
let ds = mm.query_sql_with_datavalues(&db_id, Some(&txn_id), sql, params, "ds").await?;
```

---

### 7.7 cmx-database-pg 中使用 with_json API

**典型症状**：

```rust
// ❌ 反模式：cmx-database-pg 的 with_json 同样不推荐
use cmx_database_pg::get_default_pg_db_manager;
let mm = get_default_pg_db_manager();
mm.query_sql_with_json(&db_id, None, sql, json!([id]), "ds").await?;
```

**修复方向**：两 crate 的 `with_json` 系列 API 均不推荐新代码使用。

```rust
// ✅ 正确：两 crate 均用 _with_datavalues
mm.query_sql_with_datavalues(&db_id, None, sql, dv![id], "ds").await?;
```

---

### 7.8 误以为 cmx-database 也能读取数组列

**典型症状**：

```rust
// ❌ 误解：以为 cmx-database 也能从数据库读取数组列还原为 DataValue::Array
// cmx-database（sqlx）的 ResultConverter 读取方向不还原数组
// 只在绑定时 bind_pg_array_postgres 支持写入数组
let ds = mm.query_sql_with_datavalues(&db_id, None,
    "SELECT tags FROM posts WHERE id = $1",
    dv![post_id], "post"
).await?;
// ⚠️ tags 列如果是 TEXT[]，cmx-database 读取阶段不会还原为 DataValue::Array
```

**修复方向**：如果需要从数据库读取数组列并还原为 `DataValue::Array`，必须使用 cmx-database-pg（其 `PgResultConverter::convert_rows` 支持 TEXT_ARRAY / INT8_ARRAY / UUID_ARRAY 读取还原）。

```rust
// ✅ 正确：数组列读取还原用 cmx-database-pg
use cmx_database_pg::get_default_pg_db_manager;
let mm = get_default_pg_db_manager();
let ds = mm.query_sql_with_datavalues(&db_id, None,
    "SELECT tags FROM posts WHERE id = $1",
    dv![post_id], "post"
).await?;
// ✅ tags 列还原为 DataValue::Array
```

> 如果只需要**写入**数组参数（`WHERE id = ANY($1)`），cmx-database 的 `bind_pg_array_postgres` 已支持，无需引入 cmx-database-pg。

---

## 八、命名与目录反模式

### 8.1 `plugin_id` 含连字符

**典型症状**：`plugin_id = "cmx-account"`（连字符）

**修复方向**（来源：[AGENTS.md §11](../../../AGENTS.md)）：用下划线 `plugin_id = "cmx_account"`

---

### 8.2 业务模块用 `cmx_` 前缀

**典型症状**：业务模块自建表用 `cmx_xxx` 前缀。

**修复方向**（来源：[AGENTS.md §5.4](../../../AGENTS.md)）：系统基础表用 `cmx_` 前缀，业务/插件自建表不强制加（由模块自行命名）。

---

### 8.3 表带外键约束

**典型症状**：

```sql
CREATE TABLE cmx_order (
    ...
    user_id BIGINT REFERENCES cmx_user(id),  -- ❌ 禁止外键
);
```

**修复方向**（来源：[AGENTS.md §5.4](../../../AGENTS.md)）：保留关联字段，用 `CREATE INDEX` 替代。

```sql
CREATE TABLE cmx_order (
    ...
    user_id BIGINT  -- 保留关联字段
);
CREATE INDEX idx_order_user_id ON cmx_order(user_id);
```

---

## 九、WASM 插件反模式

### 9.1 插件目录缺 `handlers/` / `extism/` 分离

**典型症状**：
- 所有业务逻辑写在 `lib.rs` 一个文件
- handlers 与 extism 混杂

**修复方向**（来源：[wasm-plugin-developer](../../wasm-plugin-developer/SKILL.md)）：
- `handlers/` 纯业务逻辑（`H: HostFunctions` 泛型）
- `extism/` 适配层（`impl HostFunctions for ExtismHost`）
- `models/` 业务模型

---

### 9.2 `#[plugin_fn]` 函数摘要以句号结尾

**典型症状**：

```rust
/// 查询账户信息。       ← ❌ plugin_fn 例外：不要句号
#[plugin_fn]
pub fn query_account(...) -> Result<...> { ... }
```

**修复方向**：

```rust
/// 查询账户信息         ← ✅ 不带句号
#[plugin_fn]
pub fn query_account(...) -> Result<...> { ... }
```

---

### 9.3 插件 `plugin_id` 不符合规范

**典型症状**：

```json
{ "plugin": { "id": "my-plugin" } }   ← ❌ 连字符
{ "plugin": { "id": "MyPlugin" } }    ← ❌ PascalCase
```

**修复方向**：`{ "plugin": { "id": "my_plugin" } }`（全小写下划线）

---

## 十、测试反模式

### 10.1 `#[ignore]` 测试长期遗留

**典型症状**：

```rust
#[test]
#[ignore = "TODO 修复"]    ← ❌ 长期遗留
fn test_foo() { ... }
```

**修复方向**：
- 立即修复或删除
- 真要推迟，关联 issue/tracker 编号

---

### 10.2 关键 Service 0 测试

**典型症状**：`cmx-biz/src/form/service.rs` 200 行无任何测试

**修复方向**：核心 CRUD / 权限校验 / 边界场景，至少 happy path + 1 error path。

---

## 十一、配置反模式

### 11.1 硬编码 `"default"` 作为 app_id

**典型症状**（来源：[AGENTS.md §6.1](../../../AGENTS.md)）：

```rust
// ❌ 反模式
let app_id = "default";
```

**修复方向**：

```rust
// ✅ 正确
let app_id = cmx_utils::ConfigManager::global().get_app_id();
```

---

### 11.2 直接 `std::env::var`

**典型症状**：

```rust
// ❌ 反模式
let host = std::env::var("DB_HOST").unwrap();
```

**修复方向**：

```rust
// ✅ 正确
let host = cmx_utils::ConfigManager::global().get_string("database.host")?;
```

---

## 十二、本文件维护规则

1. **发现新反模式**：在本文件对应分类下追加「症状 + 案例 + 修复方向」。
2. **更新反模式案例**：项目内已修复的案例，从"反例"移到"已修复"附录。
3. **定期 review**：每季度复盘本文件，删除已不适用的反模式。
