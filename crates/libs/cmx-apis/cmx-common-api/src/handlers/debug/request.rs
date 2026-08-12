//! 调试 API 请求结构体

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 插件调用请求
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvokeRequest {
    /// 函数名称
    pub function: String,
}
