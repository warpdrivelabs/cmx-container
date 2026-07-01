//! IAM 用户/权限查询示例。
//!
//! 演示如何从 WASM 插件读取当前调用者身份（auth_context 透传），
//! 并通过宿主函数查询任意用户详情与权限。

use crate::handlers::PluginCore;
use crate::host::HostFunctions;
use crate::models::*;

impl<H: HostFunctions> PluginCore<H> {
    /// 演示读取当前调用者身份（来自 auth_context 透传，零宿主调用）。
    ///
    /// 返回当前用户的 user_id / username / roles / permissions。
    /// 若未认证则返回错误。
    pub fn who_am_i(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let ctx = input
            .context
            .auth_context
            .as_ref()
            .ok_or_else(|| "未认证：auth_context 缺失".to_string())?;

        Ok(FunctionOutput::from_json(serde_json::json!({
            "user_id": ctx.user_id,
            "username": ctx.username,
            "roles": ctx.roles,
            "permissions": ctx.permissions,
            "org_id": ctx.org_id,
        })))
    }

    /// 演示查询任意用户详情（宿主函数，显式传 user_id）。
    pub fn query_user(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let user_id = input
            .input
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "参数缺失：user_id".to_string())?;

        let user = self.host.get_user_details(user_id)?;

        Ok(FunctionOutput::from_json(match user {
            Some(u) => serde_json::json!({
                "found": true,
                "user_id": u.user_id,
                "username": u.username,
                "nickname": u.nickname,
                "email": u.email,
                "org_id": u.org_id,
                "status": u.status,
            }),
            None => serde_json::json!({ "found": false }),
        }))
    }

    /// 演示权限校验：当前用户是否拥有指定权限码。
    ///
    /// 从 auth_context 取当前 user_id，调用宿主 has_permission（走缓存+熔断）。
    pub fn check_my_permission(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let code = input
            .input
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "参数缺失：code".to_string())?;

        let user_id = input
            .context
            .auth_context
            .as_ref()
            .map(|c| c.user_id.as_str())
            .ok_or_else(|| "未认证：auth_context 缺失".to_string())?;

        let allowed = self.host.has_permission(user_id, code)?;

        Ok(FunctionOutput::from_json(serde_json::json!({
            "user_id": user_id,
            "code": code,
            "allowed": allowed,
        })))
    }

    /// 演示查询用户有效权限聚合（roles + permissions）。
    pub fn query_permissions(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
        let user_id = input
            .input
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "参数缺失：user_id".to_string())?;

        let perms = self.host.get_user_effective_permissions(user_id)?;

        Ok(FunctionOutput::from_json(match perms {
            Some(p) => serde_json::json!({
                "found": true,
                "user_id": p.user_id,
                "username": p.username,
                "roles": p.roles,
                "permissions": p.permissions,
                "active_temp_roles": p.active_temp_roles,
            }),
            None => serde_json::json!({ "found": false }),
        }))
    }
}
