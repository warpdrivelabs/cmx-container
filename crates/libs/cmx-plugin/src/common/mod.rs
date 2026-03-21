//! 通用工具模块
//!
//! 提供插件管理各服务共用的工具函数，消除代码重复。

pub mod package;
pub mod definition;
pub mod dependency;
pub mod service;

pub use package::{PackageUtils, PackageUtilsDeps};
pub use definition::DefinitionUtils;
pub use dependency::{DependencyUtils, DependencyUtilsDeps};
pub use service::{ServiceUtils, ServiceUtilsDeps};
