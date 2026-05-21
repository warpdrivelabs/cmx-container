//! 插件管控 API 响应结构体
//!
//! 定义管控操作的响应格式

use serde::Serialize;
use utoipa::ToSchema;

/// 管控操作响应
#[derive(Debug, Serialize, ToSchema)]
pub struct ControlActionResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 执行动作（installed/upgraded/downgraded/uninstalled）
    pub action: String,
    /// 应用ID
    pub app_id: String,
}
