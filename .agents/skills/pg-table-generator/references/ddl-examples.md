# DDL 完整示例（4 个）+ 字段类型参考

> 本文件是 pg-table-generator 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

## 使用示例

### 示例1：简单表

用户输入：

```
生成用户表 user，包含字段：username(varchar(50)), email(varchar(100)), phone(varchar(20))
```

输出：

```sql
-- 用户表
CREATE TABLE sys_user (
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

COMMENT ON TABLE sys_user IS '用户表';
COMMENT ON COLUMN sys_user.id IS '主键ID';
COMMENT ON COLUMN sys_user.username IS '用户名';
COMMENT ON COLUMN sys_user.email IS '邮箱';
COMMENT ON COLUMN sys_user.phone IS '手机号';
COMMENT ON COLUMN sys_user.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN sys_user.create_time IS '创建时间';
COMMENT ON COLUMN sys_user.update_time IS '更新时间';
COMMENT ON COLUMN sys_user.create_by IS '创建人ID';
COMMENT ON COLUMN sys_user.create_name IS '创建人姓名';
COMMENT ON COLUMN sys_user.update_by IS '更新人ID';
COMMENT ON COLUMN sys_user.update_name IS '更新人姓名';
COMMENT ON COLUMN sys_user.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';
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
CREATE TABLE biz_order (
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

CREATE UNIQUE INDEX uk_order_order_no ON biz_order (order_no) WHERE archived = 0;

COMMENT ON TABLE biz_order IS '订单表';
COMMENT ON COLUMN biz_order.id IS '主键ID';
COMMENT ON COLUMN biz_order.order_no IS '订单编号';
COMMENT ON COLUMN biz_order.amount IS '订单金额';
COMMENT ON COLUMN biz_order.status IS '订单状态';
COMMENT ON COLUMN biz_order.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN biz_order.create_time IS '创建时间';
COMMENT ON COLUMN biz_order.update_time IS '更新时间';
COMMENT ON COLUMN biz_order.create_by IS '创建人ID';
COMMENT ON COLUMN biz_order.create_name IS '创建人姓名';
COMMENT ON COLUMN biz_order.update_by IS '更新人ID';
COMMENT ON COLUMN biz_order.update_name IS '更新人姓名';
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
CREATE TABLE biz_order_item (
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
CREATE INDEX idx_order_item_order_id ON biz_order_item (order_id);
CREATE INDEX idx_order_item_user_id ON biz_order_item (user_id);
CREATE INDEX idx_order_item_product_id ON biz_order_item (product_id);

COMMENT ON TABLE order_item IS '订单项表';
COMMENT ON COLUMN biz_order_item.id IS '主键ID';
COMMENT ON COLUMN biz_order_item.order_id IS '关联订单表ID';
COMMENT ON COLUMN biz_order_item.user_id IS '关联用户表ID';
COMMENT ON COLUMN biz_order_item.product_id IS '关联商品表ID';
COMMENT ON COLUMN biz_order_item.quantity IS '数量';
COMMENT ON COLUMN biz_order_item.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN biz_order_item.create_time IS '创建时间';
COMMENT ON COLUMN biz_order_item.update_time IS '更新时间';
COMMENT ON COLUMN biz_order_item.create_by IS '创建人ID';
COMMENT ON COLUMN biz_order_item.create_name IS '创建人姓名';
COMMENT ON COLUMN biz_order_item.update_by IS '更新人ID';
COMMENT ON COLUMN biz_order_item.update_name IS '更新人姓名';
COMMENT ON COLUMN biz_order_item.ext_attributes IS '扩展属性，存储JSON格式的额外业务属性';
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
