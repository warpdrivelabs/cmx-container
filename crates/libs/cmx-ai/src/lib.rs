//! cmx-ai —— AI 生成能力中继层（一期薄代理）。
//!
//! 作为前端与 [OpenCode](https://opencode.ai)（:4096）之间的纯转发层，职责：
//! - [`opencode_client`]：用 reqwest 封装 OpenCode 的 session / prompt_async / question / permission
//!   等接口，统一携带访问凭证（`OPENCODE_SERVER_PASSWORD`）。
//! - [`sse_relay`]：维护**一条**到 OpenCode `GET /event` 的全局 SSE 长连接，按事件载荷的
//!   `sessionID` 分发到各前端订阅，并把 OpenCode 原生事件翻译为简化的 cmx-ai 事件。
//! - [`session_registry`]：前端订阅路由表（`{opencode ses_* → Vec<前端 mpsc sender>}`）
//!   与待处理 question/permission id 管理。
//!
//! # 一期边界（薄代理）
//! - 不持久化会话；会话即 OpenCode 的 session，刷新即丢。
//! - 不保存生成产物，仅经 SSE 返回前端展示。
//! - 会话 id（`sid`）直接透传 OpenCode 的 `ses_*`；二期引入 cmx-ai 自有 id 时前端无感。
//!
//! # 启动
//! web-server 启动期调用 [`init_ai_subsystem`] 完成全局 SessionRegistry 初始化与
//! 后台 relay task 拉起；HTTP 路由经 `cmx-api::handlers::ai` 注册到 `/api/ai/*`。

pub mod config;
pub mod error;
pub mod opencode_client;
pub mod session_registry;
pub mod sse_relay;
pub mod types;

pub use config::{load_config, OpenCodeConfig};
pub use error::{AiError, AiResult};
pub use opencode_client::OpenCodeClient;
pub use session_registry::{AiSseEvent, SessionRegistry};
pub use sse_relay::start_global_relay;

use std::sync::OnceLock;

use tokio::sync::OnceCell;

/// 全局 SessionRegistry 单例（前端订阅路由表 + pending id 管理）。
static REGISTRY: OnceCell<SessionRegistry> = OnceCell::const_new();

/// 全局 OpenCode 客户端单例（复用连接池）。
static CLIENT: OnceCell<OpenCodeClient> = OnceCell::const_new();

/// 已初始化标志（供 init 幂等检查）。
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// 初始化 AI 子系统：加载配置、构建全局客户端、拉起后台 SSE relay task。
///
/// 在 web-server 启动阶段（app_state 构建后、HTTP listen 前）调用一次。
/// 多次调用幂等：仅首次实际执行初始化。
///
/// # 总开关 `opencode.enabled`（默认关闭）
/// 配置为 `false`（或未配置）时直接返回——不建客户端、不拉 SSE relay task，
/// 避免后台日志永久刷「无法连接 OpenCode」错误。此时 `/api/ai/*` 接口统一返回 503。
/// 需启用时配置 `opencode.enabled = true`（或设环境变量 `OPENCODE_ENABLED=true`），
/// 并确保 OpenCode 服务已部署在 `opencode.base_url`。
///
/// # Panics
/// 内部 spawn 失败会记录日志但不 panic；配置缺失时以默认值（`http://127.0.0.1:4096`）启动。
pub async fn init_ai_subsystem() {
    if INITIALIZED.get().is_some() {
        return;
    }

    let config = load_config();

    // 总开关：默认关闭。关闭时跳过所有初始化，handler 层 client()/registry() 将返回 None。
    if !config.enabled {
        tracing::info!(
            "cmx-ai 子系统未启用（opencode.enabled=false）。/api/ai/* 将返回 503。\
             如需启用，请配置 opencode.enabled=true 并确保 OpenCode 服务已部署。"
        );
        // 仍标记为已初始化，避免重复进入这段日志。
        let _ = INITIALIZED.set(());
        return;
    }

    let client = OpenCodeClient::new(config.clone());

    // 先占位初始化全局单例，供 handler 层取用。
    // tokio::sync::OnceCell::set 是同步方法（返回 Result，非 future），无需 .await。
    let _ = REGISTRY.set(SessionRegistry::new());
    let _ = CLIENT.set(client.clone());

    // 拉起后台全局 SSE relay task（到 OpenCode GET /event 的长连接 + 事件分发）。
    start_global_relay(client).await;

    let _ = INITIALIZED.set(());
    tracing::info!(
        base_url = %config.base_url,
        "cmx-ai 子系统已初始化（薄代理模式，SSE relay 已启动）"
    );
}

/// 获取全局 SessionRegistry（handler 层订阅/取消订阅/pending id 用）。
///
/// 返回 `None` 表示尚未调用 [`init_ai_subsystem`]。
pub fn registry() -> Option<&'static SessionRegistry> {
    REGISTRY.get()
}

/// 获取全局 OpenCode 客户端（handler 层转发请求用）。
///
/// 返回 `None` 表示尚未调用 [`init_ai_subsystem`]。
pub fn client() -> Option<&'static OpenCodeClient> {
    CLIENT.get()
}

/// [`registry`] 的别名（handler 层常用 `get_registry()` 语义）。
pub fn get_registry() -> Option<&'static SessionRegistry> {
    REGISTRY.get()
}

/// [`client`] 的别名（handler 层常用 `get_client()` 语义）。
pub fn get_client() -> Option<&'static OpenCodeClient> {
    CLIENT.get()
}
