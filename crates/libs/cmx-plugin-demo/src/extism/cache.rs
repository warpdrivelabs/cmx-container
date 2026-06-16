use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;


/// 缓存订单状态
///
/// 将订单状态写入缓存，演示 cache_set 的使用方式。
///
/// # Arguments
///
/// 支持两种输入格式：
///
/// 1. `UpdateOrderRequest`（直接缓存单条订单）：
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `order_id` | string | 是 | 订单ID |
/// | `status` | string | 是 | 订单状态 |
///
/// 2. `query_orders` 的返回结构（批量缓存订单列表）：
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `success` | bool | 是 | 查询是否成功 |
/// | `dataset` | object | 是 | 数据集，包含 columns 和 rows |
/// | `dataset.columns` | string[] | 是 | 列名数组（需含 id 和 status） |
/// | `dataset.rows` | array[] | 是 | 行数据数组，每行为字段值数组 |
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
