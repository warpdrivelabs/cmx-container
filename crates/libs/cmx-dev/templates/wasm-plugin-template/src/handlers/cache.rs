use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 缓存订单状态（演示 cache_set）。
    pub fn cache_order_status(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let request: UpdateOrderRequest = serde_json::from_value(input.input.clone())
            .map_err(|e| format!("参数解析失败: {}", e))?;
        let cache_key = format!("order:status:{}", request.order_id);
        let status_value = serde_json::json!({
            "order_id": request.order_id,
            "status": request.status,
        });
        self.host.cache_set(&cache_key, status_value, Some(3600))?;
        self.host.log_info(&format!("订单状态已缓存: {}", cache_key))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": true,
            "message": format!("订单 {} 状态已缓存", request.order_id),
        })))
    }

    /// 读取缓存的订单（演示 cache_get）。
    pub fn get_cached_order(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let cache_key = format!("order:status:{}", order_id);
        let response = self.host.cache_get(&cache_key)?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": response.success,
            "cache_key": cache_key,
            "value": response.value,
            "exists": response.exists,
        })))
    }

    /// 删除订单缓存（演示 cache_delete）。
    pub fn remove_order_cache(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let order_id = input.input.as_str().unwrap_or_default();
        let cache_key = format!("order:status:{}", order_id);
        let response = self.host.cache_delete(&cache_key)?;
        self.host.log_info(&format!("订单缓存已删除: {}", cache_key))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": response.success,
            "cache_key": cache_key,
        })))
    }
}
