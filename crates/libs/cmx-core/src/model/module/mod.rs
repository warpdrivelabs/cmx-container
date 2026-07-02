//! 模块相关模型
//!
//! 模块迁移包清单(ModuleManifest)等数据结构

pub mod manifest;

pub use manifest::{ModuleInfo, ModuleManifest, ModulePluginEntry, ModuleResources, ModuleStats};
