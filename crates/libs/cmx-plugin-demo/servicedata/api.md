# 服务编排接口参数说明

> 本文档描述 `cmx_plugin_demo` 插件三个服务编排接口的调用参数。

---

## 1. create_order — 创建订单

**接口说明**：创建订单并扣减库存（事务操作）

**流程**：`开始 → [事务] 创建订单 → 扣减库存 → 结束`

### 输入参数

```json
{
  "customer_name": "张三",
  "product_name": "企业版许可证",
  "quantity": 10,
  "unit_price": 999.00
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `customer_name` | string | 是 | 客户名称 |
| `product_name` | string | 是 | 产品名称，需与 cmx_inventory 表中的 product_name 匹配 |
| `quantity` | integer | 是 | 订购数量 |
| `unit_price` | number | 是 | 单价 |

### 输出参数

```json
{
  "operation": "tx_update_stock",
  "product_name": "企业版许可证",
  "quantity": 10,
  "txn_id": "txn-xxx",
  "affected_rows": 1,
  "message": "事务扣减库存完成"
}
```

---

## 2. query_order — 查询订单

**接口说明**：查询订单列表并缓存结果

**流程**：`开始 → 查询订单列表 → 缓存订单状态 → 结束`

### 输入参数

```json
{
  "order_id": "ORD-001",
  "customer_name": "张三",
  "status": "pending"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `order_id` | string | 否 | 订单ID，精确匹配 |
| `customer_name` | string | 否 | 客户名称，精确匹配 |
| `status` | string | 否 | 订单状态，取值：pending / confirmed / cancelled |

> 所有参数均为可选，不传则返回全部订单。

### 输出参数

```json
{
  "success": true,
  "dataset": {
    "columns": ["id", "customer_name", "product_name", "quantity", "status"],
    "rows": [
      ["ORD-001", "张三", "企业版许可证", 10, "pending"]
    ]
  }
}
```

---

## 3. process_order — 订单处理流程

**接口说明**：根据订单金额自动路由，大额订单走审批事务，普通订单走简单事务

**流程**：

```
开始 → 金额路由判断(switch)
  ├─ high_value（总额 >= 10000）: [事务] 创建订单 → 扣减库存 → 记录审批
  └─ normal（总额 < 10000）:      [事务] 创建订单 → 扣减库存
→ 最终处理 → 结束
```

### 输入参数

```json
{
  "customer_name": "张三",
  "product_name": "企业版许可证",
  "quantity": 100,
  "unit_price": 999.00
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `customer_name` | string | 是 | 客户名称 |
| `product_name` | string | 是 | 产品名称，需与 cmx_inventory 表中的 product_name 匹配 |
| `quantity` | integer | 是 | 订购数量 |
| `unit_price` | number | 是 | 单价 |

> **路由规则**：`unit_price × quantity >= 10000` 走大额订单分支（额外执行审批记录），否则走普通订单分支。

### 输出参数

```json
{
  "final": true,
  "tx_create_output": {
    "operation": "tx_create_order",
    "order_id": "uuid-xxx",
    "txn_id": "txn-xxx",
    "affected_rows": 1
  },
  "tx_stock_output": {
    "operation": "tx_update_stock",
    "product_name": "企业版许可证",
    "quantity": 100,
    "affected_rows": 1
  },
  "tx_approval_output": {
    "operation": "tx_record_approval",
    "approval_id": "uuid-yyy",
    "order_id": "uuid-xxx",
    "affected_rows": 1
  },
  "txn_id": "txn-xxx",
  "message": "订单处理流程执行完成"
}
```

> `tx_approval_output` 仅大额订单分支存在，普通订单分支该字段为 null。
