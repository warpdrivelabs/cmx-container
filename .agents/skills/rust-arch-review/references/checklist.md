# 4 大类 11 子维度检查清单（Checklist）

> 适用范围：rust-arch-review 技能全量审查。
> 使用方法：审查时按本清单逐项执行；每项均含「检查方法 / 通过标准 / 量化阈值 / 严重级别建议」。

---

## A. 宏观架构

### A1. Crate 划分

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| A1.1 | workspace 成员职责清晰 | 读 `Cargo.toml` `[workspace.members]` | 每个 crate 有单一职责 | 无职责重叠 crate | 🔴 严重（如多 crate 重复实现） |
| A1.2 | 依赖方向单向、无循环 | `cargo metadata --format-version=1` + 文本依赖图 | 基础层 → 业务层 → API 层 → 应用层 | 0 个循环依赖 | 🔴 严重 |
| A1.3 | cmx-core 保持零业务 | 读 `cmx-core/Cargo.toml` | 不依赖 `cmx-database` / `axum` / `sea-query` / 任何业务 crate | 0 个禁止依赖 | 🔴 严重 |
| A1.4 | cmx-core 包含必要基础类型 | 对照 `references/reuse-catalog.md` §1 | `dv!` / `ParamsBuilder` / `DataValue` 全部存在 | 全员存在 | 🟡 警告 |
| A1.5 | 新增 crate 有充分理由 | 人工评估 | 复用性 / 编译隔离 / 团队边界 三者满足其一 | 不为 1 个文件单建 crate | 🟡 警告 |
| A1.6 | 旧 crate 不应留死代码 | 读 `cmx-model/src/dict/` + `data/dict/` | 与新接口重复的旧实现应清理或显式标记 | 0 个未标记的重复实现 | 🟡 警告（[AGENTS.md §18](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) 硬约束） |
| A1.7 | 不应在 cmx-api 中重定义业务 Entity | Grep `pub struct.*Entity` under `cmx-api/src/handlers/` | 业务 Entity 仅在 cmx-biz / cmx-iam 定义 | 0 个重定义 | 🔴 严重（违反职责边界） |

**审查工具**：

```bash
# 列出 workspace 成员
cat Cargo.toml | grep -A 50 "^\[workspace\]"

# 检测循环依赖
cargo metadata --format-version=1 | jq '.packages[].dependencies[].name' | sort | uniq -c | sort -rn

# 检测 cmx-core 禁止依赖
grep -E "sea-query|axum|cmx-database|cmx-biz|cmx-iam" crates/libs/cmx-core/Cargo.toml

# 检测 cmx-api 业务 Entity 重定义
grep -rn "pub struct.*Entity" crates/libs/cmx-api/src/handlers/
```

---

### A2. 目录结构

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| A2.1 | mod 嵌套深度 ≤ 3 层 | Grep 路径分隔符 | `mod a { mod b { mod c { mod d } } }` 4 层即告警 | ≤ 3 层 | 🟡 警告 |
| A2.2 | 文件粒度合理 | 人工评估 | 单文件 < 800 行；超 800 行的考虑拆分 | 文件 ≤ 800 行 | 🔵 建议 |
| A2.3 | 可见性精确控制 | Grep `pub` | `pub(crate)` / `pub(super)` 优于 `pub` | `pub` 占比 < 30% | 🟡 警告 |
| A2.4 | `lib.rs` / `mod.rs` 不堆业务 | 读 `lib.rs` 长度 | `lib.rs` < 50 行（除 `pub use` 重导出） | < 50 行 | 🟡 警告 |
| A2.5 | 同类职责集中放置 | 读目录树 | 业务实体按 entity 拆子文件（如 `form/`, `menu/`, `application/`） | 跨目录重复 < 1 | 🟡 警告 |
| A2.6 | 命名遵循 Rust 约定 | Grep 文件名 / 类型名 | 模块 snake_case、类型 PascalCase、常量 SCREAMING_SNAKE_CASE | 0 个违例 | 🔵 建议 |
| A2.7 | 测试代码就近放置 | 读目录 | `src/foo.rs` 单元测试在 `mod tests`，集成测试在 `tests/` | 0 个 `src/test_*.rs` 散落 | 🔵 建议 |
| A2.8 | 不存在 `// xxx 开始` 风格的代码分区注释 | Grep 注释 | 反对 `// region: --- 业务代码` 大区块注释，应通过子模块拆分 | 0 个 `// region:` 注释 | 🔵 建议 |
| A2.9 | WASM 插件目录遵循标准结构 | 调 `wasm-plugin-developer` 技能 | `models/` / `handlers/` / `extism/` / `tests/` 完整 | 全员存在 | 🟡 警告 |

**审查工具**：

```bash
# mod 嵌套深度
grep -rn "^\s*mod \w" crates/libs/cmx-biz/src/ | awk -F'mod' '{print NF-1}' | sort -rn | head

# 文件粒度
find crates/libs/ -name "*.rs" -exec wc -l {} \; | sort -rn | head -20

# pub 占比
find crates/libs/cmx-biz/src/ -name "*.rs" -exec grep -c "^\s*pub " {} \; | awk '{s+=$1; n++} END {print "pub 占比: " s/n}'
```

---

## B. 模块设计

### B1. Trait 解耦

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| B1.1 | 跨模块依赖走 cmx-traits | Grep `use cmx_xxx::` | cmx-service 不应直接 import cmx-plugin 具体类型 | 0 个跨层强依赖 | 🔴 严重 |
| B1.2 | Trait 粒度遵循 ISP | 读 trait 定义 | 单 trait < 8 个方法 | 每个 trait ≤ 8 方法 | 🟡 警告 |
| B1.3 | 依赖倒置（DIP） | 读业务 crate 顶层 | 业务逻辑定义在 cmx-traits，实现注入 | 0 个反向依赖 | 🔴 严重 |
| B1.4 | 不存在"上帝 Struct" | 读 struct 字段 | 单 struct 字段数 < 30 | < 30 字段 | 🟡 警告 |
| B1.5 | 泛型 vs `dyn Trait` 选用正确 | 读泛型参数 | 热路径用泛型（静态分发），多态边界用 `dyn` | 误用 0 处 | 🟡 警告 |
| B1.6 | `async-trait` 仅在必要时使用 | Grep `async_trait` | 优先 Rust 1.75+ 原生 `async fn in trait` | 误用 0 处 | 🔵 建议 |
| B1.7 | 错误传播不跨模块泄露 | 读 `pub use` | 跨 crate 错误应用 `#[from]` 转换 | 0 个直接暴露他 crate 错误 | 🟡 警告 |

**审查工具**：

```bash
# 检测跨层强依赖
grep -rn "use cmx_plugin::" crates/libs/cmx-service/src
grep -rn "use cmx_iam::" crates/libs/cmx-api/src

# trait 方法数
grep -c "fn " crates/libs/cmx-traits/src/auth/service.rs

# "上帝 Struct" 字段数
awk '/pub struct/,/^}/' crates/libs/cmx-iam/src/user/entity.rs | wc -l
```

---

### B2. 代码复用（核心新增）

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| B2.1 | **Service 层复用 GenericCrudService** | Grep `GenericCrudService` | 每个标准实体 Service 都应复用 | 缺失率 0% | 🟠 中等 |
| B2.2 | **Entity derive `modql::Fields`** | Grep `modql::field::Fields` | Entity 必 derive | 缺失率 0% | 🔴 严重 |
| B2.3 | **Filter derive `modql::FilterNodes`** | Grep `modql::filter::FilterNodes` | Filter 必 derive | 缺失率 0% | 🔴 严重 |
| B2.4 | **BMC 实现 DbBmc trait** | Grep `impl DbBmc for` | 每个 BMC 必实现 | 缺失率 0% | 🔴 严重 |
| B2.5 | **Handler 用 `declare_crud_handlers!` 宏** | Grep `declare_crud_handlers!` | 标准 CRUD 不应手写 | 手写率 0% | 🟠 中等 |
| B2.6 | **SQL 参数用 `dv!` 宏** | Grep `dv!` | Service 层 SQL 参数用宏 | 偏离率 < 10% | 🟡 警告 |
| B2.7 | **动态 UPDATE 用 ParamsBuilder** | Grep `ParamsBuilder` | 动态 UPDATE 应用 builder | 偏离率 0% | 🟠 中等 |
| B2.8 | **SQL 用 `execute_sql_with_datavalues`** | Grep `execute_sql_with_json` | 禁止 json 路径 | 出现率 0% | 🟠 中等 |
| B2.9 | **权限用属性宏** | Grep `require_permission` 散落 | 必用 `#[has_permission]` 宏 | 散落率 0% | 🟡 警告 |
| B2.10 | **统一错误用 `cmx_api_types::Error`** | Grep `anyhow::Error` 在 cmx-api | 应转为 `Error` | 直接返回率 0% | 🟡 警告 |
| B2.11 | **统一响应用 `ApiResp<T>`** | Grep `Json<MyResp` 自定义响应 | 应为 `ApiResp<T>` | 自定义率 0% | 🔵 建议 |
| B2.12 | **cmx-biz Service 复用** | Grep `cmx_biz::service::` | cmx-api 业务应调 Service 而非手写 SQL | 偏离率 0% | 🔴 严重 |
| B2.13 | **cmx-traits 抽象层使用** | Grep `use cmx_traits::` | 跨模块解耦 | 偏离率 < 10% | 🟡 警告 |
| B2.14 | **ID 生成复用 cmx-utils** | Grep `Uuid::new_v4\|rand::random` | 必用 `UuidGenerator` / `snowflake_id` | 偏离率 0% | 🟡 警告 |
| B2.15 | **配置读取复用 ConfigManager** | Grep `std::env::var` | 必用 `ConfigManager::global()` | 散落率 0% | 🟡 警告 |
| B2.16 | **不重复造轮子（自定义类型）** | 人工识别 | 已有 `DataValue` / `DataSet` / `Permission` 等不重写 | 重定义 0 个 | 🟠 中等 |
| B2.17 | **业务实体不重定义** | Grep `pub struct.*Entity` in cmx-api | 业务 Entity 应在 cmx-biz 定义 | 重定义 0 个 | 🔴 严重 |
| B2.18 | **默认用 cmx-database 而非 cmx-database-pg** | Grep `cmx_database_pg\|get_default_pg_db_manager` | 非独占能力场景应改回 cmx-database。无任何 crate 独家依赖 pg | 滥用 0 处 | 🟠 中等 |
| B2.19 | **cmx-database-pg 仅用于 4 项独占能力** | Grep `stream_chunks\|TokioPgRowSource\|get_conn` | 仅 ①`query_sql_zmc_stream_chunks` ②数组列读取还原 ③`get_conn()` ④ToSql 适配器 才引入 pg。注意 `query_zmc_streaming` 两者都有，不是独占 | 偏离 0 处 | 🟡 警告 |
| B2.20 | **事务内不用 query_sql_zmc** | Grep `query_sql_zmc` 同行有 `txn` | ZmcDataSet 只读不走事务，事务内应用 `query_sql_with_datavalues` | 误用 0 处 | 🟠 中等 |
| B2.21 | **两 crate 均禁 with_json** | Grep `_with_json` in cmx-database 和 cmx-database-pg | 新代码必用 `_with_datavalues` | 出现率 0% | 🟠 中等 |
| B2.22 | **可无痛替换的 pg 消费方应替换** | 人工识别 4 个同时挂两者的 crate | cmx-api/cmz-biz/cmz-database-test/web-server 中只用对齐 API 的可改回 cmx-database（注意 `SqlParams::SeaValues` -> `SqlxValues`） | 遗漏率 < 20% | 🔵 建议 |
| B2.23 | **不误用 TokioPgRowSource 全路径** | Grep `TokioPgRowSource` | 可考虑改回 `SqlxPgRowSource`（cmx-database crate 根已导出） | 误用 0 处 | 🔵 建议 |

**完整复用资产清单**：见 [references/reuse-catalog.md](./reuse-catalog.md)。

**复用偏离度评分公式**：

```
偏离率 = 偏离次数 / 应复用总次数 × 100%
```

- < 10%：✅ 复用充分
- 10%–30%：🟡 警告
- 30%–60%：🟠 中等
- > 60%：🔴 严重

---

## C. 实现质量

### C1. 错误处理

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| C1.1 | Error 必派生 `thiserror` | Grep `impl.*Error for` / `impl Display` | 禁止手写 impl | 出现率 0% | 🔴 严重（[AGENTS.md §1.1/1.2](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| C1.2 | 禁止 `derive_more::From` | Grep `derive_more::From` | 与 thiserror 冲突 | 出现率 0% | 🔴 严重 |
| C1.3 | 每个 crate 有独立 error 模块 | 读 `src/error.rs` | 必有 | 缺失 0 | 🟡 警告 |
| C1.4 | 使用 `pub type Result<T> = ...` 糖 | Grep `type Result<` | 必用 | 缺失 0 | 🟡 警告 |
| C1.5 | 禁止裸 `unwrap()` | Grep `\.unwrap()`（排除 tests/） | 必用 `?` 或 `expect("有意义提示")` | 散落率 0% | 🟠 中等（[AGENTS.md §1.4](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| C1.6 | 跨模块错误用 `#[from]` 转换 | Grep `\[\s*from\s*\]` | 禁止跨层直接暴露 | 缺失 0 | 🟡 警告 |
| C1.7 | 异步状态共享不滥用 `Arc<Mutex<...>>` | Grep `Arc<Mutex<` | 优先 `Arc<RwLock<...>>` 或 channel | 滥用 < 3 处 | 🟡 警告 |
| C1.8 | `init()` 返回 `Result<()>` | 读 `init()` 函数 | 禁止 `panic!` / `expect` / `unwrap` | 违例 0 | 🔴 严重（[AGENTS.md §17](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |

**审查工具**：

```bash
# 禁止手写 Error impl
grep -rn "impl.*Error for\|impl Display for" crates/libs/ --include="*.rs"

# 禁止 derive_more::From
grep -rn "derive_more::From\|derive(\s*From" crates/libs/ --include="*.rs"

# 禁止裸 unwrap（排除 tests/）
grep -rn "\.unwrap()" crates/libs/ --include="*.rs" | grep -v "/tests/"

# 必返回 Result 的 init
grep -rn "fn init(" crates/libs/ --include="*.rs" -A 2
```

---

### C2. 异步编程模式

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| C2.1 | `Send` / `Sync` 约束合理 | 读 trait bounds | 必要才加，不滥用 | 误用 0 处 | 🟡 警告 |
| C2.2 | 异步边界正确 | 读 async fn | 真正 I/O 才用 async | 误用 0 处 | 🟡 警告 |
| C2.3 | 不存在 `block_in_place` 滥用 | Grep `block_in_place` | 配合 rayon 短任务 | 误用 0 处 | 🟡 警告 |
| C2.4 | 资源生命周期管理正确 | 读资源持有 | Drop / 显式 close | 资源泄漏 0 | 🔴 严重 |
| C2.5 | 异步 trait 优先用 1.75+ 原生 | Grep `async_trait` | 1.75+ 后用原生 | 误用 0 处 | 🔵 建议 |
| C2.6 | 取消安全有保障 | 读 async fn | 重要操作有 cancel safety 注释 | 缺失 < 5% | 🟡 警告 |
| C2.7 | `tokio::spawn` 必带错误处理 | Grep `tokio::spawn` | 必 `.await` 或 `JoinHandle` | 裸 spawn 0 | 🟡 警告 |

---

### C3. Rust 最佳实践（核心新增）

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| C3.1 | **命名规范** | Grep 类型 / 常量 / 函数名 | PascalCase / SCREAMING_SNAKE_CASE / snake_case | 违例 0 | 🔵 建议 |
| C3.2 | **API 文档覆盖率** | 读 `pub fn` | 每个 `pub fn` 必含 `# Arguments` + `# Returns` + `# Examples` | 覆盖率 100% | 🟠 中等（[AGENTS.md §13](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| C3.3 | **文档摘要以句号结尾** | Grep `///` 首行 | 必以 `.` 结尾 | 违例 0 | 🟡 警告 |
| C3.4 | **禁止 `////` 注释** | Grep `////` | 必用 `///` | 违例 0 | 🔵 建议 |
| C3.5 | **禁止块注释** | Grep `/\*\* \|/\* ` | 优先行注释 | 违例 0 | 🔵 建议 |
| C3.6 | **`# Examples` 用复数** | Grep `# Example[^s]` | 复数 | 违例 0 | 🔵 建议 |
| C3.7 | **TODO / FIXME / SAFETY 不在 `///` 中** | Grep `///.*TODO\|///.*FIXME\|///.*SAFETY` | 必用行注释 `// SAFETY: ...` | 违例 0 | 🟡 警告 |
| C3.8 | **`#[must_use]` 标注** | Grep `pub fn.*->` | 重要返回应标注 | 缺失 < 5% | 🔵 建议 |
| C3.9 | **字符串使用 `&str` 而非 `&String`** | Grep `&String` | 优先 `&str` | 滥用 < 3 处 | 🔵 建议 |
| C3.10 | **集合用迭代器** | Grep `for i in 0\.\.vec\.len()` | 优先 `iter()` | 滥用 < 3 处 | 🟡 警告 |
| C3.11 | **避免 `.clone()` 热路径** | Grep `\.clone()` | 评估必要性 | 误用 0 处（人工评估） | 🟡 警告 |
| C3.12 | **`if let Some/Ok` 优于 `match`** | 读 match 单分支 | 优先 `if let` | 滥用 < 5 处 | 🔵 建议 |
| C3.13 | **Builder 模式构造复杂对象** | 读 struct 构造 | 字段 > 5 用 builder | 违反 0 处 | 🔵 建议 |
| C3.14 | **Newtype 模式隔离类型** | 读 typedef | 同语义不同源用 newtype | 误用 0 | 🔵 建议 |
| C3.15 | **`#[derive(Debug)]`** | 读 struct | 全部 derive | 缺失 0 | 🔵 建议 |
| C3.16 | **类型不冗余** | 读函数签名 | `Result<T, Error>` 而非 `Result<T, Box<dyn Error>>` | 滥用 0 | 🟡 警告 |
| C3.17 | **unsafe 隔离** | Grep `unsafe` | 必须 `// SAFETY: ...` 注释 | 注释覆盖率 100% | 🟠 中等 |
| C3.18 | **错误信息有指导意义** | 读 Error 定义 | `#[error("...")]` 必含原因 | 模糊信息 0 | 🟡 警告 |
| C3.19 | **无 dead_code** | `cargo build` 输出 | 不用代码删或 `#[allow(dead_code)]` 注释保留 | 活跃 0 | 🟡 警告（[AGENTS.md §3.6](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| C3.20 | **无未使用导入** | `cargo build` 输出 | 必删 | 出现 0 | 🔵 建议 |
| C3.21 | **`pub struct` 字段私有化** | Grep `pub struct` | 必 `pub(crate)` 优先，构造器或 builder 暴露 | 滥用 0 | 🟡 警告 |

**审查工具**：

```bash
# 文档注释覆盖率
total_pub_fn=$(grep -rn "^\s*pub fn\b" crates/libs/cmx-biz/src/ | wc -l)
doced=$(grep -B1 "^\s*pub fn\b" crates/libs/cmx-biz/src/ -r | grep "///" | wc -l)
echo "文档覆盖率: $doced / $total_pub_fn"

# 命名违例
grep -rn "^\s*pub fn [A-Z]" crates/libs/ --include="*.rs"
grep -rn "^\s*pub struct [a-z]" crates/libs/ --include="*.rs"
grep -rn "^\s*const [a-z]" crates/libs/ --include="*.rs"

# unsafe 注释
grep -B1 "^\s*unsafe\b" crates/libs/ -rn --include="*.rs" | grep "SAFETY" | wc -l
```

---

## D. 工程规范

### D1. 依赖管理

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| D1.1 | workspace 集中管理依赖 | 读子 `Cargo.toml` | 子 crate 无 `version = "x.y"` 形式 | 违例 0 | 🔴 严重（[AGENTS.md §3.1](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D1.2 | 用 `workspace = true` | Grep `{ workspace = true }` | 必有 | 缺失 0 | 🔴 严重 |
| D1.3 | 依赖有清晰注释 | 读 `Cargo.toml` | 每个依赖上方一行注释 | 缺失 0 | 🟡 警告（[AGENTS.md §3.3](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D1.4 | 禁止分组注释 | Grep `^# ===` | 单行注释 | 违例 0 | 🔵 建议 |
| D1.5 | 禁止 `log` crate | Grep `^log = \| use log::` | 必用 `tracing` | 出现率 0% | 🔴 严重（[AGENTS.md §3.4](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D1.6 | 未用依赖注释保留 | 读 `Cargo.toml` | 不删而注释 `# [dependencies]\n# xxx = "1.0"` | 缺失 0 | 🔵 建议（[AGENTS.md §3.6](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D1.7 | Feature 精确控制 | 读 features | 不启无用 feature | 滥用 0 | 🟡 警告 |
| D1.8 | 内部依赖 `version` + `path` | 读 workspace deps | 内部 crate 必带 `path` | 缺失 0 | 🟡 警告 |

**审查工具**：

```bash
# 子 crate 硬编码版本（违规）
for f in crates/libs/*/Cargo.toml crates/libs/cmx-infra/*/Cargo.toml; do
  grep -E "^[a-z\-]+ = \"[0-9]" "$f" 2>/dev/null | head -3
done

# 禁止 log crate
grep -rn "^log = " crates/libs/*/Cargo.toml crates/libs/cmx-infra/*/Cargo.toml 2>/dev/null

# workspace = true 缺失
for f in crates/libs/*/Cargo.toml; do
  echo "=== $f ==="
  grep -A1 "^\[dependencies\]" "$f" | head
done
```

---

### D2. 命名规范

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| D2.1 | 文件名 snake_case | `ls crates/libs/*/src/` | 全 snake_case | 违例 0 | 🔵 建议 |
| D2.2 | 类型名 PascalCase | Grep `pub struct/enum/trait` | 全 PascalCase | 违例 0 | 🔵 建议 |
| D2.3 | 函数名 snake_case | Grep `pub fn` | 全 snake_case | 违例 0 | 🔵 建议 |
| D2.4 | 常量 SCREAMING_SNAKE_CASE | Grep `pub const` | 全大写下划线 | 违例 0 | 🔵 建议 |
| D2.5 | 枚举变体 PascalCase | Grep `enum.*\{` | 全 PascalCase | 违例 0 | 🔵 建议 |
| D2.6 | 模块名 snake_case | `ls` 模块目录 | 全 snake_case | 违例 0 | 🔵 建议 |
| D2.7 | `plugin_id` 用下划线 | Grep `plugin_id.*-` | 禁止连字符 | 违例 0 | 🟡 警告（[AGENTS.md §11](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D2.8 | 表名以 `cmx_` 前缀 | 读 DDL | 系统表必带 | 违例 0 | 🟡 警告（[AGENTS.md §5.4](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D2.9 | 禁止外键约束 | Grep `FOREIGN KEY` | DDL 必无 | 出现 0 | 🟡 警告（[AGENTS.md §5.4](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D2.10 | 审计字段 9 项齐全 | 读表 DDL | 标准审计字段 | 缺失表 0 | 🟡 警告 |

---

### D3. 注释规范

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| D3.1 | `pub fn` 含 `# Arguments` + `# Returns` | Grep `pub fn` 上方 | 必含 | 覆盖率 100% | 🟠 中等（[AGENTS.md §13](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） |
| D3.2 | 文档摘要以句号结尾 | Grep `///\s*[A-Z][^.]*$` | 必 `.` 结尾 | 违例 0 | 🟡 警告 |
| D3.3 | `lib.rs` / `mod.rs` 顶部 `//!` 注释 | 读文件顶部 | 必有 | 缺失 0 | 🟡 警告 |
| D3.4 | `///` 与 `//` 区分使用 | 读注释 | `///` 项，`//` 函数体 | 误用 0 | 🔵 建议 |
| D3.5 | 复杂逻辑有 `// 解释为什么` 而非 `// 解释做什么` | 读函数体 | 解释 why | 自评 | 🟡 警告 |
| D3.6 | `// TODO: / FIXME: / HACK: / SAFETY:` 在行注释 | Grep `///.*TODO` | 禁止 | 违例 0 | 🔵 建议 |
| D3.7 | `pub fn` 注释覆盖率 | 自动化 | 100% | < 100% 即 🟠 | 🟠 中等 |
| D3.8 | `#[plugin_fn]` 函数摘要**不以句号结尾** | 读 WASM 函数 | 例外（[AGENTS.md §13](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md)） | 例外豁免 | — |

> 详细注释规范：[rust-comment-convention](../../rust-comment-convention/SKILL.md) 技能。

---

### D4. 测试

| # | 检查项 | 检查方法 | 通过标准 | 量化阈值 | 建议严重级别 |
|---|--------|---------|---------|---------|-------------|
| D4.1 | 单元测试存在 | 读 `#[cfg(test)]` / `mod tests` | 核心逻辑必有 | 覆盖率 ≥ 60% | 🟡 警告 |
| D4.2 | 集成测试在 `tests/` 目录 | 读 `tests/` | 必有 | 缺失 0 | 🔵 建议 |
| D4.3 | 异步测试用 `#[tokio::test]` | Grep `#[test]` + async | 正确标注 | 误用 0 | 🟡 警告 |
| D4.4 | 测试用 `unwrap()` 合法 | 读测试代码 | 可用 | 0 约束 | — |
| D4.5 | 关键 Service 有 happy path 测试 | 读 Service.rs | 必有 | 缺失 0 | 🟡 警告 |
| D4.6 | 关键 Service 有 error path 测试 | 读 Service.rs | 必有 | 缺失率 < 50% | 🟡 警告 |
| D4.7 | Handler 端到端测试 | 读 `tests/*_e2e.rs` | 关键 Handler 必有 | 缺失 0 | 🟡 警告 |
| D4.8 | 不留 `#[ignore]` 测试 | Grep `#\[ignore\]` | 不留 | 出现 0 | 🔵 建议 |
| D4.9 | WASM 插件 `tests/` 对应 handlers/ | 读插件目录 | 一一对应 | 缺失 0 | 🟡 警告 |

---

## E. 跨维度的硬约束（项目特定）

### E1. 业务领域硬约束

| 规范条目 | 来源 | 检查方法 | 量化阈值 | 严重级别 |
|---------|------|---------|---------|---------|
| `app_id` 取自 `get_app_id()`，禁止硬编码 `"default"` | [AGENTS.md §6](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `"default"` 在 plugin install/import | 出现 0 | 🔴 严重 |
| `app_id ≡ module_code`（当前架构），不要把 `application_code` 当 `app_id` | [AGENTS.md §6](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 install/import 逻辑 | 误用 0 | 🟡 警告 |
| Service `list/page` 必用 `filters + list_options` | [AGENTS.md §7](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `(page_size, keyword,` 散落 | 散落 0 | 🟠 中等 |
| Handler 路由除 `get_by_id` 外全 POST + JSON | [AGENTS.md §8.3](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `.route(.*get(` | 误用 0 | 🟠 中等 |
| `ForCreate` 不含 id/create_time/update_time | [AGENTS.md §8.3](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 Entity 定义 | 违例 0 | 🟡 警告 |
| `ForUpdate` 字段全 Option | [AGENTS.md §8.3](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 Entity 定义 | 违例 0 | 🟡 警告 |
| `Entity` 必 `derive(Fields)` | [AGENTS.md §9](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `derive.*Fields` | 缺失 0 | 🔴 严重 |
| `Filter` 必 `derive(FilterNodes)`，字段用 `OpValsXxx` | [AGENTS.md §9](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `FilterNodes` | 缺失 0 | 🔴 严重 |
| 必用 `execute_sql_with_datavalues` 而非 `_with_json` | [AGENTS.md §10](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `_with_json` | 出现 0 | 🔴 严重 |
| 动态 UPDATE 必用 `ParamsBuilder` | [AGENTS.md §10](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `format!("\$\d` | 散落 0 | 🟠 中等 |
| 事务内传 `txn_id: Some(&txn_id)` | [AGENTS.md §10](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读事务代码 | 误用 0 | 🟡 警告 |
| `plugin_id` 只能用 `_` | [AGENTS.md §11](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | Grep `plugin_id.*-` | 违例 0 | 🟠 中等 |
| 插件 metadata `ordinal` 连续不跳跃 | [AGENTS.md §12](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 metadata | 跳跃 0 | 🟡 警告 |
| 种子数据 `conflict_columns` 选业务唯一列 | [AGENTS.md §12](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 seeddata | 误选 0 | 🟡 警告 |
| cmx-core 不引入业务依赖 | [AGENTS.md §14](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读 `cmx-core/Cargo.toml` | 0 业务依赖 | 🔴 严重 |
| 迁移文件 `YYYYMMDD_XXX.{up,down}.sql` | [AGENTS.md §5.5](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | `ls docs/sql/migrations/` | 命名违例 0 | 🟡 警告 |
| `init_ddl.sql` 保持最新完整 | [AGENTS.md §5.6](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读迁移文件 | 同步 0 偏差 | 🟡 警告 |
| 旧接口标记（cmx-model/src/dict/ 等）不参考 | [AGENTS.md §18](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) | 读新代码 | 参考 0 | 🔴 严重 |

---

## F. 审查执行细节

### F1. 单文件审查（小范围）

执行顺序：

1. 读 `Cargo.toml`（5 行）→ 确认依赖合规（D1）
2. 读整个文件 → 检查命名（D2）、注释（D3）
3. Grep 错误处理模式（C1）
4. Grep `pub` 占比（A2.3）
5. 检查是否复用现有资产（B2）
6. 10 分钟内完成

### F2. 单 crate 审查（中等范围）

执行顺序：

1. 读 `Cargo.toml` + `lib.rs` + `src/error.rs`（架构入口）
2. 读目录树 → 评估目录结构（A2）
3. 抽样读 3-5 个核心文件
4. Grep 业务领域硬约束（E1）
5. 复用偏离度扫描（B2.1-B2.17）
6. 规范符合度扫描（C/D）
7. 30-60 分钟完成

### F3. 全 workspace 审查（大规模）

执行顺序：

1. workspace 拓扑扫描（A1.1-A1.6）
2. 循环依赖检测（A1.2）
3. cmx-core 轻量约束（A1.3）
4. 按 crate 逐个审查（每 crate 30 分钟）
5. 全量复用偏离度（B2）
6. 全量规范符合度（C/D）
7. 跨 crate 一致性（避免平行实现）
8. 1-2 工作日完成；建议按 crate 分批输出报告

---

## G. 与本清单配套的关联文档

- [reuse-catalog.md](./reuse-catalog.md)：项目级可复用资产清单
- [anti-patterns.md](./anti-patterns.md)：常见反模式 + 项目内真实案例
- [report-template.md](./report-template.md)：审查报告输出模板
- [项目 AGENTS.md 18 章规范](../../../AGENTS.md)
- [项目 .trae/rules/project_rule.md](../../../.trae/rules/project_rule.md)
