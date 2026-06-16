---
name: "pg-table-generator"
description: "Generates PostgreSQL DDL table definitions with standard audit fields, proper comments, and nullable constraints. Invoke when user asks to create/generate PostgreSQL table structure, DDL, or schema definition."
---

# PostgreSQL 表结构生成器

根据用户需求生成 PostgreSQL DDL 表定义。

## 生成规则

### 标识符命名规则

1. **不使用双引号**：表名、字段名、约束名、索引名等所有标识符均不使用双引号包裹，直接使用小写字母和下划线命名。
2. **不指定 schema**：生成的 SQL 不包含 `schema_name` 前缀，直接使用表名（即默认 schema 为 `public`，由数据库 search_path 决定）。

### 标准审计字段（所有表必需）

所有表必须包含以下标准字段（除 `id` 外均放在表最后）：

| 字段名 | 类型 | 约束 | 说明 |
|--------|------|------|------|
| id | varchar(64) | NOT NULL | 主键 |
| archived | int4 | DEFAULT 0 | 是否归档 |
| create_time | timestamp | DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| update_time | timestamp | DEFAULT CURRENT_TIMESTAMP | 更新时间 |
| create_by | varchar(100) | - | 创建人ID |
| create_name | varchar(100) | - | 创建人姓名 |
| update_by | varchar(100) | - | 更新人ID |
| update_name | varchar(100) | - | 更新人姓名 |

### 字段约束规则

1. **必填字段**：仅主键字段（id）和唯一索引字段设置为 NOT NULL
2. **可选字段**：所有非主键、非唯一索引字段均为可选（可NULL）
3. **唯一索引字段**：根据业务需求设置 NOT NULL 和 UNIQUE

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
- 外键字段：`'关联XXX表ID'`

## 输出格式

```sql
-- 表注释
CREATE TABLE table_name (
    -- 业务字段（主键在前）
    id varchar(64) NOT NULL,
    field_name varchar(255),
    -- ... 其他业务字段 ...

    -- 标准审计字段（除id外按此顺序排列）
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),

    CONSTRAINT pk_table_name PRIMARY KEY (id)
);

-- 唯一索引（如有）
CREATE UNIQUE INDEX uk_table_name_field ON table_name (field_name);

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

    CONSTRAINT pk_user PRIMARY KEY (id)
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

    CONSTRAINT pk_product PRIMARY KEY (id),
    CONSTRAINT uk_product_code UNIQUE (code)
);

CREATE UNIQUE INDEX uk_product_code ON product (code);

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

    CONSTRAINT pk_order PRIMARY KEY (id),
    CONSTRAINT uk_order_order_no UNIQUE (order_no)
);

CREATE UNIQUE INDEX uk_order_order_no ON order (order_no);

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

## 字段类型参考

| 业务场景 | 推荐类型 |
|----------|----------|
| 短文本/枚举 | varchar(N) |
| 长文本 | text |
| 整数 | int4 |
| 长整数 | int8 |
| 小数 | numeric(p,s) |
| 金额 | numeric(12,2) 或 numeric(18,2) |
| 日期时间 | timestamp |
| 日期 | date |
| 时间 | time |
| 布尔值 | bool |
| JSON | jsonb |
| 数组 | type[] |

## 触发关键词

- "生成表"
- "创建表"
- "表结构"
- "DDL"
- "PostgreSQL 表"
- "pg 表"
- "数据库表"
