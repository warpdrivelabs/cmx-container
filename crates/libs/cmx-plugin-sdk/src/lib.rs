//! cmx-plugin-sdk — CMX 插件开发 SDK
//!
//! 基于 Extism PDK 的插件开发 SDK，提供：
//! - 宿主函数调用封装
//! - 插件函数导出宏
//! - 错误类型定义
//! - 工具函数

pub mod host_calls;
pub mod error;

pub use extism_pdk::*;
pub use host_calls::{
    HostCaller,
    DbQueryRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    ServiceCallRequest, ServiceCallResponse,
};
pub use error::PluginError;
