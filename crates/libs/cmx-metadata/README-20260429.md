# cmx-metadata

表定义元数据管理 crate，提供 JSON ↔ TableDefine ↔ DDL 的双向转换、增量 DDL 生成、多语言伴生表生成及 DDL 执行能力。

## 架构定位

```
cmx-plugin → cmx-metadata → cmx-database → cmx-core
```

- **cmx-core**：定义基础结构体（`TableDefine`、`ColumnDefine`、`FieldType` 等）和 trait
- **cmx-database**：底层 SQL 执行、连接池管理、事务管理
- **cmx-metadata**（本 crate）：元数据的具体处理逻辑
- **cmx-plugin**：插件注册表、ZIP 加载、签名验证

## 模块结构

```
src/
├── lib.rs              # 模块入口
├── error.rs            # MetadataError 统一错误类型
├── loader.rs           # JSON → TableDefine 加载
├── config.rs           # TableDefinesConfigManager（多配置拓扑排序加载）
├── i18n.rs             # 多语言伴生表生成（_i18n 后缀表）
├── executor.rs         # DDL 执行（PgTableDefineExecutor）
├── ddl/
│   ├── mod.rs          # DdlDialect trait + 便捷函数
│   ├── postgres.rs     # PostgresDdlDialect 实现
│   └── diff.rs         # DdlDiff 增量 DDL 生成
└── parser/
    ├── mod.rs          # DdlParser trait + 便捷函数
    └── postgres.rs     # PostgresDdlParser 实现
```

## 核心功能

### 1. JSON 加载（loader）

从 JSON 字符串或文件加载 `TableDefine`，支持三种根格式：

```rust
use cmx_metadata::loader::{table_define_from_str, table_defines_from_str, load_table_defines_from_path};

// 单表 JSON 对象
let table = table_define_from_str(r#"{ "table_name": "t1", "display_name": "表1", "columns": [] }"#)?;

// 多表：{ "tables": [...] } 或顶层数组 [...]
let tables = table_defines_from_str(json_str)?;

// 从文件加载
let tables = load_table_defines_from_path(Path::new("tables.json"))?;
```

### 2. 配置管理（config）

通过 `TableDefinesConfigManager` 管理多套建表配置，支持依赖声明和拓扑排序加载：

```rust
use cmx_metadata::config::TableDefinesConfigManager;

let manager = TableDefinesConfigManager::from_config_paths(&[
    Path::new("sys_tables_config.json"),
    Path::new("biz_tables_config.json"),
])?;

// 按依赖顺序加载所有表定义
let all_tables = manager.load_all_tables(base_path)?;

// 按配置名加载
let sys_tables = manager.load_tables_by_config_name(base_path, "sys_tables")?;
```

配置 JSON 格式：

```json
{
  "name": "domain_app_module",
  "description": "域-应用-模块三层结构表定义",
  "depends_on": [],
  "priority": 0,
  "files": ["domain_app_module_tables.json"]
}
```

### 3. DDL 生成（ddl）

通过 `DdlDialect` trait 生成数据库特定的 DDL，目前支持 PostgreSQL：

```rust
use cmx_metadata::ddl::{table_to_pg_ddl, tables_to_pg_ddl, table_to_pg_ddl_roundtrip};

// 生成单表 DDL（CREATE TABLE + INDEX + COMMENT）
let ddl = table_to_pg_ddl(&table)?;

// 生成多表 DDL
let ddl = tables_to_pg_ddl(&tables)?;

// roundtrip 模式：优先使用 col.db_type 保留原始类型
let ddl = table_to_pg_ddl_roundtrip(&table)?;
```

### 4. DDL 解析（parser）

将 DDL 文本解析还原为 `TableDefine`，支持 CREATE TABLE / CREATE INDEX / COMMENT ON：

```rust
use cmx_metadata::parser::{pg_ddl_to_table_define, pg_ddl_to_table_defines};

let table = pg_ddl_to_table_define(ddl_text)?;
let tables = pg_ddl_to_table_defines(ddl_text)?;
```

### 5. 增量 DDL（diff）

比对两组 `TableDefine`，生成 ALTER TABLE / ADD COLUMN / DROP COLUMN 等增量语句：

```rust
use cmx_metadata::ddl::diff::DdlDiff;
use cmx_metadata::ddl::postgres::PostgresDdlDialect;

let dialect = PostgresDdlDialect::default();
let stmts = DdlDiff::diff_to_ddl(&dialect, &old_tables, &new_tables)?;
```

### 6. 多语言伴生表（i18n）

为标记 `i18n: true` 的表生成后缀 `_i18n` 的伴生表，包含 `ref_id`、`locale` 和所有 `i18n` 列：

```rust
use cmx_metadata::i18n::derive_i18n_table_define;

if let Some(i18n_table) = derive_i18n_table_define(&base_table) {
    // i18n_table.table_name == "cmx_domain_i18n"
    // 含 ref_id, locale, name, description 等列
}
```

### 7. DDL 执行（executor）

通过 `PgTableDefineExecutor` 将 `TableDefine` 转为 DDL 后在数据库中执行：

```rust
use cmx_metadata::PgTableDefineExecutor;
use cmx_core::model::meta::base::TableDefineDbExecutor;

let executor = PgTableDefineExecutor::new("db_id", Some("txn_id".to_string()));
executor.create_table(&table_define)?;
```

## 测试

```bash
# 运行全部测试（31 单元测试 + 25 集成测试）
cargo test -p cmx-metadata
```

测试数据位于 `tests/` 目录：
- `domain_app_module_tables.json` — 域-应用-模块三张表的完整定义
- `domain_app_module_config.json` — 对应的建表配置文件

集成测试覆盖了完整的工作流：JSON 加载 → DDL 生成 → DDL 解析 → round-trip 验证 → 增量 DDL diff → i18n 伴生表 → 端到端 schema 演进。

## 依赖

| 依赖 | 用途 |
|------|------|
| `cmx-core` | 基础结构体（TableDefine 等） |
| `cmx-database` | 底层 SQL 执行 |
| `serde` / `serde_json` | JSON 序列化 |
| `thiserror` | 错误类型定义 |
| `chrono` | 日期时间处理 |
| `regex` | DDL 解析（正则匹配） |
| `tokio` | 异步运行时（executor 模块） |
