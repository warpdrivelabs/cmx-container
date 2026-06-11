use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;


/// 查询订单列表
///
/// 执行参数化查询获取订单列表，演示 db_query 的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `OrderQueryRequest` 格式的查询参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `order_id` | string | 否 | 订单ID |
/// | `customer_name` | string | 否 | 客户名称 |
/// | `status` | string | 否 | 订单状态 |
#[plugin_fn]
pub fn query_orders(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.query_orders(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 创建订单
///
/// 执行参数化 INSERT 创建订单，演示 db_execute 的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `CreateOrderRequest` 格式的创建参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `customer_name` | string | 是 | 客户名称 |
/// | `product_name` | string | 是 | 产品名称 |
/// | `quantity` | integer | 是 | 数量 |
/// | `unit_price` | number | 是 | 单价 |
#[plugin_fn]
pub fn create_order(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.create_order(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 更新订单状态
///
/// 执行参数化 UPDATE 更新订单状态，演示 db_execute 的使用方式。
///
/// # Arguments
///
/// * `input` - 包含 `UpdateOrderRequest` 格式的更新参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `order_id` | string | 是 | 订单ID |
/// | `status` | string | 否 | 订单状态 |
/// | `quantity` | integer | 否 | 数量 |
#[plugin_fn]
pub fn update_order(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.update_order(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 删除订单
///
/// 执行参数化 DELETE 删除订单，演示 db_execute 的使用方式。
///
/// # Arguments
///
/// * `input` - `string` 订单ID。
#[plugin_fn]
pub fn delete_order(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.delete_order(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
