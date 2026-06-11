use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;
use extism_pdk::*;

/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识，用于服务编排的路由节点。
///
/// # Arguments
///
/// * `input` - 包含 `RouteInput` 格式的路由参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `route` | string | 是 | 路由标识，取值为 "1"、"2" 或 "3" |
///
/// # Returns
///
/// 返回 "1"、"2" 或 "3"，对应三个分支。
#[plugin_fn]
#[doc_type = "branch_fn"]
pub fn route_check(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.route_check(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 通用分支处理函数
///
/// 根据 input 中的 branch 字段区分处理逻辑，用于服务编排的分支节点。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，需包含 `branch` 字段标识分支编号。输入来源于上一步骤的输出。
#[plugin_fn]
pub fn branch_process(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.branch_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 合并结果函数
///
/// 从上下文中获取各分支的输出并合并，用于服务编排的合并节点。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，来源于上一步骤的输出及上下文中的 `step_outputs` 缓存。
#[plugin_fn]
pub fn merge_result(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.merge_result(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务内创建订单
///
/// 在事务中执行订单创建操作，通过上下文获取 txn_id 确保在同一事务中执行。
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
pub fn tx_create_order(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_create_order(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务内更新库存
///
/// 在事务中执行库存更新操作，通过上下文获取 txn_id 确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，需包含 `product_name` 和 `quantity` 字段。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `product_name` | string | 是 | 产品名称 |
/// | `quantity` | integer | 是 | 扣减数量 |
#[plugin_fn]
pub fn tx_update_stock(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_update_stock(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 最终处理函数
///
/// 整合各步骤的输出并缓存结果，用于服务编排的最终节点。
///
/// # Arguments
///
/// * `input` - 动态 JSON 输入，来源于上一步骤的输出及上下文中的 `step_outputs` 缓存。
#[plugin_fn]
pub fn final_process(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.final_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
