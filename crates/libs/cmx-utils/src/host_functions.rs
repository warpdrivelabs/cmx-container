//! WASM 宿主函数 — 日志
//!
//! 为 WASM 插件提供日志记录能力的宿主函数。
//! 所有日志消息自动附加插件ID前缀，便于追踪来源。

use std::string::String;

use cmx_traits::{HostFuncError, HostFunctionProvider, HostFuncWrapper, WasmLinker};

/// 日志宿主函数提供者
///
/// 向 WASM 运行时注册日志记录函数（info/warn/error）。
/// 宿主函数从输入数据中解析日志消息文本，通过 tracing 输出。
pub struct LoggingHostFunctions;

impl LoggingHostFunctions {
    /// 创建日志宿主函数提供者
    pub fn new() -> Self {
        Self
    }

    /// 从输入字节中解析日志消息
    ///
    /// # 参数
    ///
    /// * `input` - 输入数据字节（预期为 UTF-8 字符串）
    ///
    /// # 返回值
    ///
    /// 返回 UTF-8 字符串，如果解码失败返回错误提示。
    fn parse_log_message(input: &[u8]) -> String {
        match std::str::from_utf8(input) {
            Ok(s) => s.to_string(),
            Err(_) => format!("[无法解码的日志消息, {} 字节]", input.len()),
        }
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

    /// 注册日志相关的宿主函数
    ///
    /// 注册以下函数：
    /// - `cmx:log/info` — 记录 info 级别日志
    /// - `cmx:log/warn` — 记录 warn 级别日志
    /// - `cmx:log/error` — 记录 error 级别日志
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
        // cmx:log/info — 记录 info 级别日志
        let info_fn: HostFuncWrapper = Box::new(|caller, input| {
            let msg = Self::parse_log_message(input);
            let plugin_id = caller.caller_data().plugin_id.clone();
            tracing::info!("[plugin:{}] {}", plugin_id, msg);
            Ok(Vec::new())
        });
        linker.define("cmx:log", "info", info_fn)?;

        // cmx:log/warn — 记录 warn 级别日志
        let warn_fn: HostFuncWrapper = Box::new(|caller, input| {
            let msg = Self::parse_log_message(input);
            let plugin_id = caller.caller_data().plugin_id.clone();
            tracing::warn!("[plugin:{}] {}", plugin_id, msg);
            Ok(Vec::new())
        });
        linker.define("cmx:log", "warn", warn_fn)?;

        // cmx:log/error — 记录 error 级别日志
        let error_fn: HostFuncWrapper = Box::new(|caller, input| {
            let msg = Self::parse_log_message(input);
            let plugin_id = caller.caller_data().plugin_id.clone();
            tracing::error!("[plugin:{}] {}", plugin_id, msg);
            Ok(Vec::new())
        });
        linker.define("cmx:log", "error", error_fn)?;

        Ok(())
    }

    /// 返回提供的函数名列表
    fn provided_functions(&self) -> Vec<&str> {
        vec![
            "cmx:log/info",
            "cmx:log/warn",
            "cmx:log/error",
        ]
    }
}
