//! 加密服务初始化（feature `crypto`）。
//!
//! 从 web-server `lib.rs` 提取的纯全局单例：`CryptoService::init_from_env()`（读 env `CMX_ENCRYPT_KEY`）。
//! 数据源 db_url 解密依赖它，故须在 datasource 之前调。幂等。

use tracing::info;

/// 从环境初始化全局加密服务（幂等）。
pub fn init_crypto() {
    cmx_utils::crypto::CryptoService::init_from_env();
    info!("加密服务初始化完成");
}
