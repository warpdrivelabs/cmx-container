---
name: "pg-table-generator"
description: "Generates PostgreSQL DDL table definitions with standard audit fields, proper comments, and nullable constraints. Invoke when user asks to create/generate PostgreSQL table structure, DDL, or schema definition."
---

# PostgreSQL 表结构生成器

根据用户需求生成 PostgreSQL DDL 表定义。

## 硬约束（必须遵守）

1. **主键不用** **`CONSTRAINT`**：`CREATE TABLE` 内部主键直接写 `PRIMARY KEY (id)`，**禁止** `CONSTRAINT pk_xxx PRIMARY KEY (id)` 这种命名形式。
2. **禁止外键约束**：**禁止**生成任何 `FOREIGN KEY ... REFERENCES ...` 语句。如有关联字段，保留字段并用 `CREATE INDEX` 替代。
3. **索引必须单独创建**：所有 `UNIQUE` 约束、普通索引、唯一索引都必须用 `CREATE UNIQUE INDEX` / `CREATE INDEX` 写在 `CREATE TABLE` 之后，**禁止**在 `CREATE TABLE` 内部使用 `CONSTRAINT ... UNIQUE (...)` 或 `UNIQUE (...)` 列约束/表约束。
4. **`CREATE TABLE`** **内部不出现任何** **`CONSTRAINT`** **子句**：除了上面 1/3 点的限制，`CREATE TABLE (...)` 体内不得出现 `CONSTRAINT ...` 形式。
5. **唯一索引必须为部分唯一索引（Partial Unique Index）**：所有 `CREATE UNIQUE INDEX` 语句必须包含 `WHERE archived = 0` 条件，**仅对"未归档"记录**强制唯一。已归档（`archived = 1`）的记录不参与唯一性检查，允许存在同名字段的历史快照。

## 标识符命名规则

1. **不使用双引号**：表名、字段名、约束名、索引名等所有标识符均不使用双引号包裹，直接使用小写字母和下划线命名。
2. **不指定 schema**：生成的 SQL 不包含 `schema_name` 前缀，直接使用表名（即默认 schema 为 `public`，由数据库 search\_path 决定）。

### 标准审计字段（所有表必需）

所有表必须包含以下标准字段（除 `id` 外均放在表最后）：

| 字段名          | 类型           | 约束                         | 说明    |
| ------------ | ------------ | -------------------------- | ----- |
| id           | varchar(64)  | NOT NULL                   | 主键    |
| archived     | int4         | DEFAULT 0                  | 是否归档  |
| create\_time | timestamp    | DEFAULT CURRENT\_TIMESTAMP | 创建时间  |
| update\_time | timestamp    | DEFAULT CURRENT\_TIMESTAMP | 更新时间  |
| create\_by   | varchar(100) | -                          | 创建人ID |
| create\_name | varchar(100) | -                          | 创建人姓名 |
| update\_by   | varchar(100) | -                          | 更新人ID |
| update\_name | varchar(100) | -                          | 更新人姓名 |

### 标准分级字段（树形/层级结构表必需）

当表具有树形或层级结构时，必须包含以下分级字段（放在业务字段之后、审计字段之前）：

| 字段名        | 类型            | 约束        | 说明                                                  |
| ---------- | ------------- | --------- | --------------------------------------------------- |
| leaf       | int4          | DEFAULT 0 | 是否明细：1-是叶子节点，0-非叶子节点                                |
| depth      | int4          | DEFAULT 1 | 级数：根节点为1，逐层递增                                       |
| parent\_id | varchar(64)   | -         | 父节点ID，根节点为空                                         |
| id\_path   | varchar(1000) | -         | ID全路径，以/分隔，如 /root\_id/parent\_id/current\_id       |
| code\_path | varchar(1000) | -         | 编号全路径，以/分隔，如 /ROOT\_CODE/PARENT\_CODE/CURRENT\_CODE |

**使用场景**：

- 组织架构、菜单、分类、地区等树形结构数据
- 需要快速查询子树、祖先链的场景
- 用户未明确说明是否为树形表时，**不添加**这些字段

### 标准扩展信息字段（可选）

当表需要存储额外的动态业务属性时，可包含以下扩展字段（放在审计字段之后）：

| 字段名             | 类型   | 约束 | 说明                     |
| --------------- | ---- | -- | ---------------------- |
| ext\_attributes | text | -  | 扩展属性，存储 JSON 格式的额外业务属性 |

**使用场景**：
- 表需要灵活扩展、存储不确定的业务属性时添加
- 用户未明确要求时，**不添加**此字段

### 字段约束规则

1. **必填字段**：仅主键字段（id）和需要参与唯一索引的字段设置为 NOT NULL。
2. **可选字段**：所有非主键、非唯一索引字段均为可选（可NULL）。
3. **唯一性约束**：**不**在列上写 `UNIQUE`，**不**在表内写表级 `UNIQUE(...)`。唯一性由 `CREATE UNIQUE INDEX` 在 `CREATE TABLE` 之外承担（见硬约束 3）。
4. **主键约束**：列上写 `NOT NULL`，表内末尾用 `PRIMARY KEY (id)`，**不**写 `CONSTRAINT pk_xxx PRIMARY KEY (id)`（见硬约束 1）。
5. **部分唯一索引**（见硬约束 5）：所有 `CREATE UNIQUE INDEX` **必须**追加 `WHERE archived = 0` 条件。

### 部分唯一索引（Partial Unique Index）

**硬约束 5 的展开说明**：

#### 强制写法

```sql
-- ✅ 正确：部分唯一索引，仅约束未归档记录
CREATE UNIQUE INDEX uk_table_name_field ON table_name (field) WHERE archived = 0;

-- ❌ 错误：完整唯一索引（不允许）
CREATE UNIQUE INDEX uk_table_name_field ON table_name (field);
```

#### 业务语义

- `archived = 0`（未归档）的记录必须唯一
- `archived = 1`（已归档）的记录不参与唯一性检查
- 允许场景：先归档 `code = 'A'` 的记录（保留历史），再新建 `code = 'A'` 的新记录

#### 适用范围

- 所有具有 `archived` 字段的表（系统标准表均满足）
- 所有 `CREATE UNIQUE INDEX` 语句（无论单列还是复合列）
- 复合唯一索引：`CREATE UNIQUE INDEX uk_xxx ON table (col1, col2) WHERE archived = 0;`

#### 注意事项

- `WHERE archived = 0` 是硬性要求，**不允许省略**
- 普通索引（`CREATE INDEX`）**不**需要 `WHERE` 子句
- 迁移文件中也必须使用部分唯一索引，与 `init_ddl.sql` 保持一致

### 注释规则

**必须生成完整的 COMMENT 语句**，格式如下：

```sql
-- 表注释
COMMENT ON TABLE table_name IS '表业务含义';

-- 字段注释
COMMENT ON COLUMN table_name.field_name IS '字段用途';
```

**注释命名建议**：

- 表注释：简洁明了，如 `'用户表'`、`'订单表'`
- 主键字段：`'ID'`
- 状态字段：`'状态：0-禁用，1-启用'`
- 时间字段：`'创建时间'`、`'更新时间'`
- 关联字段（无外键）：`'关联XXX表ID'`，并在外层用 `CREATE INDEX` 加速查询

## 输出格式

> 严格遵守"硬约束"4 条：主键用 `PRIMARY KEY (id)`；`CREATE TABLE` 体内**不**出现 `CONSTRAINT`；**不**生成 `FOREIGN KEY`；所有唯一/普通索引单独写在表外。

```sql
-- 表注释
CREATE TABLE table_name (
    -- 业务字段（主键在前）
    id varchar(64) NOT NULL,
    field_name varchar(255),
    -- ... 其他业务字段 ...

    -- 标准分级字段（仅树形/层级结构表包含）
    leaf int4 DEFAULT 1,
    depth int4 DEFAULT 1,
    parent_id varchar(64),
    id_path varchar(1000),
    code_path varchar(1000),

    -- 标准审计字段（除id外按此顺序排列）
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    -- 标准扩展信息字段（可选，按需添加）
    ext_attributes text,

    PRIMARY KEY (id)
);

-- 唯一索引（如有，单独写在表外，部分唯一索引：仅约束未归档记录）
CREATE UNIQUE INDEX uk_table_name_field ON table_name (field_name) WHERE archived = 0;

-- 分级字段索引（仅树形表，单独写在表外）
CREATE INDEX idx_table_name_parent_id ON table_name (parent_id);

COMMENT ON TABLE table_name IS '表业务注释';
COMMENT ON COLUMN table_name.id IS '主键ID';
-- ... 其他字段注释 ...
```

## 使用示例

### 示例1：简单表

用户输入：

```
生成用户表 user，包含字段：username(varchar(50)), email(varchar(100)), phone(varchar(20))
```

输出：

```sql
-- 用户表
CREATE TABLE user (
    -- 主键
    id varchar(64) NOT NULL,

    -- 业务字段
    username varchar(50),
    email varchar(100),
    phone varchar(20),

    -- 标准审计字段
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    -- 标准扩展信息字段（可选，按需添加）
    ext_attributes text,

    PRIMARY KEY (id)
);

COMMENT ON TABLE user IS '用户表';
COMMENT ON COLUMN user.id IS '主键ID';
COMMENT ON COLUMN user.username IS '用户名';
COMMENT ON COLUMN user.email IS '邮箱';
COMMENT ON COLUMN user.phone IS '手机号';
COMMENT ON COLUMN user.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN user.create_time IS '创建时间';
COMMENT ON COLUMN user.update_time IS '更新时间';
COMMENT ON COLUMN user.create_by IS '创建人ID';
COMMENT ON COLUMN user.create_name IS '创建人姓名';
COMMENT ON COLUMN user.update_by IS '更新人ID';
COMMENT ON COLUMN user.update_name IS '更新人姓名';
COMMENT ON COLUMN user.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';
```

### 示例2：带唯一索引的表

用户输入：

```
生成商品表 product，包含：name(varchar(100)必填), code(varchar(50)唯一索引必填), price(numeric(10,2)), stock(int4)
```

输出：

```sql
-- 商品表
CREATE TABLE product (
    -- 主键
    id varchar(64) NOT NULL,

    -- 业务字段
    name varchar(100) NOT NULL,
    code varchar(50) NOT NULL,
    price numeric(10,2),
    stock int4,

    -- 标准审计字段
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    -- 标准扩展信息字段（可选，按需添加）
    ext_attributes text,

    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_product_code ON product (code) WHERE archived = 0;

COMMENT ON TABLE product IS '商品表';
COMMENT ON COLUMN product.id IS '主键ID';
COMMENT ON COLUMN product.name IS '商品名称';
COMMENT ON COLUMN product.code IS '商品编码';
COMMENT ON COLUMN product.price IS '商品价格';
COMMENT ON COLUMN product.stock IS '库存数量';
COMMENT ON COLUMN product.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN product.create_time IS '创建时间';
COMMENT ON COLUMN product.update_time IS '更新时间';
COMMENT ON COLUMN product.create_by IS '创建人ID';
COMMENT ON COLUMN product.create_name IS '创建人姓名';
COMMENT ON COLUMN product.update_by IS '更新人ID';
COMMENT ON COLUMN product.update_name IS '更新人姓名';
COMMENT ON COLUMN product.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';
```

### 示例3：订单表

用户输入：

```
生成订单表 order，包含：order_no(varchar(50)唯一索引), amount(numeric(12,2)), status(int4)
```

输出：

```sql
-- 订单表
CREATE TABLE order (
    -- 主键
    id varchar(64) NOT NULL,

    -- 业务字段
    order_no varchar(50) NOT NULL,
    amount numeric(12,2),
    status int4,

    -- 标准审计字段
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX uk_order_order_no ON order (order_no) WHERE archived = 0;

COMMENT ON TABLE order IS '订单表';
COMMENT ON COLUMN order.id IS '主键ID';
COMMENT ON COLUMN order.order_no IS '订单编号';
COMMENT ON COLUMN order.amount IS '订单金额';
COMMENT ON COLUMN order.status IS '订单状态';
COMMENT ON COLUMN order.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN order.create_time IS '创建时间';
COMMENT ON COLUMN order.update_time IS '更新时间';
COMMENT ON COLUMN order.create_by IS '创建人ID';
COMMENT ON COLUMN order.create_name IS '创建人姓名';
COMMENT ON COLUMN order.update_by IS '更新人ID';
COMMENT ON COLUMN order.update_name IS '更新人姓名';
```

### 示例4：含关联字段但不使用外键的表

> 演示硬约束 2 / 3：保留关联字段 `user_id`，**不**写 `FOREIGN KEY`，外层用 `CREATE INDEX` 加速查询。

用户输入：

```
生成订单项表 order_item，包含：order_id(varchar(64) 关联order表id), user_id(varchar(64) 关联user表id), product_id(varchar(64)), quantity(int4)
```

输出：

```sql
-- 订单项表
CREATE TABLE order_item (
    -- 主键
    id varchar(64) NOT NULL,

    -- 业务字段（含关联字段，不加外键）
    order_id varchar(64) NOT NULL,
    user_id varchar(64),
    product_id varchar(64),
    quantity int4,

    -- 标准审计字段
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    -- 标准扩展信息字段（可选，按需添加）
    ext_attributes text,

    PRIMARY KEY (id)
);

-- 关联字段索引（外层单独创建，不使用外键）
CREATE INDEX idx_order_item_order_id ON order_item (order_id);
CREATE INDEX idx_order_item_user_id ON order_item (user_id);
CREATE INDEX idx_order_item_product_id ON order_item (product_id);

COMMENT ON TABLE order_item IS '订单项表';
COMMENT ON COLUMN order_item.id IS '主键ID';
COMMENT ON COLUMN order_item.order_id IS '关联订单表ID';
COMMENT ON COLUMN order_item.user_id IS '关联用户表ID';
COMMENT ON COLUMN order_item.product_id IS '关联商品表ID';
COMMENT ON COLUMN order_item.quantity IS '数量';
COMMENT ON COLUMN order_item.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN order_item.create_time IS '创建时间';
COMMENT ON COLUMN order_item.update_time IS '更新时间';
COMMENT ON COLUMN order_item.create_by IS '创建人ID';
COMMENT ON COLUMN order_item.create_name IS '创建人姓名';
COMMENT ON COLUMN order_item.update_by IS '更新人ID';
COMMENT ON COLUMN order_item.update_name IS '更新人姓名';
COMMENT ON COLUMN order_item.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';
```

## 字段类型参考

| 业务场景   | 推荐类型                          |
| ------ | ----------------------------- |
| 短文本/枚举 | varchar(N)                    |
| 长文本    | text                          |
| 整数     | int4                          |
| 长整数    | int8                          |
| 小数     | numeric(p,s)                  |
| 金额     | numeric(12,2) 或 numeric(18,2) |
| 日期时间   | timestamp                     |
| 日期     | date                          |
| 时间     | time                          |
| 布尔值    | bool                          |
| JSON   | jsonb                         |
| 数组     | type\[]                       |

## 触发关键词

- "生成表"
- "创建表"
- "表结构"
- "DDL"
- "PostgreSQL 表"
- "pg 表"
- "数据库表"

