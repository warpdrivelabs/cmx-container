//! 调试 API 响应结构体

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 当前调试会话响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CurrentDebugSessionResponse {
    /// 是否有活跃会话
    pub has_session: bool,
    /// 插件ID
    pub plugin_id: Option<String>,
    /// 项目名称
    pub project_name: Option<String>,
    /// CMX进程ID
    pub cmx_pid: Option<u32>,
    /// WASM文件路径
    pub wasm_path: Option<String>,
    /// 源代码路径
    pub source_path: Option<String>,
    /// 调试函数名称
    pub debug_function: Option<String>,
    /// 会话ID
    pub session_id: Option<String>,
    /// 上一步输出
    pub previous_output: Option<serde_json::Value>,
    /// 初始输入
    pub initial_input: Option<serde_json::Value>,
}
