use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;


/// 缓存订单状态
///
/// 将订单状态写入缓存，演示 cache_set 的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `UpdateOrderRequest` 格式的订单更新参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `order_id` | string | 是 | 订单ID |
/// | `status` | string | 否 | 订单状态 |
#[plugin_fn]
pub fn cache_order_status(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.cache_order_status(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 读取缓存的订单
///
/// 从缓存中读取订单状态，演示 cache_get 的使用方式。
///
/// # Arguments
///
/// * `input` - `string` 订单ID。
#[plugin_fn]
pub fn get_cached_order(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.get_cached_order(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 删除订单缓存
///
/// 删除缓存中的订单状态，演示 cache_delete 的使用方式。
///
/// # Arguments
///
/// * `input` - `string` 订单ID。
#[plugin_fn]
pub fn remove_order_cache(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.remove_order_cache(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
