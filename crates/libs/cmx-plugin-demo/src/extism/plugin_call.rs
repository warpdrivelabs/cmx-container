use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;
use extism_pdk::*;

/// 调用库存插件检查库存
///
/// 通过 call_plugin 调用库存插件检查库存，演示本地插件调用的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `InventoryCheckRequest` 格式的库存检查参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `product_name` | string | 是 | 产品名称 |
/// | `quantity` | integer | 是 | 需求数量 |
#[plugin_fn]
pub fn check_inventory(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.check_inventory(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 调用远程库存插件
///
/// 通过 call_remote_plugin 调用远程库存插件检查库存，演示远程插件调用的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `InventoryCheckRequest` 格式的库存检查参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `product_name` | string | 是 | 产品名称 |
/// | `quantity` | integer | 是 | 需求数量 |
#[plugin_fn]
pub fn check_remote_inventory(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.check_remote_inventory(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

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
