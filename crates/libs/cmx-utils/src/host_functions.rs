//! WASM 宿主函数 — 日志
//!
//! 为 WASM 插件提供日志记录能力的宿主函数。
//! 所有日志消息自动附加插件ID前缀，便于追踪来源。

use cmx_traits::{HostFuncError, HostFunctionProvider, HostFunctionDef, ValType};

/// 日志宿主函数提供者
///
/// 向 WASM 运行时注册日志记录函数。
/// 宿主函数从输入数据中解析日志消息文本，通过 tracing 输出。
pub struct LoggingHostFunctions;

impl LoggingHostFunctions {
    /// 创建日志宿主函数提供者
    pub fn new() -> Self {
        Self
    }
}

impl Default for LoggingHostFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl HostFunctionProvider for LoggingHostFunctions {
    /// 返回命名空间 "cmx:log"
    fn namespace(&self) -> &str {
        "cmx:log"
    }

    /// 返回提供的宿主函数列表
    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            HostFunctionDef::void_fn("log_info", "cmx:log", &[ValType::Ptr]),
            HostFunctionDef::void_fn("log_error", "cmx:log", &[ValType::Ptr]),
            HostFunctionDef::void_fn("log_debug", "cmx:log", &[ValType::Ptr]),
            HostFunctionDef::void_fn("log_warn", "cmx:log", &[ValType::Ptr]),
        ]
    }

    /// 调用宿主函数
    ///
    /// 日志函数的输入为 UTF-8 文本，从 Vec<u8> 解码为 String 后输出。
    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let message = String::from_utf8(input).unwrap_or_default();
        match name {
            "log_info" => {
                tracing::info!("[WASM] {}", message);
                Ok(Vec::new())
            }
            "log_error" => {
                tracing::error!("[WASM] {}", message);
                Ok(Vec::new())
            }
            "log_debug" => {
                tracing::debug!("[WASM] {}", message);
                Ok(Vec::new())
            }
            "log_warn" => {
                tracing::warn!("[WASM] {}", message);
                Ok(Vec::new())
            }
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    /// 列出提供的函数名
    fn provided_functions(&self) -> Vec<&str> {
        vec!["log_info", "log_error", "log_debug", "log_warn"]
    }
}
