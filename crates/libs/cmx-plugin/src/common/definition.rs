//! 插件定义解析工具模块
//!
//! 提供插件 manifest.json 解析等通用操作。
//!
//! # 功能概述
//!
//! - 解析插件定义文件（manifest.json）
//! - 验证插件定义的完整性
//! - 从安装路径读取插件定义
//!
//! # manifest.json 结构
//!
//! ```json
//! {
//!   "manifest_version": "1.0",
//!   "plugin": {
//!     "id": "example_plugin",
//!     "name": "示例插件",
//!     "version": "1.0.0",
//!     "main_file": "bin/plugin.wasm",
//!     "description": "插件描述",
//!     "vendor_name": "供应商名称",
//!     "dependencies": [],
//!     "services": []
//!   }
//! }
//! ```

use std::path::Path;
use cmx_core::model::meta::plugin::PluginManifest;
use crate::error::{PluginError, PluginResult};
use zip::ZipArchive;

/// 插件定义工具
///
/// 提供插件定义解析和验证的静态方法集合。
///
/// # 示例
///
/// ```rust,no_run
/// use std::path::Path;
/// use cmx_plugin::common::DefinitionUtils;
///
/// let plugin_path = Path::new("./plugins/my-plugin");
/// let definition = DefinitionUtils::parse_plugin_definition(plugin_path)?;
///
/// // 验证定义完整性
/// DefinitionUtils::validate_definition(&definition)?;
/// # Ok::<(), cmx_plugin::error::PluginError>(())
/// ```
pub struct DefinitionUtils;

impl DefinitionUtils {
    /// 解析插件定义
    ///
    /// 从指定目录的 manifest.json 文件中解析插件定义。
    ///
    /// # 参数
    ///
    /// * `package_path` - 包含 manifest.json 的目录路径
    ///
    /// # 返回值
    ///
    /// 返回解析后的 `PluginDefinition` 对象，包含插件的完整元数据。
    ///
    /// # 错误
    ///
    /// - `PluginError::Metadata`: 当 manifest.json 文件不存在时
    /// - `PluginError::Metadata`: 当文件读取失败时
    /// - `PluginError::Metadata`: 当 JSON 解析失败时
    /// - `PluginError::Metadata`: 当缺少必要的 plugin 对象时
    ///
    /// # manifest.json 结构
    ///
    /// ```json
    /// {
    ///   "manifest_version": "1.0",
    ///   "plugin": {
    ///     "id": "example_plugin",
    ///     "name": "示例插件",
    ///     "version": "1.0.0",
    ///     "main_file": "bin/plugin.wasm",
    ///     ...
    ///   }
    /// }
    /// ```
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use cmx_plugin::common::DefinitionUtils;
    ///
    /// let plugin_path = Path::new("./plugins/my-plugin");
    /// let definition = DefinitionUtils::parse_plugin_definition(plugin_path)?;
    /// println!("插件ID: {}, 版本: {:?}", definition.id, definition.version);
    /// # Ok::<(), cmx_plugin::error::PluginError>(())
    /// ```
    pub fn parse_plugin_definition(
        package_path: &Path,
    ) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let manifest_path = package_path.join("manifest.json");

        if !manifest_path.exists() {
            return Err(PluginError::Metadata(
                "插件定义文件 manifest.json 不存在".to_string(),
            ));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Metadata(format!("读取插件定义文件失败: {}", e)))?;

        let manifest_json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析 manifest.json 失败: {}", e)))?;

        let plugin_value = manifest_json.get("plugin").ok_or_else(|| {
            PluginError::Metadata("manifest.json 缺少 plugin 对象".to_string())
        })?;

        let definition: cmx_core::model::meta::plugin::PluginDefinition =
            serde_json::from_value(plugin_value.clone())
                .map_err(|e| PluginError::Metadata(format!("解析 plugin 定义失败: {}", e)))?;

        let _manifest: PluginManifest = serde_json::from_value(manifest_json)
            .map_err(|e| PluginError::Metadata(format!("解析 manifest 定义失败: {}", e)))?;
        Ok(definition)
    }

    /// 解析插件定义（异步版本）
    ///
    /// 异步版本的 `parse_plugin_definition`，适用于异步上下文。
    ///
    /// # 参数
    ///
    /// * `package_path` - 包含 manifest.json 的目录路径
    ///
    /// # 返回值
    ///
    /// 返回解析后的 `PluginDefinition` 对象。
    ///
    /// # 说明
    ///
    /// 当前实现内部调用同步版本，未来可能会改为真正的异步文件读取。
    pub async fn parse_plugin_definition_async(
        package_path: &Path,
    ) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        Self::parse_plugin_definition(package_path)
    }

    /// 从安装路径读取并解析插件定义
    ///
    /// 用于已安装插件的定义读取，与 `parse_plugin_definition` 功能相同。
    ///
    /// # 参数
    ///
    /// * `install_path` - 插件的安装目录路径
    ///
    /// # 返回值
    ///
    /// 返回解析后的 `PluginDefinition` 对象。
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use cmx_plugin::common::DefinitionUtils;
    ///
    /// let install_path = Path::new("./plugins/my-plugin");
    /// let definition = DefinitionUtils::parse_from_install_path(install_path)?;
    /// # Ok::<(), cmx_plugin::error::PluginError>(())
    /// ```
    pub fn parse_from_install_path(
        install_path: &Path,
    ) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        Self::parse_plugin_definition(install_path)
    }

    /// 验证插件定义的完整性
    ///
    /// 检查插件定义中的必要字段是否存在且有效。
    ///
    /// # 参数
    ///
    /// * `definition` - 要验证的插件定义对象
    ///
    /// # 返回值
    ///
    /// 验证通过返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - `PluginError::Metadata`: 当插件 ID 为空时
    /// - `PluginError::Metadata`: 当插件名称为空时
    /// - `PluginError::Metadata`: 当插件主文件路径为空时
    ///
    /// # 验证规则
    ///
    /// 1. `id` 不能为空字符串
    /// 2. `name` 不能为空字符串
    /// 3. `main_file` 不能为空字符串
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use cmx_plugin::common::DefinitionUtils;
    /// # use cmx_core::model::meta::plugin::PluginDefinition;
    ///
    /// # fn example(definition: &PluginDefinition) -> Result<(), cmx_plugin::error::PluginError> {
    /// DefinitionUtils::validate_definition(definition)?;
    /// println!("插件定义验证通过");
    /// # Ok(())
    /// # }
    /// ```
    pub fn validate_definition(
        definition: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<()> {
        if definition.id.is_empty() {
            return Err(PluginError::Metadata("插件 ID 不能为空".to_string()));
        }

        if definition.name.is_empty() {
            return Err(PluginError::Metadata("插件名称不能为空".to_string()));
        }

        if definition.main_file.is_empty() {
            return Err(PluginError::Metadata("插件主文件路径不能为空".to_string()));
        }

        Ok(())
    }

    /// 从 ZIP 包直接解析插件定义。
    ///
    /// 将 ZIP 包解压到临时目录，解析 manifest.json，然后清理临时目录。
    /// 适用于在调用 deploy 之前需要提前获取 plugin_id 和 version 的场景。
    ///
    /// # 参数
    ///
    /// * `zip_path` - ZIP 包文件的路径
    ///
    /// # 返回值
    ///
    /// 返回解析后的 `PluginDefinition` 对象。
    ///
    /// # 错误
    ///
    /// - `PluginError::Io` - ZIP 解压失败或 manifest.json 读取失败
    /// - `PluginError::Metadata` - manifest.json 解析失败
    pub fn parse_from_zip(zip_path: &Path) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let temp_dir = std::env::temp_dir().join(format!("cmx_plugin_parse_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir)?;

        // 解压 ZIP
        let zip_file = std::fs::File::open(zip_path)?;
        let mut zip_archive = ZipArchive::new(std::io::BufReader::new(zip_file))
            .map_err(|e| PluginError::Zip(e.to_string()))?;

        zip_archive.extract(&temp_dir)
            .map_err(|e| PluginError::Zip(e.to_string()))?;

        // 解析插件定义
        let result = Self::parse_plugin_definition(&temp_dir);

        // 清理临时目录
        let _ = std::fs::remove_dir_all(&temp_dir);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_core::model::meta::plugin::PluginDefinition;
    use std::fs;
    use std::path::PathBuf;

    /// 临时目录守卫，Drop 时自动清理
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("cmx_plugin_def_{}_{}", label, uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, name: &str, content: &str) {
            fs::write(self.0.join(name), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 构造一个有效的插件定义
    fn make_valid_definition() -> PluginDefinition {
        PluginDefinition {
            id: "my_plugin".to_string(),
            name: "My Plugin".to_string(),
            version: Some("1.0.0".to_string()),
            main_file: "bin/plugin.wasm".to_string(),
            r#type: "wasm-plugin".to_string(),
            source_path: None,
            table_config_files: vec![],
            supported_databases: vec![],
            domain_code: None,
            application_code: None,
            module_code: None,
            vendor_name: None,
            vendor_url: None,
            vendor_contact: None,
            development_languages: vec![],
            description: None,
            dependencies: vec![],
            services: vec![],
            datasource_id: None,
        }
    }

    /// 有效的 manifest.json（包含 type / source_path 等必填字段）
    fn valid_manifest_json() -> String {
        r#"{
            "manifest_version": "1.0",
            "plugin": {
                "type": "wasm-plugin",
                "id": "my_plugin",
                "name": "My Plugin",
                "version": "1.0.0",
                "main_file": "bin/plugin.wasm",
                "source_path": "."
            }
        }"#
        .to_string()
    }

    // ==================== validate_definition 纯逻辑 ====================

    #[test]
    fn test_validate_definition_valid() {
        let def = make_valid_definition();
        assert!(DefinitionUtils::validate_definition(&def).is_ok());
    }

    #[test]
    fn test_validate_definition_empty_id_errors() {
        let mut def = make_valid_definition();
        def.id = String::new();
        let err = DefinitionUtils::validate_definition(&def).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("ID"));
    }

    #[test]
    fn test_validate_definition_empty_name_errors() {
        let mut def = make_valid_definition();
        def.name = String::new();
        let err = DefinitionUtils::validate_definition(&def).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("名称"));
    }

    #[test]
    fn test_validate_definition_empty_main_file_errors() {
        let mut def = make_valid_definition();
        def.main_file = String::new();
        let err = DefinitionUtils::validate_definition(&def).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("主文件"));
    }

    // ==================== parse_plugin_definition 解析 ====================

    #[test]
    fn test_parse_plugin_definition_valid() {
        let dir = TempDir::new("valid");
        dir.write("manifest.json", &valid_manifest_json());

        let def = DefinitionUtils::parse_plugin_definition(dir.path()).unwrap();
        assert_eq!(def.id, "my_plugin");
        assert_eq!(def.name, "My Plugin");
        assert_eq!(def.version.as_deref(), Some("1.0.0"));
        assert_eq!(def.main_file, "bin/plugin.wasm");
        assert_eq!(def.r#type, "wasm-plugin");
    }

    #[test]
    fn test_parse_plugin_definition_missing_file_errors() {
        let dir = TempDir::new("missing_manifest");

        let err = DefinitionUtils::parse_plugin_definition(dir.path()).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("manifest.json 不存在"));
    }

    #[test]
    fn test_parse_plugin_definition_invalid_json_errors() {
        let dir = TempDir::new("invalid_json");
        dir.write("manifest.json", "{ not valid json");

        let err = DefinitionUtils::parse_plugin_definition(dir.path()).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("解析 manifest.json 失败"));
    }

    #[test]
    fn test_parse_plugin_definition_missing_plugin_object_errors() {
        let dir = TempDir::new("no_plugin");
        // manifest 缺少 plugin 对象
        dir.write("manifest.json", r#"{"manifest_version": "1.0"}"#);

        let err = DefinitionUtils::parse_plugin_definition(dir.path()).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("plugin 对象"));
    }

    #[test]
    fn test_parse_plugin_definition_missing_required_fields_errors() {
        let dir = TempDir::new("missing_fields");
        // plugin 缺少 type 与 main_file
        dir.write(
            "manifest.json",
            r#"{"manifest_version":"1.0","plugin":{"id":"p","name":"x","source_path":"."}}"#,
        );

        let err = DefinitionUtils::parse_plugin_definition(dir.path()).unwrap_err();
        assert!(matches!(err, PluginError::Metadata(_)));
        assert!(err.to_string().contains("解析 plugin 定义失败"));
    }

    #[test]
    fn test_parse_from_install_path_equivalent() {
        let dir = TempDir::new("install_path");
        dir.write("manifest.json", &valid_manifest_json());

        let def = DefinitionUtils::parse_from_install_path(dir.path()).unwrap();
        assert_eq!(def.id, "my_plugin");
    }

    #[tokio::test]
    async fn test_parse_plugin_definition_async_equivalent() {
        let dir = TempDir::new("async");
        dir.write("manifest.json", &valid_manifest_json());

        let def = DefinitionUtils::parse_plugin_definition_async(dir.path()).await.unwrap();
        assert_eq!(def.id, "my_plugin");
        assert_eq!(def.version.as_deref(), Some("1.0.0"));
    }
}
