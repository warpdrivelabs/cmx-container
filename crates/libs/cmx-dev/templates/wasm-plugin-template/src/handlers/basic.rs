use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 问候函数。
    ///
    /// 简单入参出参示例，不依赖任何宿主函数，
    /// 适合作为第一个插件函数的参考。
    pub fn greet(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let name = input.input.as_str().unwrap_or("World");
        let result = serde_json::json!({
            "message": format!("Hello, {}!", name),
            "greeting": format!("Welcome to cmx插件, {}!", name),
        });
        Ok(FunctionOutput::from_json(result))
    }

    /// 日志功能演示。
    ///
    /// 演示四级日志（info/error/debug/warn）的使用方式。
    pub fn demo_log(&self, _input: &FunctionInput) -> Result<FunctionOutput, String> {
        self.host.log_info("[订单插件] 信息日志示例")?;
        self.host.log_error("[订单插件] 错误日志示例")?;
        self.host.log_debug("[订单插件] 调试日志示例")?;
        self.host.log_warn("[订单插件] 警告日志示例")?;
        Ok(FunctionOutput::from_json(serde_json::json!({
            "success": true,
            "message": "四级日志记录完成",
        })))
    }
}
