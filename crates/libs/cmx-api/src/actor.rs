//! 操作者身份提取（doc/mdm 等 API handler 共用）。
//!
//! 从 `CmxSvrContext.auth_context` 提取操作者 id/name，消除各 handler 重复手写。

use crate::middleware::CmxSvrContext;

/// 从请求上下文提取操作者 id（i64）。
///
/// `auth_context.user_id` 是 String（系统身份为字面量 "system"）。
/// 缺失/空/非数字 -> 兜底 `0`（约定 0=系统），保存**永不因身份缺失失败**。
pub fn actor_id_i64(svr_ctx: &CmxSvrContext) -> i64 {
    svr_ctx
        .0
        .auth_context
        .as_ref()
        .map(|a| a.user_id.trim())
        .filter(|u| !u.is_empty())
        .and_then(|u| u.parse::<i64>().ok())
        .unwrap_or(0)
}

/// 从请求上下文提取操作者显示名。
///
/// 缺失/空 -> "系统"（对齐 model_operator 兜底惯例）。
pub fn actor_name(svr_ctx: &CmxSvrContext) -> String {
    svr_ctx
        .0
        .auth_context
        .as_ref()
        .map(|a| a.username.trim())
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .unwrap_or_else(|| "系统".to_string())
}
