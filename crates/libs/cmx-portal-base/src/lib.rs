//! cmx-portal-base —— 门户系列 crate 的共享基础设施层。
//!
//! 从 `cmx-portal` 下沉而来，承载被 `cmx-portal` / `cmx-form` / `cmx-model` 共用的：
//! - [`config`]  —— 数据根目录解析（`portal.data_root` / `CMX_PORTAL_DATA_ROOT` / `./data`）。
//! - [`error`]   —— 统一错误 [`PortalError`] 及 `impl From<PortalError> for cmx_api_types::Error`。
//! - [`fsutil`]  —— JSON 文件原子读写（临时文件 + rename）。
//! - [`util`]    —— ID/段安全校验、写锁等通用工具。
//!
//! 下沉的根因：`cmx-portal`（含 agent）需依赖 `cmx-form`/`cmx-model`，而后两者又共用上述
//! 基础设施；若基础设施留在 `cmx-portal`，将形成循环依赖。独立成 base crate 即打破环。

pub mod config;
pub mod error;
pub mod fsutil;
pub mod util;

pub use config::data_root;
pub use error::{PortalError, PortalResult};
