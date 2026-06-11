use serde::{Deserialize, Serialize};

/// 订单状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Processing,
    Shipped,
    Completed,
    Cancelled,
}

/// 订单创建请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_name: String,
    pub product_name: String,
    pub quantity: u32,
    pub unit_price: f64,
}

/// 订单查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderQueryRequest {
    pub order_id: Option<String>,
    pub customer_name: Option<String>,
    pub status: Option<OrderStatus>,
}

/// 订单更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOrderRequest {
    pub order_id: String,
    pub status: Option<OrderStatus>,
    pub quantity: Option<u32>,
}
