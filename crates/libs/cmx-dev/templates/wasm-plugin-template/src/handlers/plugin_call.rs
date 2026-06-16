use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {

    /// 调用订单服务编排（演示 call_service_by_key）。
    pub fn call_order_service(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let service_request = CallServiceRequest {
            service_key: "query_order".to_string(),
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
            service_key: "query_order".to_string(),
            input: input.input.clone(),
            server_name: Some("cmx-server".to_string()),
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
