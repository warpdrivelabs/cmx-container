use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 金额路由判断（服务编排用）。
    ///
    /// 根据订单金额判断走大额审批流程还是普通流程。
    /// switch 节点的返回值仅用于路由判断，不会传递给下一个节点。
    pub fn check_order_amount(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let unit_price = input
            .input
            .get("unit_price")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let quantity = input
            .input
            .get("quantity")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let total = unit_price * quantity as f64;
        let result = if total >= 10000.0 {
            "high_value"
        } else {
            "normal"
        };
        self.host.log_info(&format!(
            "金额路由判断: unit_price={}, quantity={}, total={}, 分支={}",
            unit_price, quantity, total, result
        ))?;
        Ok(FunctionOutput::from_json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// 事务内创建订单（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    /// 从 initial_input 获取原始业务参数，因为 switch 节点后 current_output 自动恢复为初始输入。
    pub fn tx_create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务创建订单, txn_id={:?}", txn_id))?;
        let request: CreateOrderRequest = serde_json::from_value(input.context.initial_input.clone())
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
            txn_id,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "operation": "tx_create_order",
            "order_id": id,
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务创建订单完成",
        })))
    }

    /// 事务内扣减库存（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    /// 从 initial_input 获取原始业务参数，因为前序节点 tx_create_order 的输出不含库存字段。
    pub fn tx_update_stock(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务扣减库存, txn_id={:?}", txn_id))?;
        let product_name = input
            .context
            .initial_input
            .get("product_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let quantity = input
            .context
            .initial_input
            .get("quantity")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let sql = "UPDATE cmx_inventory SET stock = stock - $1 WHERE product_name = $2".to_string();
        let params = vec![
            serde_json::json!(quantity),
            serde_json::json!(product_name),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(serde_json::Value::Array(params)),
            dataset_id: None,
            db_id: None,
            txn_id,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "operation": "tx_update_stock",
            "product_name": product_name,
            "quantity": quantity,
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务扣减库存完成",
        })))
    }

    /// 事务内记录审批（服务编排用，仅大额订单）。
    ///
    /// 在同一事务中记录大额订单的审批信息。
    /// 从 initial_input 获取原始业务参数，从 step_outputs 获取创建订单的输出。
    pub fn tx_record_approval(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务记录审批, txn_id={:?}", txn_id))?;
        let customer_name = input
            .context
            .initial_input
            .get("customer_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let order_id = input
            .context
            .get_step_output("tx_create_order_hv")
            .or_else(|| input.context.get_step_output("tx_create_order_nm"))
            .and_then(|v| v.get("order_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO cmx_order_approval (id, order_id, customer_name, approval_status) VALUES ($1, $2, $3, 'pending')".to_string();
        let params = vec![
            serde_json::json!(id),
            serde_json::json!(order_id),
            serde_json::json!(customer_name),
        ];
        let db_request = DbRequest {
            sql,
            params: Some(serde_json::Value::Array(params)),
            dataset_id: None,
            db_id: None,
            txn_id,
        };
        let db_response = self.host.db_execute(db_request)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "operation": "tx_record_approval",
            "approval_id": id,
            "order_id": order_id,
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务记录审批完成",
        })))
    }

    /// 最终处理函数（服务编排用）。
    ///
    /// 整合各步骤的输出并缓存最终结果。
    pub fn final_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行最终处理")?;
        let tx_create_output = input
            .context
            .get_step_output("tx_create_order_hv")
            .or_else(|| input.context.get_step_output("tx_create_order_nm"))
            .cloned();
        let tx_stock_output = input
            .context
            .get_step_output("tx_update_stock_hv")
            .or_else(|| input.context.get_step_output("tx_update_stock_nm"))
            .cloned();
        let tx_approval_output = input.context.get_step_output("tx_record_approval").cloned();
        // 缓存最终结果
        self.host.cache_set(
            "order:final_result",
            serde_json::json!({"processed": true}),
            Some(3600),
        )?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "final": true,
            "tx_create_output": tx_create_output,
            "tx_stock_output": tx_stock_output,
            "tx_approval_output": tx_approval_output,
            "txn_id": input.context.txn_id,
            "message": "订单处理流程执行完成",
        })))
    }
}
