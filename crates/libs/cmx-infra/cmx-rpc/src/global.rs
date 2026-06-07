//! 全局 RPC 客户端管理
//!
//! 提供全局单例的 RpcClient 访问，避免在各个模块间传递客户端实例。

use std::sync::{Arc, OnceLock};

use cmx_traits::RpcClient;

/// 全局 RPC 客户端
///
/// 使用 OnceLock 实现全局单例，确保 RPC 客户端只初始化一次。
pub struct GlobalRpcClient;

static GLOBAL_RPC_CLIENT: OnceLock<Arc<dyn RpcClient>> = OnceLock::new();

impl GlobalRpcClient {
    /// 设置全局 RPC 客户端
    ///
    /// 只能调用一次，重复调用返回错误。
    pub fn set(client: Arc<dyn RpcClient>) -> Result<(), String> {
        GLOBAL_RPC_CLIENT
            .set(client)
            .map_err(|_| "GlobalRpcClient already initialized".to_string())
    }

    /// 获取全局 RPC 客户端引用
    ///
    /// 必须在 set 之后调用，否则 panic。
    pub fn get() -> &'static Arc<dyn RpcClient> {
        GLOBAL_RPC_CLIENT
            .get()
            .expect("GlobalRpcClient not initialized")
    }

    /// 检查全局 RPC 客户端是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_RPC_CLIENT.get().is_some()
    }
}
