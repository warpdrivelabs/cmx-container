//! 插件管理：插件定义 JSON、ZIP 装配清单、表配置与表定义关联（不执行、不实例化 WASM）
//!
//! - **插件定义**：[PluginDefinition] / [PluginManifest] 见 [def]。
//! - **注册表**：[PluginRegistry] 见 [registry]，负责加载、注册与验签。

pub mod def;
pub mod registry;

pub use def::{PluginDefinition, PluginManifest, PluginManifestSigningPayload, supported_db, supported_lang};
pub use registry::{PluginError, PluginRegistry, VerifySignatureConfig};
