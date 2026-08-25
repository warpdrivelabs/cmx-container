# 常用列模板（8 段 JSON）+ seeddata 种子数据规范

> 本文件是 plugin-metadata-generator 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

常用列模板（8 段 JSON）+ seeddata 种子数据规范

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

#### 金额/小数列

```json
{
  "name": "unit_price",
  "label": "单价",
  "field_type": "Decimal",
  "is_primary_key": false,
  "is_nullable": false,
  "default_value": "0",
  "i18n": false,
  "precision": 18,
  "scale": 2,
  "db_type": "NUMERIC(18,2)",
  "ordinal": 8,
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

**执行策略**：
- 种子数据以 100 行为一批次执行
- 批次执行失败时自动降级为逐行执行
- 种子数据失败**不阻断**插件安装流程
- 执行完成后会校验数据库实际行数

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
