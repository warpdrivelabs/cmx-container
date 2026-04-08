//! WASM 宿主函数 — 日志
//!
//! 为 WASM 插件提供日志记录能力的宿主函数。
//! 所有日志消息自动附加插件ID前缀，便于追踪来源。

use cmx_traits::{HostFuncError, ExtismFunctionProvider};
use extism::{host_fn, ValType, UserData, Manifest};

const PTR: ValType = ValType::I64;

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

impl ExtismFunctionProvider for LoggingHostFunctions {
    /// 返回命名空间 "cmx:log"
    fn namespace(&self) -> &str {
        "cmx:log"
    }

    /// 注册日志宿主函数
    fn register_functions(&self, builder: &mut extism::PluginBuilder) -> Result<(), HostFuncError> {
        // log_info 函数
        host_fn!(log_info(_user_data: (); message: String) {
            tracing::info!("[WASM] {}", message);
            Ok(())
        });

        // log_error 函数
        host_fn!(log_error(_user_data: (); message: String) {
            tracing::error!("[WASM] {}", message);
            Ok(())
        });

        // log_debug 函数
        host_fn!(log_debug(_user_data: (); message: String) {
            tracing::debug!("[WASM] {}", message);
            Ok(())
        });

        // log_warn 函数
        host_fn!(log_warn(_user_data: (); message: String) {
            tracing::warn!("[WASM] {}", message);
            Ok(())
        });

        // 使用 std::mem::replace 替换 builder
        let temp_manifest = Manifest::new([extism::Wasm::data(vec![])]);
        let temp_builder = extism::PluginBuilder::new(temp_manifest);
        let old_builder = std::mem::replace(builder, temp_builder);

        // 注册函数
        let new_builder = old_builder
            .with_function("log_info", [PTR], [], UserData::new(()), log_info)
            .with_function("log_error", [PTR], [], UserData::new(()), log_error)
            .with_function("log_debug", [PTR], [], UserData::new(()), log_debug)
            .with_function("log_warn", [PTR], [], UserData::new(()), log_warn);

        // 替换回去
        *builder = new_builder;

        Ok(())
    }

    /// 列出提供的函数名
    fn provided_functions(&self) -> Vec<&str> {
        vec!["log_info", "log_error", "log_debug", "log_warn"]
    }
}
