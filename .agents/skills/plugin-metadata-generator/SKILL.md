---
name: plugin-metadata-generator
description: 生成插件表结构元数据（metadata/）和种子数据（seeddata/），包括完整的字段类型映射、列定义规范和配置文件规范。当用户需要创建或修改插件的数据库表结构定义、种子数据、或 config 配置文件时必用。
---

# 插件元数据和种子数据生成器

>指导 AI 生成符合平台要求的表结构定义、种子数据和配置文件。

---

## 一、文件关系概览

### 1.1 三类文件的关系

```
config/{name}_config.json        — 配置入口（注册表定义和种子数据）
  ├── files → metadata/{name}_tables.json   — 表结构定义（DDL 元数据）
  └── seed_data → seeddata/{name}_seed.json — 种子数据（初始化数据）
```

### 1.2 加载流程

```
manifest.json → plugin.table_config_files → config/{name}_config.json
  ├── files[] → metadata/ 目录下查找并加载表定义 → 生成 DDL → 执行建表
  └── seed_data[] → 加载种子数据文件 → 生成 UPSERT → 执行插数据
```

---

## 二、config 配置文件规范

### 2.1 {name}_config.json 完整格式

```json
{
  "name": "account",
  "description": "会计科目表定义，支持六大类科目的树形管理",
  "depends_on": [],
  "priority": 0,
  "files": ["account_tables.json"],
  "seed_data": [
    {
      "table_name": "cmx_account",
      "file": "seeddata/account_seed.json",
      "conflict_columns": ["code"],
      "enabled": true
    }
  ]
}
```

### 2.2 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 配置名称（唯一标识），用于拓扑排序时的依赖引用 |
| `description` | string | 否 | 配置描述 |
| `depends_on` | string[] | 是 | 依赖的其他配置名称数组，空数组表示无依赖 |
| `priority` | integer | 是 | 优先级，数值越小越先执行，同级按文件顺序 |
| `files` | string[] | 是 | 引用 `metadata/` 目录下的表定义文件名（仅文件名，不含路径） |
| `seed_data` | array | 否 | 种子数据配置数组 |

### 2.3 seed_data 配置

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `table_name` | string | 是 | 目标表名，必须与 metadata 中定义的 table_name 一致 |
| `file` | string | 是 | 种子数据文件路径（相对于插件根目录） |
| `conflict_columns` | string[] | 是 | 冲突检测列，用于生成 `ON CONFLICT ... DO UPDATE` |
| `enabled` | boolean | 是 | 是否启用，`false` 时跳过该种子数据 |

### 2.4 拓扑排序规则

多个配置文件之间通过 `depends_on` 和 `priority` 决定执行顺序：

1. 先按 `depends_on` 进行拓扑排序（Kahn 算法）
2. 同级按 `priority` 排序（数值越小越先）
3. 先执行 `files`（建表），再执行 `seed_data`（插数据）

**示例**：config_b 依赖 config_a

```json
// config_a.json
{ "name": "config_a", "depends_on": [], "priority": 0, "files": ["tables_a.json"] }

// config_b.json
{ "name": "config_b", "depends_on": ["config_a"], "priority": 1, "files": ["tables_b.json"] }
```

---

## 三、metadata 表结构定义规范

### 3.1 顶层结构

文件位于 `metadata/` 目录，格式为：

```json
{
  "tables": [
    { /* TableDefine */ },
    { /* TableDefine */ }
  ]
}
```

一个文件可以定义多张表。

### 3.2 TableDefine 完整字段

```json
{
  "table_name": "cmx_account",
  "display_name": "会计科目",
  "version": 1,
  "comment": "会计科目表，采用树形层级结构组织",
  "primary_keys": ["id"],
  "i18n": false,
  "schema": "public",
  "columns": [ /* ColumnDefine[] */ ],
  "indexes": [ /* IndexDefine[] */ ],
  "is_partitioned": false,
  "partition_type": null,
  "partition_columns": [],
  "extensions": {}
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `table_name` | string | 是 | 表名，建议 `cmx_` 前缀 |
| `display_name` | string | 是 | 显示名称 |
| `version` | integer | 是 | 表定义版本号，用于增量升级 |
| `comment` | string | 是 | 表注释 |
| `primary_keys` | string[] | 是 | 主键列名数组 |
| `i18n` | boolean | 是 | 是否启用国际化（生成伴生表） |
| `schema` | string | 是 | 数据库 schema，通常为 `"public"` |
| `tablespace` | string | 否 | 表空间，通常不设置 |
| `columns` | array | 是 | 列定义数组，详见 §3.3 |
| `indexes` | array | 否 | 索引定义数组，详见 §3.6 |
| `is_partitioned` | boolean | 否 | 是否分区表，默认 false |
| `partition_type` | string/null | 否 | 分区类型 |
| `partition_columns` | array | 否 | 分区列 |
| `extensions` | object | 否 | 扩展字段，默认 `{}` |
| `create_time` | string/null | 否 | 创建时间（系统自动维护，生成时设为 `null`） |
| `update_time` | string/null | 否 | 更新时间（系统自动维护，生成时设为 `null`） |

### 3.3 ColumnDefine 完整字段

```json
{
  "name": "code",
  "label": "科目编码",
  "field_type": "String",
  "is_primary_key": false,
  "is_nullable": false,
  "default_value": null,
  "i18n": false,
  "length": 32,
  "db_type": "VARCHAR(32)",
  "ordinal": 2,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 列名（数据库字段名） |
| `label` | string | 是 | 显示名称（中文） |
| `field_type` | string | 是 | 字段类型枚举，详见 §3.4 |
| `is_primary_key` | boolean | 是 | 是否为主键列 |
| `is_nullable` | boolean | 是 | 是否允许为 NULL |
| `default_value` | any | 是 | 默认值，无则为 `null` |
| `i18n` | boolean | 是 | 该列是否参与国际化 |
| `length` | integer | 条件必填 | 仅 `String` 类型需要，表示 VARCHAR 长度 |
| `precision` | integer | 条件必填 | 数值总精度（总位数），仅 `Decimal` 类型需要 |
| `scale` | integer | 条件必填 | 数值小数位数，仅 `Decimal` 类型需要 |
| `db_type` | string | 是 | PostgreSQL 类型声明，详见 §3.5 |
| `ordinal` | integer | 是 | 列顺序号，从 1 开始连续递增 |
| `create_time` | string/null | 否 | 创建时间（系统自动维护，生成时设为 `null`） |
| `update_time` | string/null | 否 | 更新时间（系统自动维护，生成时设为 `null`） |
| `is_foreign_key` | boolean | 是 | 是否为**逻辑外键**（仅元数据标记，供导入校验/文档/关系展示用；**不会生成 DDL `FOREIGN KEY` 约束**——项目全局禁物理外键，靠普通列 + 索引维护关联） |
| `foreign_key_table` | string/null | 条件必填 | 逻辑外键引用的表名（`is_foreign_key=true` 时必填） |
| `foreign_key_column` | string/null | 条件必填 | 逻辑外键引用的列名（`is_foreign_key=true` 时必填） |
| `extensions` | object | 否 | 扩展字段，默认 `{}` |

> **与 pg-table-generator 的主键口径差异**：插件 metadata 表主键惯例 `field_type: "Int" / db_type: "BIGINT"`（自增系，由插件框架维护）；手工维护的系统表 DDL 主键为 `varchar(64)`（见 [pg-table-generator](../pg-table-generator/SKILL.md)）。两类表各有惯例，不要互相套用。

### 3.4 FieldType 枚举及对应 DB 类型

| field_type | PostgreSQL db_type | 说明 | 需要额外字段 |
|---|---|---|---|
| `String` | `VARCHAR(length)` | 字符串 | **length**（必填） |
| `Int` | `BIGINT` 或 `INT` | 整数 | — |
| `Float` | `DOUBLE PRECISION` | 浮点数 | — |
| `Decimal` | `NUMERIC(precision, scale)` | 精确小数 | **precision + scale** |
| `DateTime` | `TIMESTAMP WITH TIME ZONE` | 日期时间 | — |
| `Date` | `DATE` | 日期 | — |
| `Bool` | `BOOLEAN` | 布尔值 | — |
| `Text` | `TEXT` | 长文本 | — |
| `Binary` | `BYTEA` | 二进制数据 | — |
| `Array` | `JSONB` | 数组 | — |
| `Json` | `JSONB` | JSON 数据 | — |
| `Uuid` | `UUID` | UUID 标识符 | — |
| `Unknown` | `TEXT` | 未知类型 | — |

### 3.5 db_type 生成规则

根据 `field_type` 自动生成 `db_type`：

| field_type | db_type 生成规则 | 示例 |
|---|---|---|
| `String` | `"VARCHAR(length)"`，length 为必填字段 | `"VARCHAR(32)"` |
| `Int` | 通常用 `"BIGINT"`，小范围整数可用 `"INT"` | `"BIGINT"` |
| `Float` | 固定 `"DOUBLE PRECISION"` | `"DOUBLE PRECISION"` |
| `Decimal` | `"NUMERIC(precision, scale)"`，precision 默认 18，scale 默认 2 | `"NUMERIC(18,2)"` |
| `DateTime` | 固定 `"TIMESTAMP WITH TIME ZONE"` | `"TIMESTAMP WITH TIME ZONE"` |
| `Date` | 固定 `"DATE"` | `"DATE"` |
| `Bool` | 固定 `"BOOLEAN"` | `"BOOLEAN"` |
| `Text` | 固定 `"TEXT"` | `"TEXT"` |
| `Binary` | 固定 `"BYTEA"` | `"BYTEA"` |
| `Array` | 固定 `"JSONB"` | `"JSONB"` |
| `Json` | 固定 `"JSONB"` | `"JSONB"` |
| `Uuid` | 固定 `"UUID"` | `"UUID"` |

### 3.6 索引定义规范

```json
"indexes": [
  { "name": "uk_cmx_account_code", "columns": ["code"], "kind": "unique" },
  { "name": "idx_cmx_account_type", "columns": ["account_type"], "kind": "normal" }
]
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 索引名称，唯一索引用 `uk_` 前缀，普通索引用 `idx_` 前缀 |
| `columns` | string[] | 是 | 索引列名数组，支持多列组合索引 |
| `kind` | string | 是 | 索引类型：`"unique"`（唯一）或 `"normal"`（普通） |

**命名规范**：`{uk|idx}_{表名}_{列名组合}`

### 3.7 外键定义规范

当列是外键时，需要同时设置以下三个字段：

```json
{
  "name": "domain_id",
  "field_type": "Int",
  "is_foreign_key": true,
  "foreign_key_table": "cmx_domain",
  "foreign_key_column": "id"
}
```

### 3.8 常用列模板

8 段可直接复制的列定义 JSON 模板（id 主键列 / code 业务编码列 / name 名称列 / 审计字段组 / 分级字段组 / 外键列 / status 状态列 / JSON 扩展列）见 [references/column-templates-and-seeddata.md](references/column-templates-and-seeddata.md)。

---

## 四、seeddata 种子数据规范

JSON 与 CSV 双格式、字段类型值格式、`conflict_columns` 选取原则、种子数据命名规范的完整说明见 [references/column-templates-and-seeddata.md](references/column-templates-and-seeddata.md)。**速记**：JSON 用数组对象、CSV 表头=列名；`conflict_columns` 选业务唯一键（常是 code）实现幂等导入。

---
## 五、生成流程

### 5.1 操作步骤

按以下顺序执行：

**步骤1**：根据业务需求设计表结构

确定表名、列定义、索引、外键等。参考 §3.8 的列模板快速构建。

**步骤2**：生成 `metadata/{name}_tables.json`

按照 §3 的规范生成表结构定义文件。确保：
- `ordinal` 从 1 开始连续递增
- `primary_keys` 中的列标记了 `is_primary_key: true`
- 外键列标记了 `is_foreign_key: true` 并填写关联信息
- `db_type` 与 `field_type` 匹配（参考 §3.5）

**步骤3**：生成 `seeddata/{table_name}_seed.json`

按照 §4 的规范生成种子数据。确保：
- 键名与 `columns` 中的 `name` 完全一致
- 值类型与 `field_type` 匹配
- 数据量适中（建议不超过 100 条）

**步骤4**：创建或更新 `config/{name}_config.json`

在配置文件中注册表定义和种子数据：
- `files` 字段添加表定义文件名（仅文件名，不含路径）
- `seed_data` 数组添加种子数据配置
- 如果是新增配置文件，需要在 `manifest.json` 的 `table_config_files` 中注册

### 5.2 校验清单

生成完成后，逐项校验：

- [ ] `columns` 中 `ordinal` 从 1 开始连续递增，无跳跃
- [ ] `primary_keys` 中的列都已标记 `is_primary_key: true`
- [ ] `is_primary_key: true` 的列 `is_nullable` 为 `false`
- [ ] 外键列标记了 `is_foreign_key: true` 且 `foreign_key_table` 和 `foreign_key_column` 已填写
- [ ] `String` 类型列有 `length` 字段且 `db_type` 为 `VARCHAR(length)`
- [ ] `db_type` 与 `field_type` 匹配（参考 §3.5）
- [ ] 索引命名符合规范（`uk_` 或 `idx_` 前缀）
- [ ] 种子数据的键名与 `columns` 中的 `name` 一致
- [ ] 种子数据的值类型与 `field_type` 一致
- [ ] `conflict_columns` 中的列在 `indexes` 中有对应的唯一索引
- [ ] `config` 中的 `files` 引用的文件名在 `metadata/` 目录下存在
- [ ] `config` 中的 `seed_data.file` 引用的文件路径正确
