use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;

/// 问候函数
///
/// 简单入参出参示例，不依赖任何宿主函数。
///
/// # Arguments
///
/// * `input` - `string` 名称。
///
/// # Returns
///
/// 返回包含问候语的 `FunctionOutput`。
#[plugin_fn]
pub fn greet(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.greet(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 日志功能演示
///
/// 演示四级日志（info/error/debug/warn）的使用方式。
#[plugin_fn]
pub fn demo_log(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_log(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
