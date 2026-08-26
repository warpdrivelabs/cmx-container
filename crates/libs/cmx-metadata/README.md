# cmx-metadata

> 表定义元数据管理模块：表定义 JSON 加载、DDL 生成/解析、增量 DDL diff、DDL 执行、i18n 伴生表派生、种子数据装载执行器。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

## 项目简介

cmx-metadata 是 cmx-container 项目的**表定义执行器**：把「表定义 JSON」落地为数据库中的物理表（DDL 生成/执行）、维护表结构演进（增量 diff）、派生多语言伴生表，并装载种子数据。

职责边界：

- 基础结构体（`TableDefine` / `ColumnDefine` / `FieldType` / `IndexDefine` / `TableDefinesConfig` / `SeedDataConfig`）定义在 `cmx-core`（`cmx_core::model::meta`），本模块提供加载、解析、生成、执行能力
- DDL/SQL 执行依赖 `cmx-database` 的全局 `DatabaseManager`（`get_default_db_manager()`），按 `(db_id, txn_id)` 寻址，可参与外部事务
- **与 cmx-model-meta 的分工**：`cmx-model-meta`（crates/libs/cmx-model/）是模型中心的**设计期元数据建模**（DCT 数据字典 / DOC 业务单据 / FC 弹性组合定义，纯 JSON 文件、不落库）；本 crate 处理的是**物理表定义 → DDL → 建表/迁移/种子数据**的执行链路。模型中心部署时（cmx-model-deploy）会复用本 crate 的执行能力

主要使用方：`cmx-plugin`（插件/模块安装时建表 + 种子数据）、`cmx-model-deploy`（模型部署）、`cmx-common-api`。

## 快速开始

### 安装

```toml
[dependencies]
cmx-metadata = { workspace = true }
```

### 核心示例

```rust
use cmx_metadata::{
    load_table_defines_from_path, PostgresDdlDialect,
    PgTableDefineExecutor, TableDefineDbExecutor, // trait 方法需要
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 从表定义 JSON（{ "tables": [...] }）加载
    let tables = load_table_defines_from_path(Path::new("sys_tables.json"))?;

    // 2. 构造执行器（db_id + 可选事务 ID；SQL 经全局 DatabaseManager 执行）
    let executor = PgTableDefineExecutor::new("default", None);

    // 3. 建表或升级（表已存在时走增量升级）
    for table in &tables {
        executor.create_or_upgrade_table(table).await?;
    }
    Ok(())
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 表定义加载 | 单表/多表 JSON 文件与字符串加载（`loader`），多配置文件管理与依赖排序（`config`） |
| DDL 生成 | `DdlDialect` trait（方言抽象）+ `PostgresDdlDialect` 实现：CREATE TABLE / INDEX / COMMENT / DROP / ALTER |
| DDL 解析 | `DdlParser` trait + `PostgresDdlParser`：DDL 字符串 → `TableDefine`（往返） |
| 增量 DDL | `DdlDiff`：两组 `TableDefine` 比对，产出 `TableChange` 列表并可转为 ALTER DDL 语句；含默认值语义等价比较（消除编译侧/内省侧形态差异假阳性）、索引改名重建、INVALID 索引强制重建、手工索引保护（不删只报告） |
| DDL 执行 | `PgTableDefineExecutor`（建表/升级，先探测表存在性分流，消除误导日志）+ `execute_ddl_by_ids` 等自由函数（裸 DDL 语句执行） |
| 库内表结构回读 | `PgTableDefineExecutor::query_current_table_define`：查 `information_schema` 重建 `TableDefine`；INVALID/NOT-READY 索引以 `valid = false` 带出（diff 侧强制 DROP+重建） |
| i18n 伴生表 | `derive_i18n_table_define`：为 `i18n: true` 的表派生 `<表名>_i18n` 伴生表定义 |
| 种子数据 | `load_seed_data`（.json / .csv）+ `PgSeedDataExecutor`（批量 upsert，冲突列可配，产出 `SeedDataSummary`） |

## 模块结构

```
cmx-metadata
├── src/
│   ├── lib.rs              # 库入口（导出 DdlDialect / DdlDiff / PgTableDefineExecutor 等）
│   ├── config.rs           # TableDefinesConfigManager：多配置文件管理、依赖排序、批量装载
│   ├── loader.rs           # 表定义 JSON 加载（单表/多表，文件/字符串）
│   ├── ddl/                # DDL 生成与 diff
│   │   ├── mod.rs          #   DdlDialect trait（方言抽象）
│   │   ├── postgres.rs     #   PostgresDdlDialect 实现
│   │   └── diff.rs         #   DdlDiff 增量比对 + TableChange/ColumnChange 等变更类型
│   ├── parser/             # DDL 解析（DDL → TableDefine）
│   │   ├── mod.rs          #   DdlParser trait + pg_ddl_to_table_defines 快捷函数
│   │   └── postgres.rs     #   PostgresDdlParser 实现
│   ├── executor.rs         # TableDefineDbExecutor trait + PgTableDefineExecutor + 裸 DDL 执行函数
│   ├── i18n.rs             # derive_i18n_table_define 伴生表派生
│   ├── seed/               # 种子数据
│   │   ├── config.rs       #   SeedDataSummary / SeedDataTableResult 等结果类型
│   │   ├── loader.rs       #   load_seed_data（.json / .csv）
│   │   ├── dml.rs          #   upsert DML 构建
│   │   └── executor.rs     #   PgSeedDataExecutor
│   └── error.rs            # MetadataError
└── Cargo.toml
```

## 核心类型（定义在 cmx-core）

### `TableDefine`（`cmx_core::model::meta::table`）

表定义结构，主要字段：`table_name` / `display_name` / `columns` / `primary_keys` / `indexes` / `version` / `i18n` / `comment` / `schema` / `is_partitioned` / `partition_type` / `partition_columns` / `extensions`（完整字段以 cmx-core 为准）。

### `ColumnDefine`

列定义，主要字段：`name` / `label` / `field_type` / `is_primary_key` / `is_nullable` / `default_value` / `i18n` / `length` / `precision` / `scale` / `db_type` / `ordinal` / `is_foreign_key` / `foreign_key_table`。

### `FieldType`

逻辑字段类型枚举：`String` / `Int` / `Float` / `Decimal` / `DateTime` / `Date` / `Bool` / `Text` / `Binary` / `Array` / `Json` / `Uuid` / `Unknown`。各方言经 `DdlDialect::map_column_type` 映射为具体 SQL 类型。

### `TableDefinesConfig`（`cmx_core::model::meta::plugin`）

建表配置入口（`*_config.json`）：`name` / `description` / `files`（表定义文件列表）/ `depends_on`（依赖的其他配置）/ `priority`（越小越先）/ `seed_data: Vec<SeedDataConfig>`。插件 manifest 的 `table_config_files` 指向此类文件。

### `SeedDataConfig`

种子数据条目：`table_name` / `file`（数据文件路径）/ `conflict_columns`（ON CONFLICT 冲突检测列）/ `enabled`。

## 使用指南

### 一、表定义加载

#### 1.1 从文件/字符串加载表定义

```rust
use cmx_metadata::{table_defines_from_str, load_table_defines_from_path};
use std::path::Path;

// 多表 JSON 文件（{ "tables": [ {...}, ... ] }）
let tables = load_table_defines_from_path(Path::new("sys_tables.json"))?;

// 单表结构（{ "table_name": ..., "columns": [...] }）
let table = cmx_metadata::load_table_define_from_path(Path::new("one_table.json"))?;

// 从字符串解析（校验/调试场景）
let tables = table_defines_from_str(r#"{ "tables": [ /* TableDefine 数组 */ ] }"#)?;
```

表定义 JSON 片段（与 `TableDefine` serde 格式一致）：

```json
{
  "table_name": "cmx_domain",
  "display_name": "域",
  "version": 1,
  "primary_keys": ["id"],
  "i18n": false,
  "columns": [
    {
      "name": "code",
      "label": "域编码",
      "field_type": "String",
      "is_nullable": false,
      "length": 32,
      "db_type": "VARCHAR(32)"
    }
  ]
}
```

#### 1.2 多配置文件管理（TableDefinesConfigManager）

```rust
use cmx_metadata::config::TableDefinesConfigManager;
use std::path::Path;

// 从多个 *_config.json 构造，按 depends_on / priority 拓扑排序
let manager = TableDefinesConfigManager::from_config_paths(&[
    "meta_scripts/domain_app_module_config.json",
    "meta_scripts/plugin_marketplace_registry_config.json",
])?;

// 依赖有序的配置列表（priority 小者在前，被依赖者先装载）
for config in manager.sorted_configs()? {
    println!("config: {} ({} files)", config.name, config.files.len());
}

// 以配置目录为基准，批量加载其引用的全部表定义文件
let all_tables = manager.load_all_tables(Path::new("meta_scripts/"))?;
println!("共 {} 张表", all_tables.len());
```

### 二、DDL 生成

```rust
use cmx_metadata::{DdlDialect, PostgresDdlDialect};

let dialect = PostgresDdlDialect;

// 单表完整 DDL（CREATE TABLE + CREATE INDEX + COMMENT ON）
let full = dialect.generate_full_ddl(&table)?;

// 也可分步获取
let create = dialect.generate_create_table(&table)?;
let indexes = dialect.generate_create_indexes(&table)?;   // Vec<String>
let comments = dialect.generate_comments(&table)?;        // 表注释 + 列注释

// 列级 ALTER 语句生成（供增量变更使用；schema 传 Some("public") 或 None）
let add_col = dialect.generate_add_column("t", None, &new_column)?;
let alter_cols = dialect.generate_alter_column("t", None, &old_col, &new_col)?; // Vec<String>
let drop_col = dialect.generate_drop_column("t", None, "old_col")?;
let drop_table = dialect.generate_drop_table(&table)?; // DROP TABLE IF EXISTS

// 多表完整 DDL（按入参顺序拼接）
let ddl = dialect.generate_full_ddl_for_tables(&tables)?;
```

### 三、DDL 解析（DDL → TableDefine）

```rust
use cmx_metadata::parser::{pg_ddl_to_table_defines, PostgresDdlParser};

let ddl = r#"
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);
CREATE INDEX idx_users_name ON users (name);
"#;

// 快捷函数：一次解析多张表
let tables = pg_ddl_to_table_defines(ddl)?;

// 或经 trait（方言抽象）
use cmx_metadata::parser::DdlParser;
let parser = PostgresDdlParser;
let one_table = parser.parse_create_table("CREATE TABLE users (id BIGINT PRIMARY KEY)")?;
let many = parser.parse_ddl(ddl)?;
```

### 四、增量 DDL（diff → ALTER 语句）

```rust
use cmx_metadata::{DdlDiff, PostgresDdlDialect};

let dialect = PostgresDdlDialect;

// 比对两组表定义（如：库内现状 vs 新版定义），产出变更列表
let changes = DdlDiff::diff(&old_tables, &new_tables);
for change in &changes {
    println!("change: {:?}", change); // TableChange::CreateTable / DropTable / ModifyTable(...)
}

// 变更列表 → ALTER DDL 语句
let stmts = DdlDiff::changes_to_ddl(&dialect, &changes)?;

// 一步到位：diff + to_ddl
let stmts = DdlDiff::diff_to_ddl(&dialect, &old_tables, &new_tables)?;
for s in &stmts {
    println!("{}", s);
}
```

变更类型（`ddl::diff`）：`TableChange`（建表/删表/改表）、`ColumnChange`（加列/改列/删列）、`IndexChange`（`AddIndex` / `DropIndex`（携带旧索引定义）/ `RenameIndex { old, new }`（内容一致仅改名，旧名为系统命名 `uk_`/`idx_` 前缀时 DROP 旧名+CREATE 新名）/ `PreservedManualIndex`（手工索引保留不删，仅报告提示））、`ColumnCommentChange`（注释变更）。

索引删除判定三档（库中多余且定义无内容匹配时）：
1. INVALID 索引——占名必碍事，无条件 DROP（自愈优先）；
2. 系统命名（`uk_`/`idx_` 前缀）或名字仍在当前定义中——本系统管理，DROP；
3. 其余——视为 DBA 手工创建，`PreservedManualIndex` 保留不删（不生成 DDL）。

列默认值比较经 `defaults_equivalent` 语义等价判定（两侧同套归一化：剥 `::type` cast 后缀/外层单引号定界/布尔大小写，jsonb 空格差异语义兜底），消除首次部署后每次部署都重复 `SET DEFAULT` 的永久假阳性；已知局限：timestamp 字面量落库被 PG 补零（`'2023-01-01'` → `'2023-01-01 00:00:00'::timestamp`）仍判为变更（存量定义无日期类默认值）。

### 五、DDL 执行

#### 5.1 PgTableDefineExecutor（建表/升级）

```rust
use cmx_metadata::{PgTableDefineExecutor, TableDefineDbExecutor};

// db_id 定位数据源；txn_id 传入则语句加入外部事务
let executor = PgTableDefineExecutor::new("default", None);

// 表不存在则建表；已存在则按列/索引 diff 升级
executor.create_or_upgrade_table(&table).await?;

// 也可以显式二选一
executor.create_table(&table).await?;
executor.upgrade_table(&table).await?;
```

#### 5.2 回读库内表结构

```rust
// 查 information_schema.columns / 表索引，重建 TableDefine（用于与新版定义 diff）
let current = executor.query_current_table_define(&table).await?;
let stmts = DdlDiff::diff_to_ddl(&PostgresDdlDialect, &[current], &[table])?;
```

#### 5.3 裸 DDL 语句执行（自由函数）

```rust
use cmx_metadata::execute_ddl_by_ids;

// 逐条执行（多语句）；单条可用 execute_ddl_statement_by_ids（返回 affected rows）
execute_ddl_by_ids("default", None, &stmts).await?;
```

注：所有执行入口均通过 `cmx_database::get_default_db_manager()` 全局管理器发送 SQL，executor 本身只持有 `(db_id, txn_id)` 寻址信息。

### 六、国际化（i18n）伴生表

```rust
use cmx_metadata::i18n::derive_i18n_table_define;

// 表级 i18n=true 且存在 i18n=true 的列时，派生 <table_name>_i18n 伴生表；
// 伴生表含 ref_id + locale 组合主键与全部 i18n 列；否则返回 None
if let Some(i18n_table) = derive_i18n_table_define(&table) {
    executor.create_or_upgrade_table(&i18n_table).await?;
}
```

### 七、种子数据

#### 7.1 数据文件格式

`SeedDataConfig.file` 支持 `.json`（对象数组，一行一条）与 `.csv`（首行为列名）：

```json
[
  { "code": "FI", "name": "财务域" },
  { "code": "SCM", "name": "供应链域" }
]
```

#### 7.2 装载与执行

```rust
use cmx_core::model::meta::plugin::SeedDataConfig;
use cmx_metadata::seed::PgSeedDataExecutor;

let seed_configs = vec![SeedDataConfig {
    table_name: "cmx_domain".to_string(),
    file: "seeddata/cmx_domain.json".to_string(),
    conflict_columns: vec!["code".to_string()], // ON CONFLICT (code) DO UPDATE
    enabled: true,
}];

let executor = PgSeedDataExecutor::with_batch_size("default", None, 500); // 批量插入大小

// 以插件/模块根目录为基准，按表定义批量装载（列名对齐 + upsert），返回汇总
let summary = executor.execute_all_seed_data(&tables, &seed_configs, Path::new("module_root/")).await;
if summary.has_errors() {
    for r in &summary.table_results { /* SeedDataTableResult：成功行数/失败明细 */ }
}
```

### 八、完整示例（模块安装视角）

```rust
use cmx_metadata::{
    load_table_defines_from_path, TableDefinesConfigManager,
    PgTableDefineExecutor, TableDefineDbExecutor,
    seed::PgSeedDataExecutor,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module_root = Path::new("my_module/");

    // 1. 读取配置入口（manifest.table_config_files 指向的 *_config.json）
    let manager = TableDefinesConfigManager::from_config_paths(&["my_module/config/tables_config.json"])?;

    // 2. 依赖有序装载全部表定义
    let tables = manager.load_all_tables(&module_root.join("config"))?;

    // 3. 建表/升级
    let ddl_executor = PgTableDefineExecutor::new("default", None);
    for table in &tables {
        ddl_executor.create_or_upgrade_table(table).await?;
    }

    // 4. 种子数据（各配置的 seed_data 汇总）
    let seeds = manager
        .sorted_configs()?
        .iter()
        .flat_map(|c| c.seed_data.clone())
        .collect::<Vec<_>>();
    let summary = PgSeedDataExecutor::new("default", None)
        .execute_all_seed_data(&tables, &seeds, module_root)
        .await;
    println!("种子数据：成功 {} 条 / 失败 {} 条", summary.total_success(), summary.total_failed());

    Ok(())
}
```

### 九、错误处理

`MetadataError`（thiserror）变体：

| 变体 | 场景 |
|------|------|
| `Io(std::io::Error)` | 文件读写错误（自动 From） |
| `Json(serde_json::Error)` | JSON 解析错误（自动 From） |
| `DdlGeneration(String)` | DDL 生成失败 |
| `DdlParse(String)` | DDL 解析失败 |
| `DdlExecution(String)` | DDL/SQL 执行失败 |
| `ConfigNotFound(String)` | 配置文件或表项未找到 |
| `ConfigDependency(String)` | 配置依赖缺失或循环 |
| `SeedData(String)` | 种子数据装载失败 |

```rust
use cmx_metadata::MetadataError;

match result {
    Err(MetadataError::DdlExecution(msg)) => eprintln!("DDL 执行失败: {}", msg),
    Err(MetadataError::ConfigDependency(msg)) => eprintln!("配置依赖问题: {}", msg),
    Err(e) => eprintln!("其他错误: {}", e),
    _ => {}
}
```

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-core` | TableDefine / ColumnDefine / FieldType / TableDefinesConfig / SeedDataConfig 等元数据结构 |
| `cmx-database` | 全局 DatabaseManager（`get_default_db_manager`）执行 SQL |
| `sea-query` / `sea-query-sqlx` | SQL 构建（种子数据 upsert 等） |
| `serde` / `serde_json` / `csv` / `regex` / `chrono` / `tokio` / `tracing` | 序列化、CSV 解析、异步、日志 |

### 下游使用方

| 使用方 | 用途 |
|--------|------|
| `cmx-plugin` | 插件/模块安装：表定义导入（`table_definition_importer`）→ 建表 → 种子数据 |
| `cmx-model-deploy` | 模型中心部署的建表与初始化 |
| `cmx-common-api` | HTTP 侧元数据相关能力 |

## 关键设计决策

### 1. 为什么用 `DdlDialect` / `DdlParser` trait 抽象？

表定义（`TableDefine`）是方言无关的逻辑模型（`FieldType` 为逻辑类型），生成与解析均经 trait 隔离：`PostgresDdlDialect` 是当前唯一完整实现，新增 MySQL/Oracle 等方言只需实现 trait，加载/diff/执行链路无需改动。

### 2. 为什么执行器只持 `(db_id, txn_id)` 而不持有连接？

`PgTableDefineExecutor` / `PgSeedDataExecutor` 均不持有 `DatabaseManager`，而是每次经 `cmx_database::get_default_db_manager()` 全局管理器按 `(db_id, txn_id)` 寻址执行。好处：

- 同一套执行器可在任意数据源（多租户动态数据源）上工作；
- 传入外部事务 ID 即可无缝加入调用方事务（如模块安装的“建表+种子+台账”原子流程）；
- 无连接生命周期管理负担，executor 可随意克隆/复用。

### 3. 为什么 diff 的中间表示是 `Vec<TableChange>` 而非直接生成 SQL？

`DdlDiff::diff` 产出结构化变更（建表/删表/加列/改列/索引/注释），`changes_to_ddl` 再按方言渲染为 SQL。两层解耦后：调用方可先审阅/过滤变更（如禁止删列）再生成 DDL，也便于未来接入非 SQL 目标（如生成迁移脚本文件）。

## 常见问题

### Q1: cmx-metadata 与 cmx-model-meta 是什么关系？

**A**: 两者都含「元数据」但层次不同。`cmx-model-meta` 管模型中心的设计期元数据（DCT 字典定义 / DOC 单据定义 / FC 弹性组合，JSON 文件存储、不落库）；`cmx-metadata` 管物理表定义（`TableDefine` JSON → DDL → 建表/迁移/种子数据）。模型部署（cmx-model-deploy）会把模型中心的定义落到数据库时，复用本 crate 的执行器。

### Q2: 表定义结构体为什么不在本 crate 定义？

**A**: `TableDefine` / `ColumnDefine` / `FieldType` / `TableDefinesConfig` / `SeedDataConfig` 定义在 `cmx-core`（`cmx_core::model::meta`），因为插件 SDK、插件运行时（WASM 内）等也需要同一份结构。本 crate 是围绕这些结构的加载/生成/解析/执行器。

### Q3: 建表时如何避免破坏已有数据？

**A**: `create_or_upgrade_table` 先经 `table_exists`（information_schema 探测）分流：存在走 `upgrade_table`，不存在才走 `create_table`（覆盖了 trait 默认「先试建表失败再升级」实现——默认实现对已存在的表会打印注定不执行的 CREATE/COMMENT/INDEX 日志，误导排障）。升级路径内部经 `query_current_table_define` 回读库内结构后与目标定义 diff，仅生成加列/改列/索引等增量语句，不做删表重建；手工索引不在定义中也不会被删（仅报告提示）。

### Q4: 种子数据重复执行会重复插入吗？

**A**: 不会。`SeedDataConfig.conflict_columns` 配置冲突检测列后按 PostgreSQL `ON CONFLICT ... DO UPDATE` upsert；`SeedDataSummary` 还提供 `has_warnings()`（库内行数少于文件行数时提示）供上层核对。

### Q5: 如何在事务中执行 DDL？

**A**: `PgTableDefineExecutor::new(db_id, txn_id)` 与 `PgSeedDataExecutor::new(db_id, txn_id)` 的第二个参数接受事务 ID（由 `DatabaseManager` 事务上下文的 guard 提供），所有语句将携带同一 `txn_id` 执行，随外部事务一起提交/回滚。
