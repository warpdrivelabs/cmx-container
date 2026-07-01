//! IAM 示例的 Extism 入口函数。

use crate::extism::ExtismHost;
use crate::handlers::PluginCore;
use cmx_plugin_sdk::*;

/// 查询当前调用者身份
///
/// 从 `auth_context` 透传读取当前调用者的身份信息（零宿主调用）。
/// `auth_context` 由认证中间件注入，随 `FunctionInput` 序列化进入插件。
///
/// # Arguments
///
/// * `input` - 函数输入，身份信息来自 `context.auth_context`。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `context.auth_context.user_id` | string | 是 | 当前用户 ID |
/// | `context.auth_context.username` | string | 是 | 当前用户名 |
/// | `context.auth_context.roles` | array | 否 | 当前用户角色码列表 |
/// | `context.auth_context.permissions` | array | 否 | 当前用户权限码列表 |
/// | `context.auth_context.org_id` | string | 否 | 当前用户所属组织 ID |
///
/// # Returns
///
/// 返回包含当前调用者身份信息的 `FunctionOutput`。
#[plugin_fn]
pub fn who_am_i(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.who_am_i(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 查询任意用户详情
///
/// 通过宿主函数 `get_user_details` 查询指定用户的详情（脱敏，无密码哈希）。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `user_id` 格式的查询参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `user_id` | string | 是 | 待查询的目标用户 ID |
///
/// # Returns
///
/// 返回包含用户详情的 `FunctionOutput`，用户不存在时 `found` 为 `false`。
#[plugin_fn]
pub fn query_user(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.query_user(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 校验当前用户权限
///
/// 从 `auth_context` 取当前用户 ID，调用宿主 `has_permission` 校验是否拥有指定权限码。
/// 走 IamChecker 缓存与熔断，命中缓存时无数据库往返。
///
/// # Arguments
///
/// * `input` - 函数输入，权限码来自 `input.code`，当前用户来自 `context.auth_context.user_id`。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `code` | string | 是 | 待校验的权限码，如 `user:read` |
/// | `context.auth_context.user_id` | string | 是 | 当前用户 ID |
///
/// # Returns
///
/// 返回包含权限校验结果的 `FunctionOutput`，`allowed` 为 `true` 表示拥有该权限。
#[plugin_fn]
pub fn check_my_permission(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.check_my_permission(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 查询用户有效权限聚合
///
/// 通过宿主函数 `get_user_effective_permissions` 查询指定用户的有效角色与权限聚合
/// （合并永久授权与活跃临时授权）。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `user_id` 格式的查询参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `user_id` | string | 是 | 待查询的目标用户 ID |
///
/// # Returns
///
/// 返回包含有效权限聚合的 `FunctionOutput`，包含 `roles`/`permissions` 角色码与权限码列表。
#[plugin_fn]
pub fn query_permissions(
    Msgpack(input): Msgpack<FunctionInput>,
) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.query_permissions(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
