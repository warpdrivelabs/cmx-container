//! cmx-mdm-store-pg 错误助手。
//!
//! 公共错误构造（`api_err`/`api_err_db`/`pg_detail`）已上提到 `cmx_biz::error`，
//! 此处 re-export 保持本 crate 调用点零改动。

// 公共错误助手重导出（向后兼容：本 crate 内 `api_err`/`api_err_db` 调用点零改动）。
pub use cmx_biz::{api_err, api_err_db};
