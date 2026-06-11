use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 调用库存插件检查库存（演示 call_plugin）。
    pub fn check_inventory(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: InventoryCheckRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let plugin_request = PluginFunRequest {
            plugin_id: "inventory_plugin".to_string(),
            function_name: "check_stock".to_string(),
            input: serde_json::json!({
                "product_name": request.product_name,
                "quantity": request.quantity,
            }),
            initial_input: None,
            server_name: None,
            debug: Some(false),
        };
        match self.host.call_plugin(plugin_request) {
            Ok(result) => {
                self.host.log_info("库存检查完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "inventory_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("库存检查失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("库存检查失败: {}", e),
                })))
            }
        }
    }

    /// 调用远程库存插件（演示 call_remote_plugin）。
    pub fn check_remote_inventory(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: InventoryCheckRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let plugin_request = PluginFunRequest {
            plugin_id: "inventory_plugin".to_string(),
            function_name: "check_stock".to_string(),
            input: serde_json::json!({
                "product_name": request.product_name,
                "quantity": request.quantity,
            }),
            initial_input: None,
            server_name: None,
            debug: Some(false),
        };
        match self.host.call_remote_plugin("remote-server", plugin_request) {
            Ok(result) => {
                self.host.log_info("远程库存检查完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "remote_inventory_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("远程库存检查失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("远程库存检查失败: {}", e),
                })))
            }
        }
    }

    /// 调用订单服务编排（演示 call_service_by_key）。
    pub fn call_order_service(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let service_request = CallServiceRequest {
            service_key: "order_service".to_string(),
            input: input.input.clone(),
            server_name: None,
            include_steps: Some(true),
            debug: Some(false),
            debug_node_id: None,
            debug_params: None,
        };
        match self.host.call_service_by_key(service_request) {
            Ok(result) => {
                self.host.log_info("订单服务编排调用完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "service_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("订单服务编排调用失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("服务编排调用失败: {}", e),
                })))
            }
        }
    }

    /// 调用远程服务编排（演示 call_remote_service）。
    pub fn call_remote_order_service(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let service_request = CallServiceRequest {
            service_key: "order_service".to_string(),
            input: input.input.clone(),
            server_name: None,
            include_steps: Some(true),
            debug: Some(false),
            debug_node_id: None,
            debug_params: None,
        };
        match self.host.call_remote_service("remote-server", service_request) {
            Ok(result) => {
                self.host.log_info("远程订单服务编排调用完成")?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": true,
                    "remote_service_result": result,
                })))
            }
            Err(e) => {
                self.host.log_error(&format!("远程服务编排调用失败: {}", e))?;
                Ok(FunctionOutput::from_json(serde_json::json!({
                    "success": false,
                    "message": format!("远程服务编排调用失败: {}", e),
                })))
            }
        }
    }
}
