//! 全局事件总线初始化（feature `event-bus`）。
//!
//! 从 web-server `lib.rs` 提取的纯全局单例：`GlobalEventBus::initialize()`。配置后可浮动。

use tracing::info;

use crate::{BaseError, Result};

/// 初始化全局事件总线。
pub fn init_event_bus() -> Result<()> {
    cmx_traits::event_bus::GlobalEventBus::initialize()
        .map_err(|e| BaseError::Setup(format!("初始化全局事件总线失败: {e}")))?;
    info!("全局事件总线初始化完成");
    Ok(())
}
