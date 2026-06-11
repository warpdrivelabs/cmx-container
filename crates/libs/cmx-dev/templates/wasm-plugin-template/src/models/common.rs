use serde::{Deserialize, Serialize};

/// 路由判断输入（服务编排用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInput {
    pub route: String,
}

/// 库存检查请求（用于插件调用示例）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryCheckRequest {
    pub product_name: String,
    pub quantity: u32,
}

/// 通用操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
