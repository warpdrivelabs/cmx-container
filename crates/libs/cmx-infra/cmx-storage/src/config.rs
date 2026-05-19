//! 存储配置模块
//!
//! 提供存储实例的 TOML 配置解析和管理，支持 Local 和 S3 两种存储类型。
//! 配置通过 `cmx_utils::Config` 系统加载，支持从配置文件读取配置项。

use crate::error;
use serde::{Deserialize, Serialize};

/// 存储类型枚举
///
/// 表示存储后端的具体类型，决定使用哪种存储服务实现。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    /// 本地文件系统存储
    ///
    /// 将文件存储在本地磁盘的指定目录中，适用于开发和测试环境。
    #[default]
    Local,
    /// Amazon S3 及兼容的对象存储
    ///
    /// 支持 AWS S3、MinIO、腾讯云 COS、阿里云 OSS 等 S3 兼容存储服务。
    S3,
}

/// 单个存储实例配置
///
/// 定义一个存储平台实例的完整配置信息，包括连接参数和功能开关。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInstanceConfig {
    /// 存储平台的唯一标识符
    ///
    /// 用于在多平台场景下区分不同的存储实例。
    pub platform: String,
    /// 存储类型：`local` 或 `s3`
    #[serde(rename = "storage_type")]
    pub storage_type: StorageType,
    /// 是否启用该存储平台，默认为 `true`
    #[serde(default = "default_true")]
    pub enable_storage: bool,
    /// 文件访问的基础域名，注意应以 `/` 结尾
    ///
    /// 用于拼接生成文件的访问 URL。
    pub domain: Option<String>,
    /// 存储路径的基础前缀
    ///
    /// 所有上传的文件路径都会以此为前缀。
    #[serde(default)]
    pub base_path: String,

    // S3 类型字段
    /// S3 Access Key ID
    ///
    /// 用于认证 S3 服务的访问密钥。
    pub access_key: Option<String>,
    /// S3 Secret Access Key
    ///
    /// 用于认证 S3 服务的秘密密钥。
    pub secret_key: Option<String>,
    /// S3 区域（Region）
    ///
    /// 如 `us-east-1`、`ap-northeast-1`。
    pub region: Option<String>,
    /// S3 API 端点 URL
    ///
    /// 支持自定义 S3 兼容服务的端点地址。
    pub endpoint: Option<String>,
    /// S3 桶名称（Bucket Name）
    pub bucket_name: Option<String>,

    // Local 类型字段
    /// 是否启用直接访问
    ///
    /// 启用后可通过 `storage_path` 直接访问文件，线上环境建议使用 Nginx 代理。
    #[serde(default)]
    pub enable_access: bool,
    /// 文件路径匹配模式
    ///
    /// 用于配置允许访问的文件路径格式。
    pub path_patterns: Option<String>,
    /// 本地存储的物理根目录路径
    ///
    /// 仅对 `Local` 类型生效。
    pub storage_path: Option<String>,
}

fn default_true() -> bool {
    true
}

/// 存储管理器配置
///
/// 包含所有存储实例的配置信息和默认平台设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageManagerConfig {
    /// 所有存储实例的配置列表
    pub instances: Vec<StorageInstanceConfig>,
    /// 默认存储平台的标识符
    ///
    /// 若未指定，则自动选择第一个已启用的存储实例。
    pub default_platform: Option<String>,
}

impl StorageManagerConfig {
    /// 从 `cmx_utils::Config` 加载存储配置
    ///
    /// 从配置文件中读取 `storage` 节点下的配置。
    ///
    /// # Arguments
    ///
    /// * `config` - 应用程序配置实例
    ///
    /// # Returns
    ///
    /// 成功时返回解析后的 `StorageManagerConfig`。
    ///
    /// # Errors
    ///
    /// 当配置解析失败时返回 `error::Error::ConfigError`。
    pub fn from_config(config: &cmx_utils::Config) -> error::Result<Self> {
        config
            .get_as("storage")
            .map_err(|e| error::Error::ConfigError(format!("加载存储配置失败: {}", e)))
    }

    /// 获取所有已启用的存储实例
    ///
    /// # Returns
    ///
    /// 返回配置中 `enable_storage` 为 `true` 的所有存储实例引用。
    pub fn enabled_instances(&self) -> Vec<&StorageInstanceConfig> {
        self.instances
            .iter()
            .filter(|s| s.enable_storage)
            .collect()
    }

    /// 获取默认存储平台标识
    ///
    /// # Returns
    ///
    /// 优先返回 `default_platform` 配置项；若未设置则返回第一个已启用的存储实例。
    pub fn get_default_platform(&self) -> Option<&str> {
        if let Some(ref platform) = self.default_platform {
            Some(platform.as_str())
        } else {
            self.enabled_instances().first().map(|s| s.platform.as_str())
        }
    }
}

impl StorageInstanceConfig {
    /// 拼接文件访问 URL。
    ///
    /// 对于 Local 类型：直接拼接 `domain + path`。
    /// 对于 S3 类型：拼接 `domain + bucket_name + path`（S3 路径格式要求包含 bucket）。
    ///
    /// # Arguments
    ///
    /// * `path` - 文件的存储路径（不含域名和 bucket 部分）
    ///
    /// # Returns
    ///
    /// 拼接后的完整访问 URL。
    pub fn get_access_url(&self, path: &str) -> String {
        let domain = self.domain.as_deref();
        let path = path.trim_start_matches('/');

        match self.storage_type {
            StorageType::Local => {
                // Local 类型：直接拼接 domain + path
                if let Some(domain) = domain {
                    let domain = if domain.ends_with('/') { domain } else { &format!("{}/", domain) };
                    format!("{}{}", domain, path)
                } else {
                    path.to_string()
                }
            }
            StorageType::S3 => {
                // S3 类型：拼接 domain + bucket + path
                // S3 bucket 有两种 URL 风格：
                // 1. 路径风格：http://endpoint/bucket/path  （如 MinIO 自建服务）
                // 2. 虚拟主机风格：http://bucket.endpoint/path （如 AWS S3,需要有域名才行）
                // 此处采用路径风格：endpoint + bucket + base_path + relative_path

                // 采用路径风格：http://endpoint/bucket/base_path/path
                // 注意：endpoint 需包含协议和端口（如 http://192.168.1.14:9000）
                // MinIO 自建服务或 IP 访问只能使用路径风格，无法使用虚拟主机风格

                if let Some(domain) = domain {
                    let domain = domain.trim_end_matches('/');
                    let bucket = self.bucket_name.as_deref().unwrap_or("");
                    format!("{}/{}/{}/{}", domain, bucket, self.base_path, path)
                } else {
                    // 无 domain 时返回 bucket + base_path + path
                    let bucket = self.bucket_name.as_deref().unwrap_or("");
                    format!("{}/{}/{}", bucket, self.base_path, path)
                }
            }
        }
    }

    /// 获取存储根路径
    ///
    /// # Returns
    ///
    /// `Local` 类型返回 `storage_path`（若未设置则返回 `base_path`）；
    /// `S3` 类型返回 `base_path`。
    pub fn get_root_path(&self) -> String {
        match self.storage_type {
            StorageType::Local => {
                self.storage_path.clone().unwrap_or_else(|| self.base_path.clone())
            }
            StorageType::S3 => self.base_path.clone(),
        }
    }
}
