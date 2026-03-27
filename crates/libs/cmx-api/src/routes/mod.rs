//! 通用 route注册 模块
//!

pub mod crud_handlers;
pub mod macros;
pub mod routes;

// 注意：register_crud_routes 宏通过 #[macro_export] 自动导出到 crate 根目录
// 使用时直接通过 cmx_api::register_crud_routes! 访问
