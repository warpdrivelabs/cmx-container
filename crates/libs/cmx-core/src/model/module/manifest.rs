//! 模块迁移包清单模型
//!
//! 对应模块迁移包顶层 module.manifest.json 的数据结构。
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 模块聚合包清单（对应 module.manifest.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleManifest {
    /// 清单版本
    pub manifest_version: String,
    /// 模块定义信息
    pub module: ModuleInfo,
    /// 迁移包版本号 = 导出时间戳 yyyyMMddHHmmSS（导出服务自动生成，无需手动输入）
    pub package_version: String,
    /// 资源文件清单
    #[serde(default)]
    pub resources: ModuleResources,
    /// 模块包含的插件子包列表
    #[serde(default)]
    pub plugins: Vec<ModulePluginEntry>,
    /// 资源统计信息
    #[serde(default)]
    pub stats: ModuleStats,
    /// 校验和（sha256）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// 签名算法
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algorithm: Option<String>,
    /// 签名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 签名者 key id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_key_id: Option<String>,
}

/// 模块定义信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleInfo {
    /// 模块编码
    pub code: String,
    /// 模块名称
    pub name: String,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 模块描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 模块资源文件清单（各资源的相对路径列表）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleResources {
    /// 表单文件路径列表
    #[serde(default)]
    pub forms: Vec<String>,
    /// 菜单文件路径列表
    #[serde(default)]
    pub menus: Vec<String>,
    /// 元数据文件路径列表
    #[serde(default)]
    pub metadata: Vec<String>,
    /// 权限文件路径列表
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// 模块包含的插件子包条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModulePluginEntry {
    /// 插件 ID
    pub id: String,
    /// 插件版本
    pub version: String,
    /// 子包在模块包内的相对路径
    pub package: String,
}

/// 模块资源统计信息(导出时自动填充)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModuleStats {
    /// 表单数量
    #[serde(default)]
    pub form_count: usize,
    /// 菜单数量
    #[serde(default)]
    pub menu_count: usize,
    /// 权限条数
    #[serde(default)]
    pub permission_count: usize,
    /// 元数据表个数
    #[serde(default)]
    pub table_count: usize,
    /// 插件个数
    #[serde(default)]
    pub plugin_count: usize,
}

impl ModuleManifest {
    /// 序列化为稳定字节流，用于计算 checksum / Ed25519 签名。
    ///
    /// # Errors
    /// 序列化失败时返回错误
    pub fn to_canonical_bytes(&self) -> serde_json::Result<Vec<u8>> {
        let s = serde_json::to_string(self)?;
        Ok(s.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_deserialize_minimal() {
        let json = r#"{
            "manifest_version": "1.0",
            "module": {
                "code": "GL",
                "name": "总账",
                "domain_code": "FIN",
                "application_code": "FI",
                "description": "总账模块"
            },
            "package_version": "20260630103000",
            "resources": {
                "forms": ["forms/voucher_form.json"],
                "menus": [],
                "metadata": [],
                "permissions": []
            },
            "plugins": []
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).expect("反序列化应成功");
        assert_eq!(m.module.code, "GL");
        assert_eq!(m.package_version, "20260630103000");
        assert_eq!(m.resources.forms.len(), 1);
        assert!(m.plugins.is_empty());
    }

    #[test]
    fn test_manifest_deserialize_default_resources() {
        // resources 字段缺失时应使用默认值(全空)
        let json = r#"{
            "manifest_version":"1.0",
            "module":{"code":"X","name":"x","domain_code":"D","application_code":"A"},
            "package_version":"20260630103000",
            "plugins":[]
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).expect("反序列化应成功");
        assert!(m.resources.forms.is_empty());
        assert!(m.resources.menus.is_empty());
    }

    #[test]
    fn test_package_version_is_14_digit_timestamp() {
        let json = r#"{
            "manifest_version":"1.0",
            "module":{"code":"X","name":"x","domain_code":"D","application_code":"A"},
            "package_version":"20260630103000",
            "resources":{"forms":[],"menus":[],"metadata":[],"permissions":[]},
            "plugins":[]
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).expect("反序列化应成功");
        assert_eq!(m.package_version.len(), 14, "package_version 应为14位时间戳");
        assert!(
            m.package_version.chars().all(|c| c.is_ascii_digit()),
            "package_version 应全为数字"
        );
    }

    #[test]
    fn test_manifest_with_plugins_and_checksum() {
        let json = r#"{
            "manifest_version":"1.0",
            "module":{"code":"GL","name":"总账","domain_code":"FIN","application_code":"FI"},
            "package_version":"20260630103000",
            "resources":{"forms":["forms/a.json"],"menus":[],"metadata":["metadata/b.json"],"permissions":["permissions/c.json"]},
            "plugins":[
                {"id":"plugin_gl_posting","version":"1.0.0","package":"plugins/plugin_gl_posting.zip"}
            ],
            "checksum":"sha256:abc123"
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).expect("反序列化应成功");
        assert_eq!(m.plugins.len(), 1);
        assert_eq!(m.plugins[0].id, "plugin_gl_posting");
        assert_eq!(m.resources.metadata.len(), 1);
        assert_eq!(m.checksum.as_deref(), Some("sha256:abc123"));
    }

    #[test]
    fn test_to_canonical_bytes_roundtrip() {
        let json = r#"{
            "manifest_version":"1.0",
            "module":{"code":"GL","name":"总账","domain_code":"FIN","application_code":"FI"},
            "package_version":"20260630103000",
            "resources":{"forms":[],"menus":[],"metadata":[],"permissions":[]},
            "plugins":[]
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        let bytes = m.to_canonical_bytes().expect("序列化应成功");
        assert!(!bytes.is_empty());
        // 反序列化回来应相等
        let m2: ModuleManifest = serde_json::from_slice(&bytes).expect("反序列化应成功");
        assert_eq!(m2.module.code, m.module.code);
        assert_eq!(m2.package_version, m.package_version);
    }
}
