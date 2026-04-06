//! cmx-wasmdemo - WASM 宿主函数演示模块
//!
//! 本模块用于验证 WASM 宿主函数功能，提供各种演示函数。
//!
//! # 编译目标
//!
//! 使用 `wasm32-wasip1` 目标编译：
//! ```bash
//! cargo build --release --target wasm32-wasip1
//! ```
//!
//! # 导出函数
//!
//! - `demo_log()` - 演示日志功能
//! - `demo_cache()` - 演示缓存功能
//! - `demo_database()` - 演示数据库功能
//! - `demo_plugin_info()` - 演示插件信息获取
//! - `demo_call_service()` - 演示插件间调用
//! - `run_all_demos()` - 综合测试入口

mod host_funcs;
mod memory;
mod demo;

// 重新导出演示函数
pub use demo::*;
pub use memory::{alloc, dealloc};
