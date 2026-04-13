//! 插件调用相关类型
//!
//! 定义宿主与 WASM 之间插件调用的请求和响应结构体。

use serde::{Deserialize, Serialize};

/// 服务调用请求
///
/// 用于 WASM 插件向宿主发起插件间服务调用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallRequest {
    /// 目标插件ID
    pub target_plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据(JSON 字符串)
    pub input: String,
}

/// 服务调用响应
///
/// 宿主返回给 WASM 插件的插件间调用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCallResponse {
    /// 是否成功
    pub success: bool,
    /// 输出数据(JSON 字符串)
    pub output: Option<String>,
    /// 执行耗时(微秒)
    pub elapsed_us: Option<u64>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件信息响应
///
/// 宿主返回给 WASM 插件的当前插件信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfoResponse {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 当前事务ID
    pub txn_id: Option<String>,
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
}
