//! 身份读取钩子（解耦各服务不同的 scope 机制）。
//!
//! flow-server 用 `cmx-flow-app::tenant` 的 task_local scope；平台 web-server 用
//! `cmx-traits::auth::context_scope`。本 crate 不能硬依赖任一，故各服务在启动时注入一个
//! 零捕获自由函数指针，观测中间件在请求内调它读当前身份。未注入时返回匿名。

use std::sync::OnceLock;

use serde::Serialize;

/// 当前请求身份快照（观测中间件用；各服务自行映射自己的 scope）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Identity {
    /// 租户（无则各服务给默认值，如 "default"）。
    pub tenant: String,
    /// 用户标识（可空=匿名）。
    pub user: Option<String>,
    /// 角色列表。
    pub roles: Vec<String>,
}

static IDENTITY_PROVIDER: OnceLock<fn() -> Option<Identity>> = OnceLock::new();

/// 注入身份读取器（服务启动时调一次，早于开始服务）。
pub fn set_identity_provider(f: fn() -> Option<Identity>) {
    let _ = IDENTITY_PROVIDER.set(f);
}

/// 读当前身份（未注入或不在 scope 内 → None）。
pub fn current_identity() -> Option<Identity> {
    IDENTITY_PROVIDER.get().and_then(|f| f())
}
