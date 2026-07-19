//! 安全验证器模块
//!
//! 验证插件安全性，包括包结构、签名、权限等

use super::signature::{SignatureInfo, SignatureValidator};
use std::path::Path;
use tracing::error;

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
            // 【冻结待办 · 已知未生效】插件签名验证当前**默认关闭**。
            // 注意：本字段与 `config::settings::PluginSettings::verify_signatures`（默认 true）
            // 目前**互不联动**——运行时以此处 SecurityConfig 为准，即签名验证实际处于关闭状态。
            // 启用前置条件：① 建立插件签名/公钥分发流程；② 存量插件全部补签；③ 打通 settings → SecurityConfig 的配置注入。
            // 未满足前保持关闭，避免拒绝未签名的存量插件导致安装失败。
            verify_signature: false,
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
            if let Some(ext) = package_path.extension()
                && ext != "zip"
            {
                result.add_error(format!("不支持的插件包格式: {:?}", ext));
                return;
            }

            // 验证 ZIP 内容
            self.validate_zip_contents(package_path, result);
        } else if package_path.is_dir() {
            // 目录
            self.validate_directory_contents(package_path, result);
        }
    }

    /// 验证 ZIP 文件内容
    ///
    /// 递归检查 ZIP 包中的所有文件，支持多级目录结构。
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
        let mut _has_plugin_json = false;
        let mut has_manifest_json = false;

        for i in 0..archive.len() {
            if let Ok(file) = archive.by_index(i) {
                let name = file.name();

                // 跳过目录
                if name.ends_with('/') {
                    continue;
                }

                // 检查禁止的路径模式
                for pattern in &self.config.forbidden_patterns {
                    if name.contains(pattern) {
                        result.add_error(format!("发现禁止的路径模式: {} ({})", name, pattern));
                    }
                }

                // 检查文件扩展名
                if let Some(ext) = Path::new(name).extension() {
                    let ext_str = ext.to_string_lossy();
                    if !self
                        .config
                        .allowed_extensions
                        .contains(&ext_str.to_string())
                    {
                        result.add_warning(format!("不推荐的文件扩展名: {}", name));
                    }
                }

                // 获取文件名（不含路径）
                let file_name = Path::new(name)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // 递归检查必要文件（遍历所有子路径）
                if name.ends_with(".wasm") {
                    has_wasm = true;
                }
                if file_name == "plugin.json" {
                    _has_plugin_json = true;
                }
                if file_name == "manifest.json" {
                    has_manifest_json = true;
                }
            }
        }

        if !has_wasm {
            result.add_error(
                "插件包中缺少 WASM 文件（递归搜索所有子目录未找到 .wasm 文件）".to_string(),
            );
        }
        // if !_has_plugin_json {
        //     result.add_warning("插件包中缺少 plugin.json 文件".to_string());
        // }
        if !has_manifest_json {
            result.add_warning("插件包中缺少 manifest.json 文件".to_string());
        }
    }

    /// 验证目录内容
    ///
    /// 递归遍历目录及子目录，检查必要文件。
    fn validate_directory_contents(&self, dir_path: &Path, result: &mut ValidationResult) {
        let mut has_wasm = false;
        let mut has_plugin_json = false;
        let mut has_manifest_json = false;

        // 递归遍历目录
        if let Err(e) = self.validate_directory_recursive(
            dir_path,
            result,
            &mut has_wasm,
            &mut has_plugin_json,
            &mut has_manifest_json,
        ) {
            result.add_error(format!("遍历目录失败: {}", e));
            return;
        }

        if !has_wasm {
            result.add_error(
                "插件目录中缺少 WASM 文件（递归搜索所有子目录未找到 .wasm 文件）".to_string(),
            );
        }
        // if !has_plugin_json {
        //     result.add_warning("插件目录中缺少 plugin.json 文件".to_string());
        // }
        if !has_manifest_json {
            result.add_warning("插件目录中缺少 manifest.json 文件".to_string());
        }
    }

    /// 递归遍历目录
    ///
    /// 深度优先遍历目录及子目录，检查文件。
    fn validate_directory_recursive(
        &self,
        dir_path: &Path,
        result: &mut ValidationResult,
        has_wasm: &mut bool,
        has_plugin_json: &mut bool,
        has_manifest_json: &mut bool,
    ) -> Result<(), std::io::Error> {
        let entries = std::fs::read_dir(dir_path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // 递归处理子目录
                self.validate_directory_recursive(
                    &path,
                    result,
                    has_wasm,
                    has_plugin_json,
                    has_manifest_json,
                )?;
            } else if path.is_file() {
                // 检查禁止的路径模式
                let path_str = path.to_string_lossy();
                for pattern in &self.config.forbidden_patterns {
                    if path_str.contains(pattern) {
                        result.add_error(format!(
                            "发现禁止的路径模式: {} ({})",
                            path.display(),
                            pattern
                        ));
                    }
                }

                // 检查文件扩展名
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy();
                    if !self
                        .config
                        .allowed_extensions
                        .contains(&ext_str.to_string())
                    {
                        result.add_warning(format!("不推荐的文件扩展名: {}", path.display()));
                    }

                    // 检查 WASM 文件
                    if ext == "wasm" {
                        *has_wasm = true;
                    }
                }

                // 检查必要文件
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name == "plugin.json" {
                        *has_plugin_json = true;
                    }
                    if file_name == "manifest.json" {
                        *has_manifest_json = true;
                    }
                }
            }
        }

        Ok(())
    }

    /// 验证 manifest 文件
    ///
    /// manifest.json 结构示例：
    /// ```json
    /// {
    ///   "manifest_version": "1.0",
    ///   "plugin": {
    ///     "id": "example_plugin",
    ///     "name": "示例插件",
    ///     "version": "1.0.0",
    ///     "description": "...",
    ///     "main_file": "bin/plugin.wasm",
    ///     "dependencies": ["plugin1", "plugin2"]
    ///   }
    /// }
    /// ```
    fn validate_manifest(&self, package_path: &Path, result: &mut ValidationResult) {
        let manifest_path = if package_path.is_dir() {
            package_path.join("manifest.json")
        } else {
            // todo  zip 验证 manifest.json 后续在实现
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

        // 验证 manifest_version
        if let Some(manifest_version) = manifest.get("manifest_version").and_then(|v| v.as_str()) {
            if manifest_version != "1.0" {
                result.add_warning(format!("Manifest 版本可能不兼容: {}", manifest_version));
            }
        } else {
            result.add_warning("Manifest 缺少 manifest_version 字段".to_string());
        }

        // 获取 plugin 对象
        let plugin = match manifest.get("plugin") {
            Some(p) => p,
            None => {
                result.add_error("Manifest 缺少 plugin 对象".to_string());
                return;
            }
        };

        // 验证 plugin 对象中的必要字段
        let required_fields = ["id", "name", "version", "main_file"];
        for field in &required_fields {
            if plugin.get(field).is_none() {
                result.add_error(format!("Manifest.plugin 缺少必要字段: {}", field));
            }
        }

        // 验证 ID 格式
        if let Some(id) = plugin.get("id").and_then(|v| v.as_str())
            && !self.is_valid_plugin_id(id)
        {
            result.add_error(format!("无效的插件 ID 格式: {}", id));
        }

        // 验证版本格式
        if let Some(version) = plugin.get("version").and_then(|v| v.as_str())
            && !self.is_valid_version(version)
        {
            result.add_warning(format!("版本格式可能无效: {}", version));
        }

        // 验证依赖格式（可选）
        if let Some(deps) = plugin.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps {
                if let Some(dep_id) = dep.as_str() {
                    if !self.is_valid_plugin_id(dep_id) {
                        result.add_warning(format!("依赖插件 ID 格式可能无效: {}", dep_id));
                    }
                } else {
                    result.add_warning("依赖列表中存在非字符串项".to_string());
                }
            }
        }
    }

    /// 验证签名 TODO 实现签名验证逻辑 支持zip验证 ，之后在实现0320
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
                error!("解析签名文件失败: {}", e);
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
    fn verify_signature_with_info(
        &self,
        package_path: &Path,
        sig: &SignatureInfo,
        result: &mut ValidationResult,
    ) {
        // 读取 manifest 文件作为待验证数据
        let manifest_path = if package_path.is_dir() {
            package_path.join("manifest.json")
        } else {
            result.add_error("无法验证签名：需要目录形式的插件包".to_string());
            return;
        };

        if !manifest_path.exists() {
            result.add_error("无法验证签名：manifest 文件不存在".to_string());
            return;
        }

        match self
            .signature_validator
            .verify_file_base64(&manifest_path, &sig.signature)
        {
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
        // ID 非空，且只包含字母、数字、下划线、连字符和点号
        // （`chars().all()` 对空串返回 true，需显式排除空 ID）
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Cursor, Write};
    use std::path::PathBuf;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    /// 临时目录守卫，Drop 时自动清理
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "cmx_plugin_validator_{}_{}",
                label,
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.0.join(name), content).unwrap();
        }

        fn write_bytes(&self, name: &str, content: &[u8]) {
            fs::write(self.0.join(name), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 有效的 manifest.json 内容（符合 validator 的松散校验规则）
    fn valid_manifest() -> String {
        r#"{
            "manifest_version": "1.0",
            "plugin": {
                "id": "my_plugin",
                "name": "My Plugin",
                "version": "1.0.0",
                "main_file": "bin/plugin.wasm"
            }
        }"#
        .to_string()
    }

    /// 构造一个 ZIP 文件到指定路径
    fn write_zip(zip_path: &Path, entries: &[(&str, &[u8])]) {
        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(buf);
        let options = SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        let buf = zip.finish().unwrap().into_inner();
        fs::write(zip_path, &buf).unwrap();
    }

    // ==================== ValidationResult 纯逻辑 ====================

    #[test]
    fn test_validation_result_passed() {
        let r = ValidationResult::passed();
        assert!(r.passed);
        assert!(r.errors.is_empty());
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn test_validation_result_failed() {
        let r = ValidationResult::failed(vec!["err1".to_string(), "err2".to_string()]);
        assert!(!r.passed);
        assert_eq!(r.errors.len(), 2);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn test_validation_result_add_error_sets_passed_false() {
        let mut r = ValidationResult::passed();
        r.add_error("boom".to_string());
        assert!(!r.passed);
        assert_eq!(r.errors.len(), 1);
    }

    #[test]
    fn test_validation_result_add_warning_keeps_passed() {
        let mut r = ValidationResult::passed();
        r.add_warning("warn".to_string());
        assert!(r.passed, "仅添加警告不应使验证失败");
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn test_validation_result_merge() {
        let mut a = ValidationResult::passed();
        a.add_warning("a-warn".to_string());
        let mut b = ValidationResult::passed();
        b.add_error("b-err".to_string());
        b.add_warning("b-warn".to_string());
        a.merge(b);
        assert!(!a.passed, "合并失败结果后应失败");
        assert_eq!(a.errors.len(), 1);
        assert_eq!(a.warnings.len(), 2);
    }

    // ==================== SecurityConfig 默认值 ====================

    #[test]
    fn test_security_config_defaults() {
        let cfg = SecurityConfig::default();
        // 签名验证默认关闭（插件开发端暂未支持）
        assert!(!cfg.verify_signature);
        assert!(cfg.check_permissions);
        assert!(cfg.check_package_structure);
        assert!(cfg.check_manifest);
        assert_eq!(cfg.max_plugin_size, 100 * 1024 * 1024);
        // 扩展名白名单包含 wasm/json/toml/yaml
        assert!(cfg.allowed_extensions.contains(&"wasm".to_string()));
        assert!(cfg.allowed_extensions.contains(&"json".to_string()));
        assert!(cfg.allowed_extensions.contains(&"toml".to_string()));
        assert!(cfg.allowed_extensions.contains(&"yaml".to_string()));
        // 禁止路径穿越模式
        assert!(cfg.forbidden_patterns.contains(&"../".to_string()));
        assert!(cfg.forbidden_patterns.contains(&"..\\".to_string()));
        assert!(cfg.forbidden_patterns.contains(&"/etc/".to_string()));
        assert!(cfg.forbidden_patterns.contains(&"/root/".to_string()));
    }

    #[test]
    fn test_security_validator_with_custom_config() {
        let cfg = SecurityConfig {
            verify_signature: false,
            check_permissions: false,
            check_package_structure: false,
            check_manifest: false,
            max_plugin_size: 10,
            allowed_extensions: vec!["wasm".to_string()],
            forbidden_patterns: vec!["../".to_string()],
        };
        let v = SecurityValidator::with_config(cfg);
        assert!(!v.config().check_package_structure);
        assert_eq!(v.config().max_plugin_size, 10);
    }

    #[test]
    fn test_security_validator_set_config() {
        let mut v = SecurityValidator::new();
        let cfg = SecurityConfig {
            verify_signature: false,
            check_permissions: false,
            check_package_structure: true,
            check_manifest: false,
            max_plugin_size: 5,
            allowed_extensions: vec![],
            forbidden_patterns: vec![],
        };
        v.set_config(cfg);
        assert!(v.config().check_package_structure);
        assert!(!v.config().check_manifest);
        assert_eq!(v.config().max_plugin_size, 5);
    }

    // ==================== 私有纯逻辑：插件 ID 校验 ====================

    #[test]
    fn test_is_valid_plugin_id_valid() {
        let v = SecurityValidator::new();
        assert!(v.is_valid_plugin_id("my_plugin"));
        assert!(v.is_valid_plugin_id("my-plugin"));
        assert!(v.is_valid_plugin_id("plugin123"));
        assert!(v.is_valid_plugin_id("org.plugin"));
        assert!(v.is_valid_plugin_id("ABC_123-xyz.v2"));
    }

    #[test]
    fn test_is_valid_plugin_id_invalid() {
        let v = SecurityValidator::new();
        // 含空格、特殊字符均非法
        assert!(!v.is_valid_plugin_id("my plugin"));
        assert!(!v.is_valid_plugin_id("my@plugin"));
        assert!(!v.is_valid_plugin_id("my/plugin"));
        assert!(!v.is_valid_plugin_id("my:plugin"));
        assert!(!v.is_valid_plugin_id(""));
    }

    // ==================== 私有纯逻辑：版本格式校验 ====================

    #[test]
    fn test_is_valid_version_valid() {
        let v = SecurityValidator::new();
        assert!(v.is_valid_version("1.0"));
        assert!(v.is_valid_version("1.0.0"));
        assert!(v.is_valid_version("12.3.45"));
    }

    #[test]
    fn test_is_valid_version_invalid() {
        let v = SecurityValidator::new();
        // 仅一段或非数字应无效
        assert!(!v.is_valid_version("1"));
        assert!(!v.is_valid_version("1.x"));
        assert!(!v.is_valid_version("a.b"));
        assert!(!v.is_valid_version(""));
    }

    // ==================== 文件系统：目录包结构校验 ====================

    #[tokio::test]
    async fn test_validate_valid_plugin_directory() {
        let dir = TempDir::new("valid");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.passed,
            "有效插件目录应通过校验，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_nonexistent_package() {
        let v = SecurityValidator::new();
        let missing = std::env::temp_dir().join("cmx_plugin_nonexistent_9999");
        let result = v.validate_plugin_package(&missing).await;
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("插件包不存在")));
    }

    #[tokio::test]
    async fn test_validate_missing_wasm_errors() {
        let dir = TempDir::new("no_wasm");
        dir.write("manifest.json", &valid_manifest());

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed, "缺少 WASM 文件应校验失败");
        assert!(
            result.errors.iter().any(|e| e.contains("WASM")),
            "应报告缺少 WASM 文件错误，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_missing_manifest_warns() {
        let dir = TempDir::new("no_manifest");
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        // 缺少 manifest 仅产生警告，不改变通过状态
        assert!(result.passed, "缺少 manifest 不应导致校验失败");
        assert!(
            !result.warnings.is_empty(),
            "缺少 manifest 应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_non_allowed_extension_warns() {
        let dir = TempDir::new("bad_ext");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        // .exe 不在扩展名白名单
        dir.write_bytes("helper.exe", b"binary");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(result.passed, "非白名单扩展名仅产生警告");
        assert!(
            result.warnings.iter().any(|w| w.contains("helper.exe")),
            "应针对非白名单扩展名产生警告: {:?}",
            result.warnings
        );
    }

    // ==================== 文件系统：manifest 校验 ====================

    #[tokio::test]
    async fn test_validate_manifest_missing_plugin_object_errors() {
        let dir = TempDir::new("no_plugin_obj");
        dir.write("manifest.json", r#"{"manifest_version": "1.0"}"#);
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed);
        assert!(
            result.errors.iter().any(|e| e.contains("plugin 对象")),
            "应报告缺少 plugin 对象，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_missing_required_fields_errors() {
        let dir = TempDir::new("missing_fields");
        // 缺少 version 与 main_file
        dir.write(
            "manifest.json",
            r#"{"manifest_version":"1.0","plugin":{"id":"my_plugin","name":"x"}}"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed);
        assert!(
            result.errors.iter().any(|e| e.contains("version")),
            "缺少 version 字段应报错"
        );
        assert!(
            result.errors.iter().any(|e| e.contains("main_file")),
            "缺少 main_file 字段应报错"
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_invalid_plugin_id_errors() {
        let dir = TempDir::new("bad_id");
        // id 含非法字符（空格）
        dir.write(
            "manifest.json",
            r#"{"manifest_version":"1.0","plugin":{"id":"bad id","name":"x","version":"1.0.0","main_file":"p.wasm"}}"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed);
        assert!(
            result.errors.iter().any(|e| e.contains("插件 ID 格式")),
            "非法插件 ID 应报错，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_invalid_version_warns() {
        let dir = TempDir::new("bad_version");
        // 版本非数字格式（仅产生警告）
        dir.write(
            "manifest.json",
            r#"{"manifest_version":"1.0","plugin":{"id":"my_plugin","name":"x","version":"x.y","main_file":"p.wasm"}}"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        // 版本格式无效仅产生警告，不改变通过状态
        assert!(
            result.warnings.iter().any(|w| w.contains("版本格式")),
            "无效版本应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_invalid_json_errors() {
        let dir = TempDir::new("bad_json");
        dir.write("manifest.json", "{ not a valid json");
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("manifest 文件失败")),
            "无效 JSON 应报告 manifest 解析失败，错误: {:?}",
            result.errors
        );
    }

    // ==================== 文件系统：包大小校验 ====================

    #[tokio::test]
    async fn test_validate_oversized_package_errors() {
        let dir = TempDir::new("oversize");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");

        // 配置极小的最大尺寸阈值
        let cfg = SecurityConfig {
            max_plugin_size: 1,
            ..SecurityConfig::default()
        };
        let v = SecurityValidator::with_config(cfg);
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(!result.passed);
        assert!(
            result.errors.iter().any(|e| e.contains("超出限制")),
            "超尺寸包应报错，错误: {:?}",
            result.errors
        );
    }

    // ==================== ZIP：路径穿越防护（安全关键路径） ====================

    #[tokio::test]
    async fn test_validate_zip_path_traversal_detected() {
        let dir = TempDir::new("zip_traversal");
        let zip_path = dir.path().join("plugin.zip");
        // 构造恶意 ZIP：含 ../ 路径穿越条目，同时包含 wasm 与 manifest 以隔离路径穿越错误
        write_zip(
            &zip_path,
            &[
                ("../../etc/passwd", b"malicious"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.errors.iter().any(|e| e.contains("禁止的路径模式")),
            "应检测到路径穿越条目，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_windows_path_traversal_detected() {
        let dir = TempDir::new("zip_win_traversal");
        let zip_path = dir.path().join("plugin.zip");
        // Windows 风格路径穿越模式 ..\
        write_zip(
            &zip_path,
            &[
                ("..\\..\\windows\\sys.dll", b"malicious"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.errors.iter().any(|e| e.contains("禁止的路径模式")),
            "应检测到 Windows 路径穿越条目，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_missing_wasm_errors() {
        let dir = TempDir::new("zip_no_wasm");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(&zip_path, &[("manifest.json", b"{}"), ("readme.md", b"hi")]);

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(!result.passed);
        assert!(result.errors.iter().any(|e| e.contains("WASM")));
    }

    #[tokio::test]
    async fn test_validate_zip_valid_structure_passes() {
        let dir = TempDir::new("zip_valid");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(
            &zip_path,
            &[
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
                ("readme.md", b"docs"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.passed,
            "结构合法的 ZIP 应通过校验，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_unsupported_package_format_errors() {
        let dir = TempDir::new("bad_format");
        let tar_path = dir.path().join("plugin.tar");
        // 非 zip 扩展名的文件应报错
        fs::write(&tar_path, b"some bytes").unwrap();

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&tar_path).await;
        assert!(!result.passed);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("不支持的插件包格式")),
            "应报告不支持的包格式，错误: {:?}",
            result.errors
        );
    }

    // ==================== 权限校验 ====================

    #[tokio::test]
    async fn test_check_permissions_dangerous_returns_ok() {
        // 危险权限仅记录警告，仍返回 Ok(true)
        let v = SecurityValidator::new();
        let result = v
            .check_permissions(&[
                "filesystem.write.root".to_string(),
                "network.all".to_string(),
            ])
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_check_permissions_disabled_config() {
        let cfg = SecurityConfig {
            check_permissions: false,
            ..SecurityConfig::default()
        };
        let v = SecurityValidator::with_config(cfg);
        let result = v
            .check_permissions(&["filesystem.write.root".to_string()])
            .await;
        assert!(result.unwrap(), "关闭权限检查时应直接通过");
    }

    #[tokio::test]
    async fn test_verify_signature_skipped_when_disabled() {
        // 默认配置关闭签名验证，应直接返回 Ok(true)
        let dir = TempDir::new("sig_skip");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.verify_signature(dir.path()).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    // ==================== 目录：禁止路径模式检测（安全关键路径） ====================

    #[tokio::test]
    async fn test_validate_directory_etc_path_pattern_detected() {
        // 目录形式的 /etc/ 路径模式检测：在插件目录下创建 etc/passwd 文件，
        // 其完整路径会包含 "/etc/" 模式，应被检测到
        let dir = TempDir::new("dir_etc");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        // 创建 etc 子目录并放置一个文件，使路径包含 "/etc/"
        let etc_dir = dir.path().join("etc");
        std::fs::create_dir_all(&etc_dir).unwrap();
        std::fs::write(etc_dir.join("passwd"), b"malicious").unwrap();

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("禁止的路径模式") && e.contains("/etc/")),
            "应检测到目录中的 /etc/ 路径模式，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_directory_root_path_pattern_detected() {
        // 目录形式的 /root/ 路径模式检测：在插件目录下创建 root/.ssh 文件，
        // 其完整路径会包含 "/root/" 模式，应被检测到
        let dir = TempDir::new("dir_root");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        let root_dir = dir.path().join("root").join(".ssh");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::write(root_dir.join("id_rsa"), b"stolen").unwrap();

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("禁止的路径模式") && e.contains("/root/")),
            "应检测到目录中的 /root/ 路径模式，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_etc_path_pattern_detected() {
        // ZIP 中包含 /etc/ 路径模式应被检测
        let dir = TempDir::new("zip_etc");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(
            &zip_path,
            &[
                ("/etc/passwd", b"malicious"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.errors.iter().any(|e| e.contains("/etc/")),
            "应检测到 /etc/ 路径模式，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_root_path_pattern_detected() {
        // ZIP 中包含 /root/ 路径模式应被检测
        let dir = TempDir::new("zip_root");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(
            &zip_path,
            &[
                ("/root/.ssh/id_rsa", b"stolen"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.errors.iter().any(|e| e.contains("/root/")),
            "应检测到 /root/ 路径模式，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_windows_path_pattern_detected() {
        // ZIP 中包含 C:\Windows\ 路径模式应被检测
        let dir = TempDir::new("zip_win");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(
            &zip_path,
            &[
                ("C:\\Windows\\System32\\evil.dll", b"malicious"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        assert!(
            result.errors.iter().any(|e| e.contains("C:\\Windows\\")),
            "应检测到 Windows 系统路径模式，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_zip_multiple_forbidden_patterns_all_detected() {
        // 同时包含多种禁止路径模式，应全部检测
        let dir = TempDir::new("zip_multi");
        let zip_path = dir.path().join("plugin.zip");
        write_zip(
            &zip_path,
            &[
                ("../escape.txt", b"x"),
                ("/etc/shadow", b"x"),
                ("C:\\Windows\\bad.dll", b"x"),
                ("plugin.wasm", b"\0asm"),
                ("manifest.json", b"{}"),
            ],
        );

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(&zip_path).await;
        // 至少应报告 3 种禁止路径模式
        let forbidden_count = result
            .errors
            .iter()
            .filter(|e| e.contains("禁止的路径模式"))
            .count();
        assert!(
            forbidden_count >= 3,
            "应至少检测到 3 处禁止路径模式，实际: {}, 错误: {:?}",
            forbidden_count,
            result.errors
        );
    }

    // ==================== 扩展名白名单正向用例 ====================

    #[tokio::test]
    async fn test_validate_allowed_extension_json_no_warning() {
        // .json 在白名单中，不应产生扩展名警告
        let dir = TempDir::new("ext_json");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        dir.write("config.json", r#"{"key":"value"}"#);

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.warnings.iter().all(|w| !w.contains("config.json")),
            "白名单中的 .json 不应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_allowed_extension_toml_no_warning() {
        // .toml 在白名单中，不应产生扩展名警告
        let dir = TempDir::new("ext_toml");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        dir.write("config.toml", "[package]\nname = \"x\"");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.warnings.iter().all(|w| !w.contains("config.toml")),
            "白名单中的 .toml 不应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_allowed_extension_yaml_no_warning() {
        // .yaml / .yml 在白名单中，不应产生扩展名警告
        let dir = TempDir::new("ext_yaml");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        dir.write("config.yaml", "key: value");
        dir.write("config2.yml", "key: value");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .warnings
                .iter()
                .all(|w| !w.contains("config.yaml") && !w.contains("config2.yml")),
            "白名单中的 .yaml/.yml 不应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_allowed_extension_md_txt_no_warning() {
        // .md / .txt 在白名单中，不应产生扩展名警告
        let dir = TempDir::new("ext_md_txt");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        dir.write("readme.md", "# readme");
        dir.write("notes.txt", "some notes");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .warnings
                .iter()
                .all(|w| !w.contains("readme.md") && !w.contains("notes.txt")),
            "白名单中的 .md/.txt 不应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_custom_allowed_extensions_respected() {
        // 自定义白名单：允许 .rs，禁止 .json
        let dir = TempDir::new("ext_custom");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");
        dir.write("helper.rs", "fn main() {}");
        dir.write("data.json", "{}");

        let cfg = SecurityConfig {
            allowed_extensions: vec![
                "wasm".to_string(),
                "json".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "yml".to_string(),
                "md".to_string(),
                "txt".to_string(),
                "rs".to_string(),
            ],
            ..SecurityConfig::default()
        };
        let v = SecurityValidator::with_config(cfg);
        let result = v.validate_plugin_package(dir.path()).await;
        // .rs 在自定义白名单中，不应产生警告
        assert!(
            result.warnings.iter().all(|w| !w.contains("helper.rs")),
            "自定义白名单中的 .rs 不应产生警告: {:?}",
            result.warnings
        );
    }

    // ==================== manifest_version 兼容性 ====================

    #[tokio::test]
    async fn test_validate_manifest_version_compatible_no_warning() {
        // manifest_version 为 "1.0" 不应产生版本不兼容警告
        let dir = TempDir::new("manifest_compat");
        dir.write("manifest.json", &valid_manifest());
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(result.passed);
        assert!(
            result
                .warnings
                .iter()
                .all(|w| !w.contains("manifest_version") && !w.contains("Manifest 版本")),
            "manifest_version 1.0 不应产生兼容性警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_version_incompatible_warns() {
        // manifest_version 非 1.0 应产生兼容性警告
        let dir = TempDir::new("manifest_incompat");
        dir.write(
            "manifest.json",
            r#"{
                "manifest_version": "2.0",
                "plugin": {
                    "id": "my_plugin",
                    "name": "x",
                    "version": "1.0.0",
                    "main_file": "p.wasm"
                }
            }"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Manifest 版本可能不兼容")),
            "非 1.0 manifest_version 应产生兼容性警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_missing_version_field_warns() {
        // 缺少 manifest_version 字段应产生警告
        let dir = TempDir::new("manifest_no_ver");
        dir.write(
            "manifest.json",
            r#"{
                "plugin": {
                    "id": "my_plugin",
                    "name": "x",
                    "version": "1.0.0",
                    "main_file": "p.wasm"
                }
            }"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("manifest_version 字段")),
            "缺少 manifest_version 应产生警告: {:?}",
            result.warnings
        );
    }

    // ==================== manifest 依赖校验 ====================

    #[tokio::test]
    async fn test_validate_manifest_dependencies_valid_no_warning() {
        // 依赖列表中所有 ID 合法，不应产生警告
        let dir = TempDir::new("deps_valid");
        dir.write(
            "manifest.json",
            r#"{
                "manifest_version": "1.0",
                "plugin": {
                    "id": "my_plugin",
                    "name": "x",
                    "version": "1.0.0",
                    "main_file": "p.wasm",
                    "dependencies": ["dep_a", "dep_b-1", "org.dep"]
                }
            }"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(result.passed);
        assert!(
            result.warnings.iter().all(|w| !w.contains("依赖插件 ID")),
            "合法依赖 ID 不应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_dependencies_invalid_id_warns() {
        // 依赖列表中存在非法 ID 应产生警告
        let dir = TempDir::new("deps_invalid");
        dir.write(
            "manifest.json",
            r#"{
                "manifest_version": "1.0",
                "plugin": {
                    "id": "my_plugin",
                    "name": "x",
                    "version": "1.0.0",
                    "main_file": "p.wasm",
                    "dependencies": ["good_dep", "bad dep@id"]
                }
            }"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("依赖插件 ID 格式")),
            "非法依赖 ID 应产生警告: {:?}",
            result.warnings
        );
    }

    #[tokio::test]
    async fn test_validate_manifest_dependencies_non_string_item_warns() {
        // 依赖列表中存在非字符串项应产生警告
        let dir = TempDir::new("deps_non_str");
        dir.write(
            "manifest.json",
            r#"{
                "manifest_version": "1.0",
                "plugin": {
                    "id": "my_plugin",
                    "name": "x",
                    "version": "1.0.0",
                    "main_file": "p.wasm",
                    "dependencies": ["good_dep", 123]
                }
            }"#,
        );
        dir.write_bytes("plugin.wasm", b"\0asm");

        let v = SecurityValidator::new();
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.warnings.iter().any(|w| w.contains("非字符串项")),
            "非字符串依赖项应产生警告: {:?}",
            result.warnings
        );
    }

    // ==================== 关闭子检查项的行为 ====================

    #[tokio::test]
    async fn test_validate_with_structure_check_disabled_skips_wasm_check() {
        // 关闭包结构检查时，缺少 WASM 不应导致校验失败
        let dir = TempDir::new("struct_off");
        dir.write("manifest.json", &valid_manifest());
        // 不放置 plugin.wasm

        let cfg = SecurityConfig {
            check_package_structure: false,
            ..SecurityConfig::default()
        };
        let v = SecurityValidator::with_config(cfg);
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.passed,
            "关闭包结构检查时缺少 WASM 不应失败，错误: {:?}",
            result.errors
        );
    }

    #[tokio::test]
    async fn test_validate_with_manifest_check_disabled_skips_manifest_errors() {
        // 关闭 manifest 检查时，损坏的 manifest 不应导致校验失败
        let dir = TempDir::new("manifest_off");
        dir.write("manifest.json", "{ invalid json");
        dir.write_bytes("plugin.wasm", b"\0asm");

        let cfg = SecurityConfig {
            check_manifest: false,
            ..SecurityConfig::default()
        };
        let v = SecurityValidator::with_config(cfg);
        let result = v.validate_plugin_package(dir.path()).await;
        assert!(
            result.passed,
            "关闭 manifest 检查时损坏 manifest 不应失败，错误: {:?}",
            result.errors
        );
    }
}
