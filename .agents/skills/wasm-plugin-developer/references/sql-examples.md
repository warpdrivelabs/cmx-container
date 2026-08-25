# WASM 插件 SQL 完整示例（三个）

> 本文件是 wasm-plugin-developer 技能的 references 细节层（从 SKILL.md §四 下沉，内容未改）。
> DataValue / dv! / ParamsBuilder / NullTyped 的通用规范见 cmx-sql-execution 技能（共享真源）：
> `../../cmx-sql-execution/references/datavalue-and-params.md`。
> 返回决策入口：[../SKILL.md](../SKILL.md)

### 5.9 完整示例

#### 示例 1：参数化查询（`db_query` + `data_values` + `dv!`）

```rust
use cmx_core::dv;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn query_orders(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: OrderQueryRequest = serde_json::from_value(input.input.clone())
            .unwrap_or(OrderQueryRequest {
                order_id: None,
                customer_name: None,
                status: None,
            });

        let mut sql = "SELECT id, customer_name, product_name, quantity, status FROM cmx_order WHERE 1=1".to_string();
        let mut params: Vec<DataValue> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref order_id) = request.order_id {
            sql.push_str(&format!(" AND id = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(order_id.clone()));
        }
        if let Some(ref customer_name) = request.customer_name {
            sql.push_str(&format!(" AND customer_name = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(customer_name.clone()));
        }
        if let Some(ref status) = request.status {
            sql.push_str(&format!(" AND status = ${param_idx}"));
            param_idx += 1;
            params.push(DataValue::String(status.clone()));
        }

        let db_request = DbRequest {
            sql,
            data_values: if params.is_empty() { None } else { Some(params) },
            dataset_id: None,
            db_id: None,
            txn_id: None,
            params: None,  // 新代码不使用 params
        };

        let db_response = self.host.db_query(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "dataset": db_response.dataset,
        })))
    }
}
```

#### 示例 2：INSERT（`db_execute` + `dv!` + 事务 `txn_id`）

```rust
use cmx_core::dv;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {e}"))?;

        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO cmx_order \
                   (id, customer_name, product_name, quantity, unit_price, status, remark, sort_order) \
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)".to_string();

        // ★ 使用 dv! 宏，Option<T> 直接传入即自动 .into()
        let params = dv![
            id.clone(),
            request.customer_name.clone(),
            request.product_name.clone(),
            request.quantity,
            request.unit_price,
            request.status.clone(),
            request.remark.clone(),        // Option<String> → String 或 Null
            request.sort_order,            // Option<i64> → Int 或 NullTyped(Int)
        ];

        let db_request = DbRequest {
            sql,
            data_values: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),  // 透传事务 ID
            params: None,
        };

        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!("订单创建成功, 影响行数: {:?}", db_response.affected_rows))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
```

#### 示例 3：动态 UPDATE（`ParamsBuilder` + `set_opt`）

```rust
use cmx_core::ParamsBuilder;
use cmx_core::model::cell::DataValue;

impl<H: HostFunctions> PluginCore<H> {
    pub fn update_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {e}"))?;

        // ★ ParamsBuilder 自动管理占位符，SET 从 $1 起
        let mut b = ParamsBuilder::new(0);
        b.set_opt("customer_name", request.customer_name)
         .set_opt("product_name", request.product_name)
         .set_opt("quantity", request.quantity)
         .set_opt("unit_price", request.unit_price)
         .set_opt("status", request.status)
         .set_opt("remark", request.remark)
         .set_opt("sort_order", request.sort_order);
        let (set_clause, mut params) = b.build();

        if set_clause.is_empty() {
            return Err("未提供任何更新字段".into());
        }

        // WHERE id 放最后
        let where_idx = params.len() + 1;
        params.push(DataValue::String(request.order_id));
        let sql = format!(
            "UPDATE cmx_order SET {set_clause}, update_time = NOW() WHERE id = ${where_idx}"
        );

        let db_request = DbRequest {
            sql,
            data_values: Some(params),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
            params: None,
        };

        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
```
