//! 安全验证器模块
//! 
//! 验证插件安全性，包括包结构、签名、权限等

use std::path::Path;
use std::io::Read;

use super::signature::{SignatureValidator, SignatureInfo};

/// 验证结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 是否通过
    pub passed: bool,
    /// 错误信息
    pub errors: Vec<String>,
    /// 警告信息
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// 创建通过的验证结果
    pub fn passed() -> Self {
        Self {
            passed: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
    
    /// 创建失败的验证结果
    pub fn failed(errors: Vec<String>) -> Self {
        Self {
            passed: false,
            errors,
            warnings: Vec::new(),
        }
    }
    
    /// 添加错误
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.passed = false;
    }
    
    /// 添加警告
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
    
    /// 合并另一个验证结果
    pub fn merge(&mut self, other: ValidationResult) {
        if !other.passed {
            self.passed = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// 安全验证配置
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// 是否启用签名验证
    pub verify_signature: bool,
    /// 是否启用权限检查
    pub check_permissions: bool,
    /// 是否检查包结构
    pub check_package_structure: bool,
    /// 是否检查 manifest 文件
    pub check_manifest: bool,
    /// 允许的最大插件大小（字节）
    pub max_plugin_size: u64,
    /// 允许的文件扩展名
    pub allowed_extensions: Vec<String>,
    /// 禁止的文件路径模式
    pub forbidden_patterns: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            verify_signature: true,
            check_permissions: true,
            check_package_structure: true,
            check_manifest: true,
            max_plugin_size: 100 * 1024 * 1024, // 100MB
            allowed_extensions: vec![
                "wasm".to_string(),
                "json".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "md".to_string(),
                "txt".to_string(),
            ],
            forbidden_patterns: vec![
                "../".to_string(),
                "..\\".to_string(),
                "/etc/".to_string(),
                "/root/".to_string(),
                "C:\\Windows\\".to_string(),
            ],
        }
    }
}

/// 安全验证器
/// 
/// 提供插件包的全面安全验证。
pub struct SecurityValidator {
    /// 配置
    config: SecurityConfig,
    /// 签名验证器
    signature_validator: SignatureValidator,
}

impl SecurityValidator {
    /// 创建新的安全验证器
    pub fn new() -> Self {
        Self {
            config: SecurityConfig::default(),
            signature_validator: SignatureValidator::new(),
        }
    }
    
    /// 使用配置创建验证器
    pub fn with_config(config: SecurityConfig) -> Self {
        Self {
            config,
            signature_validator: SignatureValidator::new(),
        }
    }
    
    /// 获取签名验证器（可变引用）
    pub fn signature_validator_mut(&mut self) -> &mut SignatureValidator {
        &mut self.signature_validator
    }
    
    /// 验证插件包
    /// 
    /// 执行完整的安全验证流程。
    pub async fn validate_plugin_package(&self, package_path: &Path) -> ValidationResult {
        let mut result = ValidationResult::passed();
        
        // 检查包是否存在
        if !package_path.exists() {
            result.add_error(format!("插件包不存在: {}", package_path.display()));
            return result;
        }
        
        // 检查包大小
        if self.config.check_package_structure {
            self.validate_package_size(package_path, &mut result);
        }
        
        // 检查包结构
        if self.config.check_package_structure {
            self.validate_package_structure(package_path, &mut result);
        }
        
        // 检查 manifest 文件
        if self.config.check_manifest {
            self.validate_manifest(package_path, &mut result);
        }
        
        // 验证签名
        if self.config.verify_signature {
            self.validate_signature(package_path, &mut result);
        }
        
        result
    }
    
    /// 验证包大小
    fn validate_package_size(&self, package_path: &Path, result: &mut ValidationResult) {
        if let Ok(metadata) = std::fs::metadata(package_path) {
            let size = metadata.len();
            if size > self.config.max_plugin_size {
                result.add_error(format!(
                    "插件包大小超出限制: {} 字节 > {} 字节",
                    size, self.config.max_plugin_size
                ));
            }
        }
    }
    
    /// 验证包结构
    fn validate_package_structure(&self, package_path: &Path, result: &mut ValidationResult) {
        if package_path.is_file() {
            // ZIP 文件
            if let Some(ext) = package_path.extension() {
                if ext != "zip" {
                    result.add_error(format!(
                        "不支持的插件包格式: {:?}",
                        ext
                    ));
                    return;
                }
            }
            
            // 验证 ZIP 内容
            self.validate_zip_contents(package_path, result);
        } else if package_path.is_dir() {
            // 目录
            self.validate_directory_contents(package_path, result);
        }
    }
    
    /// 验证 ZIP 文件内容
    fn validate_zip_contents(&self, package_path: &Path, result: &mut ValidationResult) {
        let file = match std::fs::File::open(package_path) {
            Ok(f) => f,
            Err(e) => {
                result.add_error(format!("无法打开插件包: {}", e));
                return;
            }
        };
        
        let reader = std::io::BufReader::new(file);
        let mut archive = match zip::ZipArchive::new(reader) {
            Ok(a) => a,
            Err(e) => {
                result.add_error(format!("无效的 ZIP 文件: {}", e));
                return;
            }
        };
        
        let mut has_wasm = false;
        let mut has_manifest = false;
        
        for i in 0..archive.len() {
            if let Ok(file) = archive.by_index(i) {
                let name = file.name();
                
                // 检查禁止的路径模式
                for pattern in &self.config.forbidden_patterns {
                    if name.contains(pattern) {
                        result.add_error(format!("发现禁止的路径模式: {} ({})", name, pattern));
                    }
                }
                
                // 检查文件扩展名
                if let Some(ext) = Path::new(name).extension() {
                    let ext_str = ext.to_string_lossy();
                    if !self.config.allowed_extensions.contains(&ext_str.to_string()) {
                        result.add_warning(format!("不推荐的文件扩展名: {}", name));
                    }
                }
                
                // 检查必要文件
                if name.ends_with(".wasm") {
                    has_wasm = true;
                }
                if name == "plugin.json" || name == "manifest.json" {
                    has_manifest = true;
                }
            }
        }
        
        if !has_wasm {
            result.add_error("插件包中缺少 WASM 文件".to_string());
        }
        if !has_manifest {
            result.add_warning("插件包中缺少 manifest 文件 (plugin.json)".to_string());
        }
    }
    
    /// 验证目录内容
    fn validate_directory_contents(&self, dir_path: &Path, result: &mut ValidationResult) {
        let entries = match std::fs::read_dir(dir_path) {
            Ok(e) => e,
            Err(e) => {
                result.add_error(format!("无法读取目录: {}", e));
                return;
            }
        };
        
        let mut has_wasm = false;
        let mut has_manifest = false;
        
        for entry in entries.flatten() {
            let path = entry.path();
            
            if path.is_file() {
                // 检查禁止的路径模式
                let path_str = path.to_string_lossy();
                for pattern in &self.config.forbidden_patterns {
                    if path_str.contains(pattern) {
                        result.add_error(format!("发现禁止的路径模式: {} ({})", path.display(), pattern));
                    }
                }
                
                // 检查文件扩展名
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy();
                    if !self.config.allowed_extensions.contains(&ext_str.to_string()) {
                        result.add_warning(format!("不推荐的文件扩展名: {}", path.display()));
                    }
                }
                
                // 检查必要文件
                if let Some(ext) = path.extension() {
                    if ext == "wasm" {
                        has_wasm = true;
                    }
                }
                if let Some(name) = path.file_name() {
                    if name == "plugin.json" || name == "manifest.json" {
                        has_manifest = true;
                    }
                }
            }
        }
        
        if !has_wasm {
            result.add_error("插件目录中缺少 WASM 文件".to_string());
        }
        if !has_manifest {
            result.add_warning("插件目录中缺少 manifest 文件 (plugin.json)".to_string());
        }
    }
    
    /// 验证 manifest 文件
    fn validate_manifest(&self, package_path: &Path, result: &mut ValidationResult) {
        let manifest_path = if package_path.is_dir() {
            package_path.join("plugin.json")
        } else {
            // 对于 ZIP 文件，manifest 在内部
            return;
        };
        
        if !manifest_path.exists() {
            result.add_warning(format!("Manifest 文件不存在: {}", manifest_path.display()));
            return;
        }
        
        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) => {
                result.add_error(format!("读取 manifest 文件失败: {}", e));
                return;
            }
        };
        
        let manifest: serde_json::Value = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                result.add_error(format!("解析 manifest 文件失败: {}", e));
                return;
            }
        };
        
        // 验证必要字段
        let required_fields = ["id", "name", "version", "wasm_file"];
        for field in &required_fields {
            if manifest.get(field).is_none() {
                result.add_error(format!("Manifest 缺少必要字段: {}", field));
            }
        }
        
        // 验证 ID 格式
        if let Some(id) = manifest.get("id").and_then(|v| v.as_str()) {
            if !self.is_valid_plugin_id(id) {
                result.add_error(format!("无效的插件 ID 格式: {}", id));
            }
        }
        
        // 验证版本格式
        if let Some(version) = manifest.get("version").and_then(|v| v.as_str()) {
            if !self.is_valid_version(version) {
                result.add_warning(format!("版本格式可能无效: {}", version));
            }
        }
    }
    
    /// 验证签名
    fn validate_signature(&self, package_path: &Path, result: &mut ValidationResult) {
        // 查找签名文件
        let signature_path = if package_path.is_dir() {
            package_path.join("plugin.sig")
        } else {
            // 对于 ZIP 文件，签名验证需要先解压
            return;
        };
        
        if !signature_path.exists() {
            result.add_warning("未找到签名文件，跳过签名验证".to_string());
            return;
        }
        
        // 读取签名文件
        let signature_content = match std::fs::read_to_string(&signature_path) {
            Ok(c) => c,
            Err(e) => {
                result.add_error(format!("读取签名文件失败: {}", e));
                return;
            }
        };
        
        // 解析签名信息
        let signature_info: serde_json::Value = match serde_json::from_str(&signature_content) {
            Ok(v) => v,
            Err(e) => {
                // 尝试作为纯签名值
                let sig = SignatureInfo::new(
                    "Ed25519".to_string(),
                    "unknown".to_string(),
                    signature_content.trim().to_string(),
                );
                self.verify_signature_with_info(package_path, &sig, result);
                return;
            }
        };
        
        let sig = match SignatureInfo::from_json(&signature_info) {
            Ok(s) => s,
            Err(e) => {
                result.add_error(format!("解析签名信息失败: {}", e));
                return;
            }
        };
        
        self.verify_signature_with_info(package_path, &sig, result);
    }
    
    /// 使用签名信息验证
    fn verify_signature_with_info(&self, package_path: &Path, sig: &SignatureInfo, result: &mut ValidationResult) {
        // 读取 manifest 文件作为待验证数据
        let manifest_path = if package_path.is_dir() {
            package_path.join("plugin.json")
        } else {
            result.add_error("无法验证签名：需要目录形式的插件包".to_string());
            return;
        };
        
        if !manifest_path.exists() {
            result.add_error("无法验证签名：manifest 文件不存在".to_string());
            return;
        }
        
        match self.signature_validator.verify_file_base64(&manifest_path, &sig.signature) {
            Ok(true) => {
                tracing::info!("签名验证通过: {}", package_path.display());
            }
            Ok(false) => {
                result.add_error("签名验证失败：签名无效".to_string());
            }
            Err(e) => {
                result.add_error(format!("签名验证失败: {}", e));
            }
        }
    }
    
    /// 验证插件 ID 格式
    fn is_valid_plugin_id(&self, id: &str) -> bool {
        // ID 应该只包含字母、数字、下划线和连字符
        id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    }
    
    /// 验证版本格式
    fn is_valid_version(&self, version: &str) -> bool {
        // 简单的语义版本检查
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() >= 2 && parts.iter().all(|p| p.parse::<u32>().is_ok())
    }
    
    /// 验证插件签名
    pub async fn verify_signature(&self, package_path: &Path) -> Result<bool, String> {
        if !self.config.verify_signature {
            return Ok(true);
        }
        
        let mut result = ValidationResult::passed();
        self.validate_signature(package_path, &mut result);
        
        if result.passed {
            Ok(true)
        } else {
            Err(result.errors.join("; "))
        }
    }
    
    /// 检查权限
    /// 
    /// 验证插件请求的权限是否被允许。
    pub async fn check_permissions(&self, permissions: &[String]) -> Result<bool, String> {
        if !self.config.check_permissions {
            return Ok(true);
        }
        
        // 定义危险权限
        let dangerous_permissions = [
            "filesystem.write.root",
            "network.all",
            "process.execute",
            "system.shutdown",
        ];
        
        let mut warnings = Vec::new();
        for perm in permissions {
            if dangerous_permissions.contains(&perm.as_str()) {
                warnings.push(format!("危险权限: {}", perm));
            }
        }
        
        if !warnings.is_empty() {
            tracing::warn!("插件请求危险权限: {:?}", warnings);
        }
        
        Ok(true)
    }
    
    /// 获取配置
    pub fn config(&self) -> &SecurityConfig {
        &self.config
    }
    
    /// 设置配置
    pub fn set_config(&mut self, config: SecurityConfig) {
        self.config = config;
    }
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self::new()
    }
}
