//! IAM 示例的 Extism 入口函数。

use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;

/// 查询当前调用者身份（来自 auth_context 透传）。
#[plugin_fn]
pub fn who_am_i(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.who_am_i(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 查询任意用户详情（显式传 user_id）。
#[plugin_fn]
pub fn query_user(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.query_user(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 权限校验：当前用户是否拥有指定权限码。
#[plugin_fn]
pub fn check_my_permission(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.check_my_permission(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 查询用户有效权限聚合（roles + permissions）。
#[plugin_fn]
pub fn query_permissions(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.query_permissions(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
