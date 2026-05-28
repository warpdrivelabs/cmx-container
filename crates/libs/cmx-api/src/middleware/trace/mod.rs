//! 高性能请求追踪中间件。
//!
//! 根据运行时日志级别自动选择追踪模式：
//!
//! - **INFO 模式**（生产环境）：仅记录方法、路径、查询参数、状态码、耗时，零额外开销
//! - **DEBUG 模式**（开发调试）：记录完整请求头、请求体、响应体（脱敏），排除文件上传下载
//!
//! 与旧 `mw_trace` 的区别：
//!
//! - INFO 级别不读取请求体/响应体，不解析 JSON，不做脱敏处理
//! - DEBUG 级别增加文件下载排除检测
//! - 通过 `RUST_LOG` 环境变量运行时感知日志级别，无配置负担

pub mod config;
pub mod detector;
pub mod layer;
pub mod sanitizer;

pub use config::{TraceConfig, TraceMode};
pub use layer::trace_layer;
