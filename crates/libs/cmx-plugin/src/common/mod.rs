//! 通用工具模块
//!
//! 提供插件管理各服务共用的工具函数，消除代码重复。

pub mod definition;
pub mod dependency;
pub mod package;
pub mod scanner;
pub mod service;
pub mod source_utils;

pub use definition::DefinitionUtils;
pub use dependency::{DependencyUtils, DependencyUtilsDeps};
pub use package::{PackageUtils, PackageUtilsDeps};
pub use scanner::scan_local_plugins;
pub use service::{ServiceUtils, ServiceUtilsDeps};
pub use source_utils::{build_plugin_source, extract_source_info};
