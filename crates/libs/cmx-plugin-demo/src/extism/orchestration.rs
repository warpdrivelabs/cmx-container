use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;
use extism_pdk::*;

/// 金额路由判断函数
///
/// 根据订单金额判断走大额审批流程还是普通流程。
/// switch 节点的返回值仅用于路由判断，不会传递给下一个节点（执行器自动恢复 current_output），
/// 因此输入中的业务字段会保留在 initial_input 中，供后续事务节点使用。
///
/// # Arguments
///
/// * `input` - 包含订单业务参数的完整输入。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `unit_price` | number | 是 | 单价 |
/// | `quantity` | integer | 是 | 数量 |
/// | `customer_name` | string | 否 | 客户名称（供后续事务节点使用） |
/// | `product_name` | string | 否 | 产品名称（供后续事务节点使用） |
///
/// # Returns
///
/// 返回 "high_value"（总额 >= 10000）或 "normal"（总额 < 10000）。
#[plugin_fn]
#[doc_type = "branch_fn"]
pub fn check_order_amount(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.check_order_amount(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务内创建订单
///
/// 在事务中执行订单创建操作，通过上下文获取 txn_id 确保在同一事务中执行。
/// 从 initial_input 获取原始业务参数，因为 switch 节点后 current_output 自动恢复为初始输入。
///
/// # Arguments
///
/// * `input` - switch 节点后的输入（current_output 自动恢复为初始输入），业务参数从 initial_input 获取。
///
/// | 字段（initial_input） | 类型 | 必填 | 说明 |
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

/// 事务内扣减库存
///
/// 在事务中执行库存扣减操作，通过上下文获取 txn_id 确保在同一事务中执行。
/// 从 initial_input 获取原始业务参数，因为前序节点 tx_create_order 的输出不含库存字段。
///
/// # Arguments
///
/// * `input` - 前序节点的输出（不含库存字段），实际业务参数从 initial_input 获取。
///
/// | 字段（initial_input） | 类型 | 必填 | 说明 |
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

/// 事务内记录审批
///
/// 在同一事务中记录大额订单的审批信息，仅大额订单分支执行。
/// 从 initial_input 获取客户名称，从 step_outputs 获取创建订单的输出（order_id）。
///
/// # Arguments
///
/// * `input` - 前序节点的输出，客户名称从 initial_input 获取，order_id 从 step_outputs 获取。
///
/// | 字段（initial_input） | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `customer_name` | string | 是 | 客户名称 |
#[plugin_fn]
pub fn tx_record_approval(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_record_approval(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 最终处理函数
///
/// 整合各步骤的输出并缓存最终结果，用于服务编排的最终节点。
///
/// # Arguments
///
/// * `input` - 前序节点的输出，各步骤输出从上下文 step_outputs 获取。
#[plugin_fn]
pub fn final_process(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.final_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
