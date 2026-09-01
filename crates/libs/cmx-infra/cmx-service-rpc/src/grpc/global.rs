//! 全局 RPC 初始化状态守卫。
//!
//! 领域客户端各自维护 [`OnceLock`] 全局单例（见 `cmx-rpcs/*` 皮肤 crate，如
//! `cmx-orchestrator-rpc::orchestrator_client`）。[`GlobalRpcClient`] 仅跟踪整体
//! 初始化状态，提供 [`GlobalRpcClient::is_initialized`] 守卫供调用方在调用前检查。

use std::sync::OnceLock;

/// 全局 RPC 客户端已初始化错误。
///
/// 当重复初始化（[`GlobalRpcClient::mark_initialized`]）时返回此错误。
#[derive(thiserror::Error, Debug)]
#[error("GlobalRpcClient 已初始化")]
pub struct GlobalRpcClientAlreadySetError;

/// 全局 RPC 客户端（初始化状态守卫）。
pub struct GlobalRpcClient;

static INITIALIZED: OnceLock<()> = OnceLock::new();

impl GlobalRpcClient {
    /// 标记全局 RPC 已初始化。
    ///
    /// 由 [`crate::grpc::factory::init_rpc_clients`] 在所有 Bundle 客户端初始化完成后调用。
    /// 只能调用一次，重复调用返回 [`GlobalRpcClientAlreadySetError`]。
    pub(crate) fn mark_initialized() -> Result<(), GlobalRpcClientAlreadySetError> {
        INITIALIZED
            .set(())
            .map_err(|_| GlobalRpcClientAlreadySetError)
    }

    /// 检查全局 RPC 客户端是否已初始化。
    pub fn is_initialized() -> bool {
        INITIALIZED.get().is_some()
    }
}
