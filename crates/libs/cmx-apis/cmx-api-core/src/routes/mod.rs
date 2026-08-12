//! 通用 route 注册模块（从 cmx-api 迁入）。
//!
//! 仅含通用骨架：
//! - `traits`：`ModuleRoutes` trait
//! - `macros`：`declare_crud_handlers!` / `register_crud_routes!` 等 CRUD 宏（`#[macro_export]`）
//!
//! 注：原 cmx-api 的 `crud_handlers.rs`（具体实体的宏调用）与 `routes_impl.rs`（api_routes
//! 聚合）仍留在 cmx-api，因其绑定具体 handler。

pub mod macros;
pub mod traits;
