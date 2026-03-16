# 表定义 JSON 解析与 DDL 生成开发文档

> 本文档已优化更新，完整版见 `docs/表定义JSON解析与DDL生成开发文档.md`

## 1. 架构概览

按职责拆分为三个 crate：

| Crate | 职责 |
|-------|------|
| **cmx-core** | 基础结构体定义（TableDefine、ColumnDefine、FieldType 等）+ 错误类型 + DB 执行 trait |
| **cmx-metadata** | JSON 加载、DDL 生成/解析、增量 DDL diff、i18n 伴生表、配置管理 |
| **cmx-database** | DDL 实际执行（execute_ddl_by_ids）+ PgTableDefineExecutor 实现 |

## 2. cmx-metadata 模块结构

```
crates/libs/cmx-metadata/src/
├── lib.rs              # 模块导出
├── error.rs            # MetadataError 错误类型
├── loader.rs           # JSON 加载（table_define_from_str 等）
├── config.rs           # TableDefinesConfig / ConfigManager
├── i18n.rs             # derive_i18n_table_define
├── ddl/
│   ├── mod.rs          # DdlDialect trait + 便捷函数
│   ├── postgres.rs     # PostgresDdlDialect 实现
│   └── diff.rs         # 增量 DDL 生成（DdlDiff）
└── parser/
    ├── mod.rs          # DdlParser trait + 便捷函数
    └── postgres.rs     # PostgresDdlParser 实现
```

## 3. 核心 Trait 设计

### 3.1 DdlDialect（DDL 生成）
- `map_column_type()` — FieldType → SQL 类型
- `generate_create_table()` — CREATE TABLE
- `generate_create_indexes()` — CREATE INDEX
- `generate_comments()` — COMMENT ON TABLE/COLUMN
- `generate_full_ddl()` — 完整 DDL（默认实现）
- `generate_add_column()` / `generate_drop_column()` / `generate_alter_column()` — ALTER TABLE
- `generate_drop_table()` — DROP TABLE

### 3.2 DdlParser（DDL 解析）
- `parse_create_table()` — 解析单条 CREATE TABLE
- `parse_ddl()` — 解析完整 DDL（多语句）

### 3.3 DdlDiff（增量 DDL）
- `diff()` — 比对两组 TableDefine → Vec<TableChange>
- `changes_to_ddl()` — 变更列表 → DDL 语句
- `diff_to_ddl()` — 一步到位

## 4. 双向转换矩阵

| 源 | 目标 | 实现方式 |
|----|------|----------|
| JSON | TableDefine | serde（loader.rs） |
| TableDefine | JSON | serde（Serialize） |
| TableDefine | DDL | DdlDialect（PG 已实现） |
| DDL | TableDefine | DdlParser（PG 已实现） |
| 旧 TableDefine + 新 TableDefine | 增量 DDL | DdlDiff |

## 5. FieldType ↔ PostgreSQL 类型映射

| FieldType | PostgreSQL | 反向匹配 |
|-----------|------------|----------|
| String | VARCHAR(n) / TEXT | VARCHAR(n), CHARACTER VARYING(n) |
| Int | BIGINT | BIGINT, INT8, INTEGER, INT4, SMALLINT |
| Float | DOUBLE PRECISION | DOUBLE PRECISION, FLOAT8, REAL |
| Decimal | NUMERIC(p,s) | NUMERIC(p,s), DECIMAL(p,s) |
| DateTime | TIMESTAMP WITH TIME ZONE | TIMESTAMPTZ |
| Date | DATE | DATE |
| Bool | BOOLEAN | BOOLEAN, BOOL |
| Text | TEXT | TEXT |
| Binary | BYTEA | BYTEA |
| Array | JSONB | — |
| Json | JSONB | JSONB, JSON |
| Uuid | UUID | UUID |

## 6. 设计决策

- **保留 db_type**：支持 DDL round-trip（跨数据库迁移场景）
- **外键不生成 FK 约束**：避免多表批量建表时的循环依赖问题
- **列注释**：生成 COMMENT ON COLUMN（label 字段）
- **prefer_db_type 模式**：优先使用 col.db_type（round-trip）

## 7. 扩展性

添加新数据库支持只需：
1. 实现 `DdlDialect` trait（如 `MySqlDdlDialect`）
2. 实现 `DdlParser` trait（如 `MySqlDdlParser`）
3. 在 `ddl/` 和 `parser/` 下新建模块文件

## 8. 测试

cmx-metadata 共 31 个单元测试全部通过，覆盖：
- JSON 加载（3 种格式）
- DDL 生成（类型映射、CREATE TABLE、INDEX、COMMENT、ALTER、DROP）
- DDL 解析（所有 PG 类型、INDEX、COMMENT）
- 往返测试（TableDefine → DDL → TableDefine）
- 增量 DDL（新增/删除/修改 表/列/索引）
