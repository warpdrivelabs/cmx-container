---
name: "plugin-metadata-generator"
description: "生成插件表结构元数据（metadata/）和种子数据（seeddata/），包括完整的字段类型映射、列定义规范和配置文件规范。Invoke when 用户需要创建或修改插件的数据库表结构定义、种子数据、或 config 配置文件时。"
---

# 插件元数据和种子数据生成器

> 根据 cmx-metadata 模块的源码规范，指导 AI 生成符合平台要求的表结构定义、种子数据和配置文件。

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
| `columns` | array | 是 | 列定义数组，详见 §3.3 |
| `indexes` | array | 否 | 索引定义数组，详见 §3.6 |
| `is_partitioned` | boolean | 否 | 是否分区表，默认 false |
| `partition_type` | string/null | 否 | 分区类型 |
| `partition_columns` | array | 否 | 分区列 |
| `extensions` | object | 否 | 扩展字段，默认 `{}` |

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
| `db_type` | string | 是 | PostgreSQL 类型声明，详见 §3.5 |
| `ordinal` | integer | 是 | 列顺序号，从 1 开始连续递增 |
| `is_foreign_key` | boolean | 是 | 是否为外键 |
| `foreign_key_table` | string/null | 条件必填 | 外键引用的表名（`is_foreign_key=true` 时必填） |
| `foreign_key_column` | string/null | 条件必填 | 外键引用的列名（`is_foreign_key=true` 时必填） |
| `extensions` | object | 否 | 扩展字段，默认 `{}` |

### 3.4 FieldType 枚举及对应 DB 类型

| field_type | PostgreSQL db_type | 说明 | 需要额外字段 |
|---|---|---|---|
| `Int` | `BIGINT` 或 `INT` | 整数 | — |
| `Float` | `DOUBLE PRECISION` | 浮点数 | — |
| `Decimal` | `NUMERIC(precision, scale)` | 精确小数 | — |
| `String` | `VARCHAR(length)` | 字符串 | **length**（必填） |
| `Text` | `TEXT` | 长文本 | — |
| `Bool` | `BOOLEAN` | 布尔值 | — |
| `Date` | `DATE` | 日期 | — |
| `DateTime` | `TIMESTAMP WITH TIME ZONE` | 日期时间 | — |
| `Json` | `JSONB` | JSON 数据 | — |
| `Binary` | `BYTEA` | 二进制数据 | — |
| `Array` | `JSONB` | 数组 | — |
| `Uuid` | `UUID` | UUID 标识符 | — |
| `Unknown` | `TEXT` | 未知类型 | — |

### 3.5 db_type 生成规则

根据 `field_type` 自动生成 `db_type`：

| field_type | db_type 生成规则 | 示例 |
|---|---|---|
| `Int` | 通常用 `"BIGINT"`，小范围整数可用 `"INT"` | `"BIGINT"` |
| `Float` | 固定 `"DOUBLE PRECISION"` | `"DOUBLE PRECISION"` |
| `Decimal` | `"NUMERIC(precision, scale)"` | `"NUMERIC(18,2)"` |
| `String` | `"VARCHAR(length)"`，length 为必填字段 | `"VARCHAR(32)"` |
| `Text` | 固定 `"TEXT"` | `"TEXT"` |
| `Bool` | 固定 `"BOOLEAN"` | `"BOOLEAN"` |
| `Date` | 固定 `"DATE"` | `"DATE"` |
| `DateTime` | 固定 `"TIMESTAMP WITH TIME ZONE"` | `"TIMESTAMP WITH TIME ZONE"` |
| `Json` | 固定 `"JSONB"` | `"JSONB"` |
| `Binary` | 固定 `"BYTEA"` | `"BYTEA"` |
| `Array` | 固定 `"JSONB"` | `"JSONB"` |
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

#### 主键列

```json
{
  "name": "id",
  "label": "主键",
  "field_type": "Int",
  "is_primary_key": true,
  "is_nullable": false,
  "default_value": null,
  "i18n": false,
  "db_type": "BIGINT",
  "ordinal": 1,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

#### 编码列

```json
{
  "name": "code",
  "label": "编码",
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

#### 名称列（支持国际化）

```json
{
  "name": "name",
  "label": "名称",
  "field_type": "String",
  "is_primary_key": false,
  "is_nullable": false,
  "default_value": null,
  "i18n": true,
  "length": 64,
  "db_type": "VARCHAR(64)",
  "ordinal": 3,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

#### 外键列

```json
{
  "name": "parent_id",
  "label": "父级ID",
  "field_type": "Int",
  "is_primary_key": false,
  "is_nullable": true,
  "default_value": null,
  "i18n": false,
  "db_type": "BIGINT",
  "ordinal": 4,
  "is_foreign_key": true,
  "foreign_key_table": "cmx_account",
  "foreign_key_column": "id",
  "extensions": {}
}
```

#### 排序号列

```json
{
  "name": "sort_order",
  "label": "排序号",
  "field_type": "Int",
  "is_primary_key": false,
  "is_nullable": true,
  "default_value": "0",
  "i18n": false,
  "db_type": "BIGINT",
  "ordinal": 10,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

#### 布尔标志列

```json
{
  "name": "is_enabled",
  "label": "是否启用",
  "field_type": "Bool",
  "is_primary_key": false,
  "is_nullable": false,
  "default_value": "true",
  "i18n": false,
  "db_type": "BOOLEAN",
  "ordinal": 9,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

#### 创建时间/更新时间列

```json
{
  "name": "create_time",
  "label": "创建时间",
  "field_type": "DateTime",
  "is_primary_key": false,
  "is_nullable": true,
  "default_value": null,
  "i18n": false,
  "db_type": "TIMESTAMP WITH TIME ZONE",
  "ordinal": 12,
  "is_foreign_key": false,
  "foreign_key_table": null,
  "foreign_key_column": null,
  "extensions": {}
}
```

---

## 四、seeddata 种子数据规范

### 4.1 JSON 格式

顶层是一个数组，每个元素是一个对象（键为列名，值为数据）：

```json
[
  { "id": 1, "code": "1001", "name": "库存现金", "parent_id": null, "is_enabled": true },
  { "id": 2, "code": "1002", "name": "银行存款", "parent_id": null, "is_enabled": true }
]
```

**规则**：

- 键名必须与 `columns` 中的 `name` 完全一致
- `null` 表示插入 NULL
- 省略某个字段则使用列定义中的 `default_value`，无默认值时为 NULL
- 所有非可省略字段建议显式提供

### 4.2 CSV 格式

首行为列名（表头），后续为数据行：

```csv
id,code,name,is_enabled
1,1001,库存现金,true
2,1002,银行存款,true
```

**规则**：

- 空字符串单元格：跳过该列（让 DML 生成器根据默认值决定）
- 类型自动转换规则见下表

### 4.3 字段类型值格式

| field_type | JSON 值格式 | CSV 值格式 | 生成 SQL 示例 |
|---|---|---|---|
| `Int` | `1` 或 `1.0` | `1` | `1` |
| `Float` | `3.14` | `3.14` | `3.14` |
| `Decimal` | `"3.14159"`（字符串） | `3.14159` | `'3.14159'` |
| `String` | `"财务域"` | `财务域` | `'财务域'` |
| `Text` | `"长文本内容"` | `长文本内容` | `'长文本内容'` |
| `Bool` | `true` / `false` | `true/1/yes/on` 或 `false/0/no/off` | `true` / `false` |
| `Date` | `"2026-04-20"` | `2026-04-20` | `'2026-04-20'::date` |
| `DateTime` | `"2026-04-20T10:30:00Z"` 或时间戳数字 | 同左 | `'...'::timestamptz` 或 `to_timestamp(...)` |
| `Json` | `{"key": "value"}`（JSON对象） | 不支持复杂JSON | `'...'::jsonb` |
| `Uuid` | `"550e8400-..."` | `550e8400-...` | `'...'::uuid` |

**时间戳自动判断**：数值 > 1,000,000,000,000 视为毫秒，否则视为秒。

**NULL 处理**：
- JSON：`null` 插入 NULL；省略字段使用默认值
- CSV：空单元格使用默认值

### 4.4 conflict_columns 选取原则

`conflict_columns` 用于生成 UPSERT 语句（`INSERT ... ON CONFLICT ... DO UPDATE`）：

- 选择业务上唯一的列（如 `code`）
- 组合唯一键用数组（如 `["domain_id", "code"]`）
- 冲突列通常与表定义中的唯一索引对应
- 当所有列都是冲突列时，自动生成 `DO NOTHING`

### 4.5 种子数据命名规范

```
seeddata/
├── {table_name}_seed.json       # 单表数据（JSON）
├── {table_name}_seed.csv        # 单表数据（CSV）
└── {module}_{table_name}_seed.json  # 带模块前缀
```

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
