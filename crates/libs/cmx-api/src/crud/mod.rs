//! 通用 CRUD 模块
//!
//! 提供通用的 CRUD 功能，支持创建、读取、更新、删除操作。

pub mod traits;
pub mod utils;
pub mod service;
pub mod macros;

pub use traits::DbBmc;
pub use service::GenericCrudService;
