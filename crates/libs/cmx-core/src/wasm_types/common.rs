//! 通用包装类型
//!
//! 定义宿主与 WASM 之间通用的请求和响应包装结构体。

use serde::{Deserialize, Serialize};

use super::context::WasmContext;

/// 通用 WASM 函数请求
///
/// 用于 Host 调用 WASM 函数时的通用请求包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionRequest<T> {
    /// 调用上下文
    pub context: WasmContext,
    /// 业务请求数据
    pub data: T,
}

/// 通用 WASM 函数响应
///
/// 用于 WASM 函数返回时的通用响应包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionResponse<T> {
    /// 是否成功
    pub success: bool,
    /// 业务响应数据
    pub data: Option<T>,
    /// 错误信息
    pub error: Option<String>,
}
