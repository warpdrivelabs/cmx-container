//! 模块路由 Trait
//!
//! 各 handler 模块实现此 trait 来定义自己的路由

use crate::app_state::CmxAppState;
use axum::Router;

/// 模块路由 Trait
///
/// 各 handler 模块实现此 trait 来定义自己的路由
pub trait ModuleRoutes {
    /// 注册该模块的路由
    fn routes(self) -> Router<CmxAppState>;

    /// 获取模块前缀路径
    fn prefix() -> &'static str;

    /// 获取模块名称（用于日志/调试）
    fn module_name(&self) -> &'static str;
}
