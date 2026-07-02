use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 缓存订单状态（演示 cache_set）。
    ///
    /// 支持两种输入：
    /// 1. UpdateOrderRequest（{ order_id, status }）— 直接使用字段
    /// 2. query_orders 的返回（{ success, dataset: { columns, rows } }）— 从 rows 中提取
    pub fn cache_order_status(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        // 优先尝试解析为 UpdateOrderRequest
        if let Ok(request) = serde_json::from_value::<UpdateOrderRequest>(input.input.clone()) {
            let cache_key = format!("order:status:{}", request.order_id);
            let status_value = serde_json::json!({
                "order_id": request.order_id,
                "status": request.status,
            });
            self.host.cache_set(&cache_key, status_value, Some(3600))?;
            self.host
                .log_info(&format!("订单状态已缓存: {}", cache_key))?;
            return Ok(FunctionOutput::from_json(serde_json::json!({
                "success": true,
                "message": format!("订单 {} 状态已缓存", request.order_id),
            })));
        }

        // 尝试从 query_orders 的返回结构（dataset）中提取订单
        let dataset = input.input.get("dataset").ok_or("无法解析输入参数")?;
        let columns: Vec<String> = dataset
            .get("columns")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        let empty_rows = Vec::new();
        let rows = dataset
            .get("rows")
            .and_then(|r| r.as_array())
            .unwrap_or(&empty_rows);

        if rows.is_empty() {
            return Ok(FunctionOutput::from_json(serde_json::json!({
                "success": false,
                "message": "无订单数据可缓存",
            })));
        }

        // 定位列索引
        let id_idx = columns.iter().position(|c| c == "id");
        let status_idx = columns.iter().position(|c| c == "status");
        let cached_count = rows.len();

        for row in rows {
            let order_id = id_idx
                .and_then(|i| row.get(i))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let status = status_idx
                .and_then(|i| row.get(i))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let cache_key = format!("order:status:{}", order_id);
            let status_value = serde_json::json!({
                "order_id": order_id,
                "status": status,
            });
            self.host.cache_set(&cache_key, status_value, Some(3600))?;
        }

        self.host
            .log_info(&format!("已缓存 {} 条订单状态", cached_count))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": true,
            "cached_count": cached_count,
            "message": format!("已缓存 {} 条订单状态", cached_count),
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
        self.host
            .log_info(&format!("订单缓存已删除: {}", cache_key))?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": response.success,
            "cache_key": cache_key,
        })))
    }
}
