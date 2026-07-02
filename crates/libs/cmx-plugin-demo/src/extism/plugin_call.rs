use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;

/// 调用订单服务编排
///
/// 通过 call_service_by_key 调用订单服务编排，演示本地服务编排调用的使用方式。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，将作为服务编排的入参传递。
#[plugin_fn]
pub fn call_order_service(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.call_order_service(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 调用远程服务编排
///
/// 通过 call_remote_service 调用远程订单服务编排，演示远程服务编排调用的使用方式。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，将作为服务编排的入参传递。
#[plugin_fn]
pub fn call_remote_order_service(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.call_remote_order_service(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
