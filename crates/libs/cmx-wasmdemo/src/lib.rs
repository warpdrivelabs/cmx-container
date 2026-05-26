//! WASM 插件示例模块。
//!
//! 该模块提供了用于展示 WASM 插件功能的基础实现，
//! 展示了日志、缓存、数据库、插件调用和服务编排等核心能力。
//!
//! # 主要功能
//!
//! * `core::PluginCore` - 插件核心实现，包含各种功能函数
//! * `host_traits::HostFunctions` - 宿主功能 trait，定义了插件可调用的宿主能力
//! * `models` - 公共数据模型定义
//!
//! # 使用方式
//!
//! 该 crate 配合 `extism` feature 使用时，通过 `extism_layer` 模块
//! 将插件功能暴露给 Extism 运行时。

pub mod models;
pub mod host_traits;
pub mod core;

#[cfg(test)]
pub mod tests;

#[cfg(feature = "extism")]
pub mod extism_layer;