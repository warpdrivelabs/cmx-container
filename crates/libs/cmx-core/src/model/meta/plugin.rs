//! 插件定义与 ZIP 装配清单的 JSON 结构（含签名与验签载荷）

use serde::{Deserialize, Serialize};

/// 装配清单/插件定义中推荐的数据库标识（小写、规范名）。
pub mod supported_db {
    /// MySQL / MariaDB
    pub const MYSQL: &str = "mysql";
    /// PostgreSQL
    pub const POSTGRES: &str = "postgres";
    /// SQLite
    pub const SQLITE: &str = "sqlite";
    /// Oracle Database
    pub const ORACLE: &str = "oracle";
}

/// 装配清单/插件定义中推荐的开发语言标识（小写、规范名）。
pub mod supported_lang {
    pub const RUST: &str = "rust";
    pub const JAVASCRIPT: &str = "javascript";
    pub const TYPESCRIPT: &str = "typescript";
    pub const C: &str = "c";
    pub const CPP: &str = "c++";
    pub const GO: &str = "go";
    pub const PYTHON: &str = "python";
    pub const JAVA: &str = "java";
    pub const KOTLIN: &str = "kotlin";
}

/// 插件依赖定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// 依赖插件ID
    pub plugin_id: String,
    /// 版本约束（如 "^1.0.0", ">=2.0.0"）
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 是否为可选依赖
    #[serde(default)]
    pub optional: bool,
}

/// 插件服务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginService {
    /// 服务ID
    pub service_id: String,
    /// 服务名称
    pub name: String,
    /// 服务描述
    #[serde(default)]
    pub description: Option<String>,
    /// 服务版本
    #[serde(default)]
    pub version: Option<String>,
    /// 服务入口点（WASM 导出函数名）
    pub entry_point: String,
}

/// 单个插件的定义（可从独立 JSON 文件加载，或从 ZIP 内的 manifest 解析）。
///
/// 表定义无需在插件 JSON 中再列；建表配置（table_config_files）中已指向各表定义 JSON 文件。
///
/// JSON 格式说明：
/// - **main_file**：WASM 二进制文件路径（相对插件根目录或 ZIP 根目录）。
/// - **table_config_files**：本插件使用的「建表配置」JSON 文件列表（即 `TableDefinesConfig` 文件，
///   如 `sys_tables_config.json`、`oracle_tables_config.json`），配置内已定义具体表定义文件列表。
/// - **supported_databases**：本插件声明支持的数据库类型，如 mysql、postgres、sqlite、oracle 等（见 [supported_db]）。
/// - **domain_code** / **application_code** / **module_code**：所属域、应用、模块编码（对应 cmx_domain / cmx_application / cmx_module）。
/// - **vendor_***：开发商信息；**development_languages**：开发语言列表（见 [supported_lang]）。
/// - **dependencies**：插件依赖列表，声明本插件依赖的其他插件。
/// - **services**：插件提供的服务列表，用于服务注册。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    /// 插件唯一标识
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 语义化版本，如 "1.0.0"
    #[serde(default)]
    pub version: Option<String>,
    /// WASM 入口文件路径（相对定义文件所在目录或 ZIP 根）
    pub main_file: String,
    //插件类型 wasm或者rhai
    pub r#type: String,
    //源码路径
    pub source_path: Option<String>,

    /// 本插件使用的建表配置 JSON 文件列表（TableDefinesConfig 格式，其内已定义表定义文件）
    #[serde(default)]
    pub table_config_files: Vec<String>,
    /// 本插件声明支持的数据库类型（小写规范名，如 mysql、postgres、sqlite、oracle）
    #[serde(default)]
    pub supported_databases: Vec<String>,
    /// 所属域编码（对应 cmx_domain.code，如 FIN 财务域）
    #[serde(default)]
    pub domain_code: Option<String>,
    /// 所属应用编码（对应 cmx_application.code，如 GL_ACCT 会计核算）
    #[serde(default)]
    pub application_code: Option<String>,
    /// 所属模块编码（对应 cmx_module.code，如 GL 总账、AR_AP 应收应付）
    #[serde(default)]
    pub module_code: Option<String>,
    /// 开发商/供应商名称
    #[serde(default)]
    pub vendor_name: Option<String>,
    /// 开发商网址
    #[serde(default)]
    pub vendor_url: Option<String>,
    /// 开发商联系方式（如邮箱、电话）
    #[serde(default)]
    pub vendor_contact: Option<String>,
    /// 开发语言列表（小写规范名，如 rust、javascript、c++、go）
    #[serde(default)]
    pub development_languages: Vec<String>,
    /// 可选说明
    #[serde(default)]
    pub description: Option<String>,
    /// 插件依赖列表 todo 现在不支持解析json文件中的依赖，因为格式不一致0320
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    /// 插件提供的服务列表
    #[serde(default)]
    pub services: Vec<PluginService>,
}

/// 用于签名的装配清单载荷（不含签名字段，字段顺序固定以保证序列化稳定）。
///
/// 签名字段（signature_algorithm / signature / signer_key_id）不参与序列化，
/// 验签时用本结构序列化得到的字节与公钥验证 signature。
#[derive(Debug, Clone, Serialize)]
pub struct PluginManifestSigningPayload {
    /// 清单格式版本
    pub manifest_version: Option<String>,
    /// 插件定义
    pub plugin: PluginDefinition,
    /// ZIP 内文件条目相对路径列表
    pub entries: Vec<String>,
}

impl PluginManifestSigningPayload {
    /// 从已解析的清单构造签名载荷（仅包含 manifest_version / plugin / entries）。
    pub fn from_manifest(m: &PluginManifest) -> Self {
        Self {
            manifest_version: m.manifest_version.clone(),
            plugin: m.plugin.clone(),
            entries: m.entries.clone(),
        }
    }

    /// 序列化为用于签名/验签的规范字节（UTF-8 JSON，键顺序固定）。
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

/// 插件 ZIP 压缩包的装配清单（置于 ZIP 根目录或约定路径，如 `manifest.json`）。
///
/// 描述 ZIP 内文件布局与插件元数据，便于安装/校验：
/// - **plugin**：插件定义（与 `PluginDefinition` 同构），其中路径均相对 ZIP 根。
/// - **entries**：ZIP 内应包含的条目相对路径列表，用于装配校验与完整性检查。
///
/// 可选**签名**字段用于防篡改：对「不含签名字段的清单」做规范序列化后签名，
/// 验签时用相同规则还原字节并用公钥验证。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 清单格式版本，便于后续兼容
    #[serde(default)]
    pub manifest_version: Option<String>,
    /// 插件定义（main_file / table_config_files 均为 ZIP 内相对路径）
    pub plugin: PluginDefinition,
    /// ZIP 内文件条目相对路径列表（装配清单，用于校验包内容）
    #[serde(default)]
    pub entries: Vec<String>,
    /// 签名算法标识，如 "Ed25519"（与 signature 同时存在时参与验签）
    #[serde(default)]
    pub signature_algorithm: Option<String>,
    /// 签名字节经 Base64 编码后的字符串（对 PluginManifestSigningPayload 的 JSON 字节签名）
    #[serde(default)]
    pub signature: Option<String>,
    /// 可选：签名者公钥或密钥标识，供宿主从本地信任库查找公钥
    #[serde(default)]
    pub signer_key_id: Option<String>,
}

impl PluginManifest {
    /// 返回用于签名/验签的规范字节（不含 signature_algorithm / signature / signer_key_id）。
    pub fn signing_payload_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let payload = PluginManifestSigningPayload::from_manifest(self);
        payload.to_canonical_bytes()
    }

    /// 是否包含签名信息（algorithm + signature 均存在）。
    pub fn has_signature(&self) -> bool {
        self.signature_algorithm.as_ref().is_some_and(|a| !a.is_empty())
            && self.signature.as_ref().is_some_and(|s| !s.is_empty())
    }
}
