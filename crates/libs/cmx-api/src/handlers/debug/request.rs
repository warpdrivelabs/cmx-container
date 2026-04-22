//! 调试 API 请求结构体

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use utoipa::ToSchema;

/// 插件调用请求
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvokeRequest {
    /// 函数名称
    pub function: String,
    /// 函数参数
    pub args: Vec<JsonValue>,
    /// 函数数据
    pub data: JsonValue,
}
