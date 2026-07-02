use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 查询订单列表（演示 db_query + 参数化查询）。
    pub fn query_orders(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: OrderQueryRequest =
            serde_json::from_value(input.input.clone()).unwrap_or(OrderQueryRequest {
                order_id: None,
                customer_name: None,
                status: None,
            });
        let mut sql =
            "SELECT id, customer_name, product_name, quantity, status FROM cmx_order WHERE 1=1"
                .to_string();
        let mut params = Vec::new();
        let mut param_idx = 1;
        if let Some(ref order_id) = request.order_id {
            sql.push_str(&format!(" AND id = ${}", param_idx));
            param_idx += 1;
            params.push(serde_json::json!(order_id));
        }
        if let Some(ref customer_name) = request.customer_name {
            sql.push_str(&format!(" AND customer_name = ${}", param_idx));
            param_idx += 1;
            params.push(serde_json::json!(customer_name));
        }
        if let Some(ref status) = request.status {
            sql.push_str(&format!(" AND status = ${}", param_idx));
            params.push(serde_json::json!(status));
        }
        let db_request = DbRequest {
            sql,
            params: if params.is_empty() {
                None
            } else {
                Some(serde_json::Value::Array(params))
            },
            dataset_id: None,
            db_id: None,
            txn_id: None,
            data_values: None,
        };
        let db_response = self.host.db_query(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "dataset": db_response.dataset,
        })))
    }

    /// 创建订单（演示 db_execute + INSERT + 参数化查询）。
    pub fn create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO cmx_order (id, customer_name, product_name, quantity, unit_price, status) VALUES ($1, $2, $3, $4, $5, 'pending')".to_string();
        let params = vec![
            serde_json::json!(id),
            serde_json::json!(request.customer_name),
            serde_json::json!(request.product_name),
            serde_json::json!(request.quantity),
            serde_json::json!(request.unit_price),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(serde_json::Value::Array(params)),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
            data_values: None,
        };
        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!(
            "订单创建成功, 影响行数: {:?}",
            db_response.affected_rows
        ))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }

    /// 更新订单状态（演示 db_execute + UPDATE + 参数化查询）。
    pub fn update_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let sql = "UPDATE cmx_order SET status = $1 WHERE id = $2".to_string();
        let params = vec![
            serde_json::json!(request.status),
            serde_json::json!(request.order_id),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(serde_json::Value::Array(params)),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
            data_values: None,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }

    /// 删除订单（演示 db_execute + DELETE + 参数化查询）。
    pub fn delete_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let sql = "DELETE FROM cmx_order WHERE id = $1".to_string();
        let params = vec![serde_json::json!(order_id)];
        let db_request = DbRequest {
            sql,
            params: Some(serde_json::Value::Array(params)),
            dataset_id: None,
            db_id: None,
            txn_id: input.context.txn_id.clone(),
            data_values: None,
        };
        let db_response = self.host.db_execute(db_request)?;
        self.host.log_info(&format!("订单已删除: {}", order_id))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": db_response.success,
            "affected_rows": db_response.affected_rows,
        })))
    }
}
