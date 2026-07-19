//! 认证上下文请求级传播。
//!
//! 基于 `tokio::task_local!` 在请求入口建立 scope，使请求生命周期内的任意
//! `await` 点都能通过 [`current_auth`] 等函数无参获取当前调用者身份，
//! 无需把 `&SVRContext` 一层层手工透传。
//!
//! # 设计要点
//!
//! - task_local 跨 `.await` 有效，但**不跨 `tokio::spawn` 边界**：脱离请求生命周期的
//!   后台任务无法读到 `current_auth()`，应改用 [`system_auth`]。
//! - 与 `SVRContext.auth_context` 注入路径**并行共存**：现有读 `&SVRContext` 的代码
//!   不受影响，task_local 是新增通道。
//! - 由 HTTP `mw_auth` / gRPC `AuthServer` 在请求入口调用 [`scope`] / [`scope_full`]
//!   包裹整个请求处理过程。
//!
//! # Examples
//!
//! ```
//! use cmx_traits::auth::context_scope::{current_auth, scope, RequestAuth};
//!
//! # async fn example() {
//! // 在任意深层代码中无参获取
//! scope(Some(RequestAuth::empty()), async {
//!     let _maybe_auth = current_auth();
//! }).await;
//! # }
//! ```

use std::sync::Arc;

use cmx_core::AuthContext;
use tokio::sync::OnceCell;
use tokio::task_local;

// task_local 容器：持有当前请求的认证快照。
// 用 `Arc<RequestAuth>` 是为了让 scope 内克隆廉价（task_local 值本身不可变，
// 内部可变性由 `Arc` 共享）。
task_local! {
    // 当前请求的认证快照。`None` 表示已进入 scope 但该请求未认证（如白名单路径）。
    static CURRENT_AUTH: Arc<RequestAuth>;
}

/// 请求级认证快照。
///
/// 由请求入口（`mw_auth` / gRPC `AuthServer`）一次性填充，请求生命周期内只读。
#[derive(Debug, Clone)]
pub struct RequestAuth {
    /// 已验证的认证上下文。M2M 调用可能是服务专用 `AuthContext`（`auth_method=api_key`）。
    pub auth_context: Option<AuthContext>,
    /// 终端用户的原始 JWT（用于跨服务 on-behalf-of 透传）。`None` 表示无用户 JWT。
    pub original_user_token: Option<String>,
    /// 请求 ID（链路追踪）。
    pub request_id: String,
    /// 调用源身份三元组（on-behalf-of 透传）。
    pub caller: Option<CallerIdentity>,
}

impl RequestAuth {
    /// 构造一个空的请求快照（`auth_context` / `original_user_token` / `caller` 为 `None`）。
    pub fn empty() -> Self {
        Self {
            auth_context: None,
            original_user_token: None,
            request_id: String::new(),
            caller: None,
        }
    }
}

/// 调用源身份三元组（on-behalf-of 透传）。
#[derive(Debug, Clone)]
pub struct CallerIdentity {
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
}

/// 全局系统身份（启动期由 `web-server` 注入）。
///
/// 供脱离请求生命周期的后台任务（`tokio::spawn` 启动的集群同步、定时轮询等）
/// 通过 [`system_auth`] 获取一个系统级 `AuthContext`，避免到处判 `None`。
static SYSTEM_AUTH: OnceCell<Arc<RequestAuth>> = OnceCell::const_new();

/// 初始化全局系统身份。
///
/// 启动期调用一次（由 `web-server` 在装配阶段执行）。重复调用返回错误，
/// 符合第十七章「初始化返回 `Result`，禁止 panic」。
///
/// # Errors
///
/// 当系统身份已被初始化过时返回 [`crate::error::SetSystemAuthError`]。
pub fn init_system_auth(snap: RequestAuth) -> Result<(), crate::error::SetSystemAuthError> {
    SYSTEM_AUTH
        .set(Arc::new(snap))
        .map_err(|_| crate::error::SetSystemAuthError)
}

/// 获取全局系统身份（后台任务兜底用）。
///
/// 在后台任务中推荐：`current_auth().or_else(system_auth)`。
pub fn system_auth() -> Option<AuthContext> {
    SYSTEM_AUTH.get().and_then(|s| s.auth_context.clone())
}

// ========================================================
// 请求级访问 API（task_local 读取）
// ========================================================

/// 在当前异步任务内获取 `AuthContext`。
///
/// 需在 [`scope`] / [`scope_full`] 作用域内调用；不在 scope 内返回 `None`
/// （不 panic，调用方自行处理）。
///
/// 适用场景：service 层深层代码、审计助手、编排节点等。
pub fn current_auth() -> Option<AuthContext> {
    CURRENT_AUTH
        .try_with(|v| v.auth_context.clone())
        .ok()
        .flatten()
}

/// 当前请求的原始终端用户 JWT（用于跨服务 on-behalf-of 透传）。
pub fn current_original_token() -> Option<String> {
    CURRENT_AUTH
        .try_with(|v| v.original_user_token.clone())
        .ok()
        .flatten()
}

/// 当前请求 ID。
pub fn current_request_id() -> Option<String> {
    CURRENT_AUTH.try_with(|v| v.request_id.clone()).ok()
}

/// 当前调用源身份。
pub fn current_caller() -> Option<CallerIdentity> {
    CURRENT_AUTH.try_with(|v| v.caller.clone()).ok().flatten()
}

// ========================================================
// scope 建立入口
// ========================================================

/// 在给定认证快照的作用域内执行 future（基础入口）。
///
/// 由 HTTP `mw_auth` / gRPC `AuthServer` 在请求入口调用，包裹整个请求处理过程，
/// 使内部任意 `await` 点都能通过 [`current_auth`] 等函数无参读取。
///
/// 如需同时透传用户 JWT / request_id / caller，改用 [`scope_full`]。
pub async fn scope<F, R>(auth: Option<AuthContext>, fut: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let snap = RequestAuth {
        auth_context: auth,
        ..RequestAuth::empty()
    };
    CURRENT_AUTH.scope(Arc::new(snap), fut).await
}

/// 在给定完整认证快照的作用域内执行 future（增强入口）。
///
/// 相比 [`scope`]，同时携带原始用户 token、request_id、caller，供跨服务调用时透传。
pub async fn scope_full<F, R>(
    auth: Option<AuthContext>,
    original_user_token: Option<String>,
    request_id: String,
    caller: Option<CallerIdentity>,
    fut: F,
) -> R
where
    F: std::future::Future<Output = R>,
{
    let snap = RequestAuth {
        auth_context: auth,
        original_user_token,
        request_id,
        caller,
    };
    CURRENT_AUTH.scope(Arc::new(snap), fut).await
}

/// 在当前 scope 内以闭包访问 [`RequestAuth`] 快照。
///
/// 供需要在 scope 内一次性读取多个字段的场景（避免多次 `try_with`）。
/// 不在 scope 内时返回 `None`。
pub fn with_request_auth<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&RequestAuth) -> T,
{
    CURRENT_AUTH.try_with(|v| f(v)).ok()
}
