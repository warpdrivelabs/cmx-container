use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 路由判断函数（服务编排用）。
    ///
    /// 根据输入的 route 字段决定返回哪个分支标识。
    pub fn route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let route_input: RouteInput = serde_json::from_value(input.input.clone())
            .unwrap_or(RouteInput {
                route: "1".to_string(),
            });
        let route = route_input.route.trim();
        let result = match route {
            "1" | "2" | "3" => route,
            _ => "1",
        };
        self.host.log_info(&format!("路由判断: route={}, 分支={}", route, result))?;
        Ok(FunctionOutput::from_json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// 通用分支处理函数（服务编排用）。
    ///
    /// 根据 input 中的 branch 字段区分处理逻辑。
    pub fn branch_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let branch = input
            .input
            .get("branch")
            .and_then(|v| v.as_str())
            .unwrap_or("1");
        self.host.log_info(&format!("执行分支{}处理", branch))?;
        let result = serde_json::json!({
            "branch": branch,
            "message": format!("分支{}处理完成", branch),
            "input": input.input,
            "initial_input": input.context.initial_input,
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 合并结果函数（服务编排用）。
    ///
    /// 从上下文中获取各分支的输出并合并。
    pub fn merge_result(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行结果合并")?;
        let branch_output = input
            .context
            .get_step_output("branch_process")
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let result = serde_json::json!({
            "merged": true,
            "branch_output": branch_output,
            "message": "结果合并完成",
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 事务内创建订单（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    pub fn tx_create_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务创建订单, txn_id={:?}", txn_id))?;
        let request: CreateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let sql = "INSERT INTO cmx_order (customer_name, product_name, quantity, unit_price, status) VALUES (?, ?, ?, ?, 'pending')".to_string();
        let params = vec![
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
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务创建订单完成",
        })))
    }

    /// 事务内更新库存（服务编排用）。
    ///
    /// 通过上下文获取 txn_id 确保在同一事务中执行。
    pub fn tx_update_stock(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let txn_id = input.context.txn_id.clone();
        self.host.log_info(&format!("事务更新库存, txn_id={:?}", txn_id))?;
        let product_name = input
            .input
            .get("product_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let quantity = input
            .input
            .get("quantity")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let sql = "UPDATE cmx_inventory SET stock = stock - ? WHERE product_name = ?".to_string();
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
            "txn_id": input.context.txn_id,
            "affected_rows": db_response.affected_rows,
            "message": "事务更新库存完成",
        })))
    }

    /// 最终处理函数（服务编排用）。
    ///
    /// 整合各步骤的输出，演示多宿主函数组合使用。
    pub fn final_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("执行最终处理")?;
        let merge_output = input
            .context
            .get_step_output("merge_result")
            .cloned()
            .unwrap_or_else(|| input.input.clone());
        let tx_create_output = input.context.get_step_output("tx_create_order").cloned();
        let tx_stock_output = input.context.get_step_output("tx_update_stock").cloned();
        // 缓存最终结果
        self.host.cache_set(
            "order:final_result",
            serde_json::json!({"processed": true}),
            Some(3600),
        )?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "final": true,
            "merge_output": merge_output,
            "tx_create_output": tx_create_output,
            "tx_stock_output": tx_stock_output,
            "txn_id": input.context.txn_id,
            "message": "服务编排执行完成",
        })))
    }
}
