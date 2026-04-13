//! WASM 上下文类型
//!
//! 定义 WASM 调用的上下文信息结构体。

use serde::{Deserialize, Serialize};

/// WASM 调用上下文
///
/// 包含 WASM 函数调用的完整上下文信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmContext {
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
    /// 数据库ID
    pub db_id: String,
    /// 事务ID
    pub txn_id: Option<String>,
    /// 插件ID
    pub plugin_id: String,
}
