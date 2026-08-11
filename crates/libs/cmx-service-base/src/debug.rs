//! 调试会话管理器初始化（feature `debug`）。
//!
//! 从 web-server `lib.rs` 提取的纯全局单例：`cmx_debug::init()`（起后台清理线程）。幂等。

use tracing::info;

/// 初始化全局调试会话管理器（幂等）。
pub fn init_debug() {
    cmx_debug::init();
    info!("调试会话管理器初始化完成");
}
