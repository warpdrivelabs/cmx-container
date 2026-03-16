//! cmx-plugin — 插件注册表、ZIP 加载、签名验证
//!
//! 基础结构体（PluginDefinition、PluginManifest 等）定义在 cmx-core 中。
//! 本 crate 提供插件管理的具体实现。

pub mod error;
pub mod registry;

pub use error::PluginError;
pub use registry::{PluginRegistry, VerifySignatureConfig};
pub use cmx_core::model::meta::plugin::{
    PluginDefinition, PluginManifest, PluginManifestSigningPayload,
    supported_db, supported_lang,
};
