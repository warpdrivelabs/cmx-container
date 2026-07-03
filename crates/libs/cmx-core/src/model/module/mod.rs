//! 模块相关模型
//!
//! 模块迁移包清单(ModuleManifest)与资源定义契约(definitions)等数据结构

pub mod definitions;
pub mod manifest;

pub use definitions::{FormDefinition, MenuDefinition};
pub use manifest::{ModuleInfo, ModuleManifest, ModulePluginEntry, ModuleResources, ModuleStats};
