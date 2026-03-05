//! 插件注册表：加载插件定义、解析 ZIP 装配清单、验签；不执行、不实例化 WASM

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json;
use thiserror::Error;

use crate::model::cell::TableDefine;
use crate::model::meta::base::TableDefinesConfigManager;

use super::def::{PluginDefinition, PluginManifest};

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP 错误: {0}")]
    Zip(String),
    #[error("插件错误: {0}")]
    Plugin(String),
    #[error("签名验证失败: {0}")]
    SignatureVerification(String),
}

/// 从 ZIP 加载时的验签配置。
///
/// - 若清单带签名且本配置提供公钥，则用公钥验签；验签失败则加载失败。
/// - 若 `require_signature == true` 且清单未带签名，则加载失败。
#[derive(Debug, Clone)]
pub struct VerifySignatureConfig {
    /// 用于验签的 Ed25519 公钥（可多次调用时复用同一配置）
    pub public_key: VerifyingKey,
    /// 若为 true，则清单必须带签名且验签通过才允许加载
    pub require_signature: bool,
}

impl VerifySignatureConfig {
    /// 从 32 字节公钥构造
    pub fn from_public_key_bytes(bytes: &[u8]) -> Result<Self, PluginError> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| PluginError::SignatureVerification("公钥长度必须为 32 字节".to_string()))?;
        let key = VerifyingKey::from_bytes(&arr)
            .map_err(|e| PluginError::SignatureVerification(format!("公钥无效: {}", e)))?;
        Ok(Self {
            public_key: key,
            require_signature: false,
        })
    }

    /// 从 Base64 编码的公钥字符串构造（无填充或标准 Base64 均可）
    pub fn from_public_key_base64(s: &str) -> Result<Self, PluginError> {
        let bytes = BASE64
            .decode(s.trim())
            .map_err(|e| PluginError::SignatureVerification(format!("公钥 Base64 解码失败: {}", e)))?;
        Self::from_public_key_bytes(&bytes)
    }

    /// 设置是否强制要求清单带签名
    pub fn require_signature(mut self, require: bool) -> Self {
        self.require_signature = require;
        self
    }
}

/// 从 ZIP 根目录读取 manifest.json 并解析为 PluginManifest。
fn read_manifest_from_zip<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<PluginManifest, PluginError> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|_| PluginError::Zip("ZIP 内缺少 manifest.json".to_string()))?;
    let mut buf = Vec::new();
    std::io::copy(&mut manifest_file, &mut buf)?;
    serde_json::from_slice(&buf).map_err(Into::into)
}

/// 使用 Ed25519 公钥验证装配清单签名。
fn verify_manifest_signature(
    manifest: &PluginManifest,
    config: &VerifySignatureConfig,
) -> Result<(), PluginError> {
    if manifest.has_signature() {
        let alg = manifest
            .signature_algorithm
            .as_deref()
            .unwrap_or("")
            .trim();
        if alg != "Ed25519" {
            return Err(PluginError::SignatureVerification(format!(
                "不支持的签名算法: {}，当前仅支持 Ed25519",
                alg
            )));
        }
        let sig_b64 = manifest
            .signature
            .as_deref()
            .ok_or_else(|| PluginError::SignatureVerification("signature 字段为空".to_string()))?;
        let sig_bytes = BASE64
            .decode(sig_b64.trim())
            .map_err(|e| PluginError::SignatureVerification(format!("签名 Base64 解码失败: {}", e)))?;
        let sig = Signature::from_bytes(
            &sig_bytes
                .try_into()
                .map_err(|_| PluginError::SignatureVerification("签名长度必须为 64 字节".to_string()))?,
        );
        let payload = manifest
            .signing_payload_bytes()
            .map_err(|e| PluginError::SignatureVerification(format!("构造验签载荷失败: {}", e)))?;
        config
            .public_key
            .verify(&payload, &sig)
            .map_err(|_| PluginError::SignatureVerification("签名验证未通过，清单可能被篡改".to_string()))?;
        Ok(())
    } else if config.require_signature {
        Err(PluginError::SignatureVerification(
            "清单未包含签名，但当前要求必须验签".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// 已注册的插件条目（仅含定义与根路径，不加载 WASM）
struct RegisteredPlugin {
    def: PluginDefinition,
    base_path: PathBuf,
}

/// 插件注册表：从 JSON 或 ZIP 装配清单加载并注册插件定义，不执行、不实例化 WASM。
pub struct PluginRegistry {
    plugins: Vec<RegisteredPlugin>,
}

impl PluginRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// 从插件定义 JSON 文件加载插件；`base_path` 为 wasm_file / table_config_files 等路径的解析根目录
    pub fn load_definition_from_path(
        &mut self,
        definition_path: &Path,
        base_path: &Path,
    ) -> Result<(), PluginError> {
        let s = std::fs::read_to_string(definition_path)?;
        let def: PluginDefinition = serde_json::from_str(&s)?;
        self.register(def, base_path)
    }

    /// 注册插件定义；`base_path` 为 wasm 与表配置文件的解析根（不读取或实例化 WASM）
    pub fn register(
        &mut self,
        def: PluginDefinition,
        base_path: &Path,
    ) -> Result<(), PluginError> {
        self.plugins.push(RegisteredPlugin {
            def,
            base_path: base_path.to_path_buf(),
        });
        Ok(())
    }

    /// 同 [register]，保留旧命名兼容
    #[inline]
    pub fn register_definition(
        &mut self,
        def: PluginDefinition,
        base_path: &Path,
    ) -> Result<(), PluginError> {
        self.register(def, base_path)
    }

    /// 从插件 ZIP 包加载：读取 ZIP 内 manifest（默认 `manifest.json`），解析装配清单并解压到指定或临时目录后注册。
    /// 若提供 `verify_config`，则对带签名的清单执行验签；验签失败或要求签名但清单未带签名时返回错误。
    pub fn load_from_zip_path(
        &mut self,
        zip_path: &Path,
        extract_to: Option<&Path>,
        verify_config: Option<&VerifySignatureConfig>,
    ) -> Result<PluginDefinition, PluginError> {
        let reader = std::fs::File::open(zip_path)?;
        self.load_from_zip(reader, extract_to, verify_config)
    }

    /// 从 ZIP 字节流加载插件（manifest 约定为 ZIP 根目录下的 `manifest.json`）。
    /// 若提供 `verify_config`，则对带签名的清单执行验签。
    pub fn load_from_zip<R: std::io::Read + std::io::Seek>(
        &mut self,
        zip_reader: R,
        extract_to: Option<&Path>,
        verify_config: Option<&VerifySignatureConfig>,
    ) -> Result<PluginDefinition, PluginError> {
        let mut archive = zip::ZipArchive::new(zip_reader)
            .map_err(|e| PluginError::Zip(e.to_string()))?;

        let manifest = read_manifest_from_zip(&mut archive)?;

        if let Some(cfg) = verify_config {
            verify_manifest_signature(&manifest, cfg)?;
        }

        let base = extract_to
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join(format!("cmx_plugin_{}", manifest.plugin.id)));
        if base.exists() {
            std::fs::remove_dir_all(&base).ok();
        }
        std::fs::create_dir_all(&base)?;

        archive
            .extract(&base)
            .map_err(|e| PluginError::Zip(e.to_string()))?;

        self.register(manifest.plugin.clone(), &base)?;
        Ok(manifest.plugin)
    }

    /// 获取已注册插件数量
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// 按 id 查找插件定义
    pub fn get_definition(&self, id: &str) -> Option<&PluginDefinition> {
        self.plugins.iter().find(|p| p.def.id == id).map(|p| &p.def)
    }

    /// 获取插件根路径（wasm_file、table_config_files 等相对此路径）
    pub fn base_path_for_plugin(&self, plugin_id: &str) -> Option<&Path> {
        self.plugins
            .iter()
            .find(|p| p.def.id == plugin_id)
            .map(|p| p.base_path.as_path())
    }

    /// 收集本插件使用的所有「建表配置」路径（绝对路径）
    pub fn table_config_files_for_plugin(&self, plugin_id: &str) -> Option<Vec<PathBuf>> {
        let p = self.plugins.iter().find(|p| p.def.id == plugin_id)?;
        Some(
            p.def.table_config_files
                .iter()
                .map(|f| p.base_path.join(f))
                .collect(),
        )
    }

    /// 为指定插件按 table_config_files 加载全部表定义（配置内已定义具体表定义文件）
    pub fn load_tables_for_plugin(
        &self,
        plugin_id: &str,
    ) -> Result<Vec<TableDefine>, PluginError> {
        let p = self
            .plugins
            .iter()
            .find(|x| x.def.id == plugin_id)
            .ok_or_else(|| PluginError::Plugin(format!("插件未找到: {}", plugin_id)))?;

        let base = &p.base_path;
        let config_paths: Vec<PathBuf> = p
            .def
            .table_config_files
            .iter()
            .map(|f| base.join(f))
            .collect();
        if config_paths.is_empty() {
            return Ok(Vec::new());
        }
        let config_manager =
            TableDefinesConfigManager::from_config_paths(&config_paths)
                .map_err(|e| PluginError::Plugin(e.to_string()))?;
        config_manager
            .load_all_tables(base)
            .map_err(|e| PluginError::Plugin(e.to_string()))
    }
}
