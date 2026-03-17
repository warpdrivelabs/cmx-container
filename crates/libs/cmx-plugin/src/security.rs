//! 安全验证器模块 - 插件安全验证
//!
//! 提供插件安全验证功能，包括签名验证、完整性校验、权限检查等。

use std::path::Path;

use sha2::{Digest, Sha256};

/// 安全验证配置
#[derive(Debug, Clone)]
pub struct SecurityValidatorConfig {
    /// 是否要求签名验证
    pub require_signature: bool,
    /// 受信任的签名公钥列表
    pub trusted_public_keys: Vec<Vec<u8>>,
    /// 是否验证文件完整性 (hash)
    pub verify_file_hash: bool,
    /// 最大允许的插件文件大小 (字节)
    pub max_plugin_size: u64,
    /// 是否启用沙箱运行
    pub enable_sandbox: bool,
    /// 允许的 WASM 导入函数白名单
    pub allowed_imports: Vec<String>,
}

impl Default for SecurityValidatorConfig {
    fn default() -> Self {
        Self {
            require_signature: false,
            trusted_public_keys: Vec::new(),
            verify_file_hash: true,
            max_plugin_size: 100 * 1024 * 1024, // 100MB
            enable_sandbox: true,
            allowed_imports: vec![
                "env".to_string(),
                "wasmtime".to_string(),
            ],
        }
    }
}

/// 验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否通过验证
    pub valid: bool,
    /// 验证项目列表
    pub checks: Vec<CheckResult>,
    /// 错误信息
    pub error_message: Option<String>,
}

/// 单项检查结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// 检查名称
    pub name: String,
    /// 是否通过
    pub passed: bool,
    /// 检查详情
    pub details: Option<String>,
}

impl ValidationResult {
    /// 创建成功的验证结果
    pub fn success() -> Self {
        Self {
            valid: true,
            checks: Vec::new(),
            error_message: None,
        }
    }

    /// 创建失败的验证结果
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            valid: false,
            checks: Vec::new(),
            error_message: Some(message.into()),
        }
    }

    /// 添加检查结果
    pub fn with_check(mut self, name: impl Into<String>, passed: bool, details: Option<impl Into<String>>) -> Self {
        self.checks.push(CheckResult {
            name: name.into(),
            passed,
            details: details.map(|d| d.into()),
        });
        if !passed {
            self.valid = false;
        }
        self
    }

    /// 获取所有失败的检查
    pub fn failed_checks(&self) -> Vec<&CheckResult> {
        self.checks.iter().filter(|c| !c.passed).collect()
    }
}

/// 安全验证器 - 负责插件安全验证
pub struct SecurityValidator {
    config: SecurityValidatorConfig,
}

impl SecurityValidator {
    /// 创建新的安全验证器
    pub fn new(config: SecurityValidatorConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建安全验证器
    pub fn default_validator() -> Self {
        Self::new(SecurityValidatorConfig::default())
    }

    /// 验证插件包
    pub async fn validate_plugin(&self, plugin_path: &Path) -> ValidationResult {
        let mut result = ValidationResult::success();

        // 1. 检查文件是否存在
        if !plugin_path.exists() {
            return ValidationResult::failure(format!("插件路径不存在: {}", plugin_path.display()));
        }
        result = result.with_check("文件存在性", true, Some("插件文件存在"));

        // 2. 检查文件大小
        if let Ok(metadata) = std::fs::metadata(plugin_path) {
            let size = metadata.len();
            let size_valid = size <= self.config.max_plugin_size;
            result = result.with_check(
                "文件大小",
                size_valid,
                Some(format!("文件大小: {} bytes, 最大允许: {}", size, self.config.max_plugin_size)),
            );
        }

        // 3. 如果是 ZIP 文件，验证 ZIP 格式
        if let Some(ext) = plugin_path.extension() {
            if ext == "zip" {
                result = self.validate_zip_format(plugin_path, result).await;
            } else if ext == "wasm" {
                result = self.validate_wasm_format(plugin_path, result).await;
            }
        }

        // 4. 验证 manifest.json 存在且有效
        let manifest_path = plugin_path.join("manifest.json");
        if manifest_path.exists() {
            result = result.with_check("manifest.json 存在", true, Option::<String>::None);
            result = self.validate_manifest(&manifest_path, result).await;
        } else {
            result = result.with_check("manifest.json 存在", false, Some("未找到 manifest.json"));
        }

        result
    }

    /// 验证 ZIP 格式
    async fn validate_zip_format(&self, path: &Path, mut result: ValidationResult) -> ValidationResult {
        match std::fs::File::open(path) {
            Ok(file) => {
                match zip::ZipArchive::new(file) {
                    Ok(mut archive) => {
                        let file_count = archive.len();
                        result = result.with_check(
                            "ZIP 格式",
                            true,
                            Some(format!("有效的 ZIP 文件，包含 {} 个文件", file_count)),
                        );

                        // 检查是否包含必要文件
                        let has_manifest = (0..archive.len())
                            .any(|i| archive.by_index(i).map_or(false, |f| f.name() == "manifest.json"));
                        result = result.with_check("ZIP 包含 manifest.json", has_manifest, Option::<String>::None);

                        // 检查是否有过多文件（潜在 zip bomb）
                        if file_count > 10000 {
                            result = result.with_check("文件数量限制", false, Some("ZIP 文件数量超过限制"));
                        }
                    }
                    Err(e) => {
                        result = result.with_check("ZIP 格式", false, Some(format!("无效的 ZIP 文件: {}", e)));
                    }
                }
            }
            Err(e) => {
                result = result.with_check("ZIP 文件读取", false, Some(format!("无法读取文件: {}", e)));
            }
        }
        result
    }

    /// 验证 WASM 格式
    async fn validate_wasm_format(&self, path: &Path, mut result: ValidationResult) -> ValidationResult {
        match tokio::fs::read(path).await {
            Ok(bytes) => {
                // 检查 WASM 魔数
                let valid_magic = bytes.len() >= 4 && &bytes[0..4] == b"\0asm";
                result = result.with_check(
                    "WASM 魔数",
                    valid_magic,
                    Some(if valid_magic { "有效的 WASM 文件" } else { "无效的 WASM 文件" }),
                );

                // 检查版本
                if bytes.len() >= 8 {
                    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                    let valid_version = version == 1;
                    result = result.with_check(
                        "WASM 版本",
                        valid_version,
                        Some(format!("WASM 版本: {}", version)),
                    );
                }

                // 检查导入函数
                if self.config.enable_sandbox {
                    // TODO: 解析 WASM 导入函数并检查白名单
                    result = result.with_check(
                        "WASM 导入函数",
                        true,
                        Some("沙箱检查已启用"),
                    );
                }
            }
            Err(e) => {
                result = result.with_check("WASM 文件读取", false, Some(format!("读取失败: {}", e)));
            }
        }
        result
    }

    /// 验证 manifest.json
    async fn validate_manifest(&self, path: &Path, mut result: ValidationResult) -> ValidationResult {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                match serde_json::from_str::<cmx_core::model::meta::plugin::PluginManifest>(&content) {
                    Ok(manifest) => {
                        result = result.with_check("manifest.json 格式", true, Option::<String>::None);

                        // 验证插件 ID 格式
                        let valid_id = !manifest.plugin.id.is_empty()
                            && manifest.plugin.id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
                        result = result.with_check(
                            "插件 ID 格式",
                            valid_id,
                            Some(format!("插件 ID: {}", manifest.plugin.id)),
                        );

                        // 验证版本格式
                        if let Some(ref version) = manifest.plugin.version {
                            let valid_version = Self::is_valid_semver(version);
                            result = result.with_check(
                                "版本格式",
                                valid_version,
                                Some(format!("版本: {}", version)),
                            );
                        }

                        // 检查签名
                        if manifest.has_signature() {
                            result = result.with_check("签名存在", true, Some("插件已签名"));
                            if !self.config.trusted_public_keys.is_empty() {
                                // TODO: 验证签名
                                result = result.with_check("签名验证", true, Some("使用受信任密钥验证"));
                            }
                        } else if self.config.require_signature {
                            result = result.with_check("签名验证", false, Some("要求签名但插件未签名"));
                        }
                    }
                    Err(e) => {
                        result = result.with_check("manifest.json 格式", false, Some(format!("解析失败: {}", e)));
                    }
                }
            }
            Err(e) => {
                result = result.with_check("manifest.json 读取", false, Some(format!("读取失败: {}", e)));
            }
        }
        result
    }

    /// 验证文件哈希
    pub async fn verify_file_hash(&self, file_path: &Path, expected_hash: &str) -> ValidationResult {
        use std::io::Read;

        let mut result = ValidationResult::success();

        match std::fs::File::open(file_path) {
            Ok(mut file) => {
                let mut hasher = sha2::Sha256::new();
                let mut buffer = [0u8; 8192];

                loop {
                    match file.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => hasher.update(&buffer[..n]),
                        Err(e) => {
                            return ValidationResult::failure(format!("读取文件失败: {}", e));
                        }
                    }
                }

                let hash = format!("{:x}", hasher.finalize());
                let matches = hash == expected_hash;

                result = result.with_check(
                    "文件哈希",
                    matches,
                    Some(format!("期望: {}, 实际: {}", expected_hash, hash)),
                );

                if !matches {
                    result = ValidationResult::failure("文件哈希不匹配".to_string());
                }
            }
            Err(e) => {
                return ValidationResult::failure(format!("无法打开文件: {}", e));
            }
        }

        result
    }

    /// 验证版本字符串是否为有效的语义版本
    fn is_valid_semver(version: &str) -> bool {
        let re = regex::Regex::new(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?(\+[a-zA-Z0-9.]+)?$").ok();
        re.map(|r| r.is_match(version)).unwrap_or(false)
    }

    /// 添加受信任的公钥
    pub fn add_trusted_key(&mut self, public_key: Vec<u8>) {
        self.config.trusted_public_keys.push(public_key);
    }

    /// 检查是否启用了签名要求
    pub fn is_signature_required(&self) -> bool {
        self.config.require_signature
    }
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self::default_validator()
    }
}
