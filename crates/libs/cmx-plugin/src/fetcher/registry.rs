//! 注册表获取模块
//!
//! 从远程插件注册表获取插件

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::source::PluginSource;

/// 插件注册表信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryInfo {
    /// 注册表URL
    pub url: String,
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 下载URL
    pub download_url: String,
    /// 校验和
    pub checksum: Option<String>,
    /// 校验和类型
    pub checksum_type: Option<String>,
    /// 插件描述
    pub description: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 主页
    pub homepage: Option<String>,
    /// 许可证
    pub license: Option<String>,
    /// 关键词
    #[serde(default)]
    pub keywords: Vec<String>,
}

impl RegistryInfo {
    /// 创建新的注册表信息
    pub fn new(url: String) -> Self {
        Self {
            url,
            name: String::new(),
            version: String::new(),
            download_url: String::new(),
            checksum: None,
            checksum_type: None,
            description: None,
            author: None,
            homepage: None,
            license: None,
            keywords: Vec::new(),
        }
    }
}

/// 注册表包版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackageVersion {
    /// 版本号
    pub version: String,
    /// 下载URL
    pub download_url: String,
    /// 校验和
    pub checksum: Option<String>,
    /// 校验和类型
    pub checksum_type: Option<String>,
    /// 发布时间
    #[serde(default)]
    pub published_at: Option<String>,
    /// 是否为最新版本
    #[serde(default)]
    pub is_latest: bool,
}

/// 注册表包详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPackageDetail {
    /// 包名
    pub name: String,
    /// 最新版本
    pub latest_version: String,
    /// 描述
    pub description: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 主页
    pub homepage: Option<String>,
    /// 许可证
    pub license: Option<String>,
    /// 关键词
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 所有版本
    #[serde(default)]
    pub versions: Vec<RegistryPackageVersion>,
    /// 下载统计
    #[serde(default)]
    pub downloads: Option<u64>,
}

/// 注册表搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrySearchResult {
    /// 包名
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 关键词
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 下载统计
    #[serde(default)]
    pub downloads: Option<u64>,
}

/// 注册表插件获取器
pub struct RegistryFetcher {
    /// 注册表信息
    registry_info: RegistryInfo,
    /// 临时目录
    temp_dir: PathBuf,
    /// HTTP 客户端
    client: reqwest::Client,
}

impl RegistryFetcher {
    /// 创建新的注册表获取器
    pub fn new(registry_info: RegistryInfo, temp_dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("CMX-Plugin-Manager/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            registry_info,
            temp_dir,
            client,
        }
    }

    /// 获取插件
    pub async fn fetch(&self, source: &PluginSource) -> Result<PathBuf, String> {
        match source {
            PluginSource::Registry {
                registry_url,
                package_name,
                version_constraint,
            } => {
                let info = self.resolve_package(registry_url, package_name, version_constraint).await?;
                self.download_package(&info).await
            }
            _ => Err("来源类型不是注册表".to_string()),
        }
    }

    /// 根据名称获取插件
    pub async fn fetch_by_name(&self, package_name: &str, version_constraint: Option<String>) -> Result<PathBuf, String> {
        let info = self.resolve_package(&self.registry_info.url, package_name, &version_constraint).await?;
        self.download_package(&info).await
    }

    /// 解析包信息
    ///
    /// 从注册表查询包信息，获取下载URL和校验和。
    pub async fn resolve_package(
        &self,
        registry_url: &str,
        package_name: &str,
        version_constraint: &Option<String>,
    ) -> Result<RegistryInfo, String> {
        let url = self.build_package_url(registry_url, package_name);

        tracing::info!("查询注册表: {}", url);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("请求注册表失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("注册表响应错误: {} - {}", response.status(), package_name));
        }

        let detail: RegistryPackageDetail = response.json()
            .await
            .map_err(|e| format!("解析注册表响应失败: {}", e))?;

        let version_info = self.select_version(&detail, version_constraint)?;

        let info = RegistryInfo {
            url: registry_url.to_string(),
            name: package_name.to_string(),
            version: version_info.version.clone(),
            download_url: version_info.download_url.clone(),
            checksum: version_info.checksum.clone(),
            checksum_type: version_info.checksum_type.clone(),
            description: detail.description.clone(),
            author: detail.author.clone(),
            homepage: detail.homepage.clone(),
            license: detail.license.clone(),
            keywords: detail.keywords.clone(),
        };

        tracing::info!(
            "解析包成功: {}@{} -> {}",
            package_name,
            info.version,
            info.download_url
        );

        Ok(info)
    }

    /// 下载包
    ///
    /// 从下载URL下载插件包到临时目录。
    pub async fn download_package(&self, info: &RegistryInfo) -> Result<PathBuf, String> {
        if info.download_url.is_empty() {
            return Err("下载URL为空".to_string());
        }

        std::fs::create_dir_all(&self.temp_dir)
            .map_err(|e| format!("创建临时目录失败: {}", e))?;

        let file_name = self.extract_filename(&info.download_url, &info.name, &info.version);
        let target_path = self.temp_dir.join(&file_name);

        if target_path.exists() {
            tracing::info!("文件已存在，跳过下载: {}", target_path.display());
            return Ok(target_path);
        }

        tracing::info!("开始下载: {} -> {}", info.download_url, target_path.display());

        let response = self.client.get(&info.download_url)
            .send()
            .await
            .map_err(|e| format!("下载请求失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("下载响应错误: {}", response.status()));
        }

        let bytes = response.bytes()
            .await
            .map_err(|e| format!("读取响应体失败: {}", e))?;

        std::fs::write(&target_path, &bytes)
            .map_err(|e| format!("写入文件失败: {}", e))?;

        if let Some(ref checksum) = info.checksum {
            self.verify_checksum(&target_path, checksum, info.checksum_type.as_deref())?;
        }

        tracing::info!(
            "下载完成: {} ({} bytes)",
            target_path.display(),
            bytes.len()
        );

        Ok(target_path)
    }

    /// 搜索插件
    ///
    /// 在注册表中搜索插件。
    pub async fn search(&self, registry_url: &str, query: &str) -> Result<Vec<RegistrySearchResult>, String> {
        let url = self.build_search_url(registry_url, query);

        tracing::info!("搜索注册表: {}", url);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("搜索请求失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("搜索响应错误: {}", response.status()));
        }

        let results: Vec<RegistrySearchResult> = response.json()
            .await
            .map_err(|e| format!("解析搜索结果失败: {}", e))?;

        tracing::info!("搜索完成: 找到 {} 个结果", results.len());

        Ok(results)
    }

    /// 获取插件详情
    ///
    /// 获取插件在注册表中的详细信息。
    pub async fn get_package_info(&self, registry_url: &str, package_name: &str) -> Result<RegistryPackageDetail, String> {
        let url = self.build_package_url(registry_url, package_name);

        tracing::info!("获取包详情: {}", url);

        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| format!("请求包详情失败: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("获取详情响应错误: {}", response.status()));
        }

        let detail: RegistryPackageDetail = response.json()
            .await
            .map_err(|e| format!("解析包详情失败: {}", e))?;

        tracing::info!("获取包详情成功: {}@{}", detail.name, detail.latest_version);

        Ok(detail)
    }

    /// 构建包URL
    fn build_package_url(&self, registry_url: &str, package_name: &str) -> String {
        let base = registry_url.trim_end_matches('/');
        format!("{}/packages/{}", base, package_name)
    }

    /// 构建搜索URL
    fn build_search_url(&self, registry_url: &str, query: &str) -> String {
        let base = registry_url.trim_end_matches('/');
        let encoded = urlencoding::encode(query);
        format!("{}/search?q={}", base, encoded)
    }

    /// 选择版本
    fn select_version(
        &self,
        detail: &RegistryPackageDetail,
        version_constraint: &Option<String>,
    ) -> Result<RegistryPackageVersion, String> {
        if detail.versions.is_empty() {
            return Err(format!("包 {} 没有可用版本", detail.name));
        }

        match version_constraint {
            Some(constraint) => {
                let parsed_constraint = crate::domain::version::VersionConstraint::parse(constraint)
                    .map_err(|e| format!("解析版本约束失败: {}", e))?;

                let matching_versions: Vec<_> = detail.versions.iter()
                    .filter(|v| {
                        if let Ok(version) = crate::domain::version::SemanticVersion::parse(&v.version) {
                            parsed_constraint.satisfies(&version)
                        } else {
                            false
                        }
                    })
                    .collect();

                if matching_versions.is_empty() {
                    return Err(format!(
                        "没有找到满足约束 {} 的版本",
                        constraint
                    ));
                }

                matching_versions.into_iter()
                    .max_by_key(|v| {
                        crate::domain::version::SemanticVersion::parse(&v.version).ok()
                            .unwrap_or_else(|| crate::domain::version::SemanticVersion::new(0, 0, 0))
                    })
                    .cloned()
                    .ok_or_else(|| "选择版本失败".to_string())
            }
            None => {
                detail.versions.iter()
                    .find(|v| v.is_latest)
                    .cloned()
                    .or_else(|| {
                        detail.versions.first().cloned()
                    })
                    .ok_or_else(|| "没有可用版本".to_string())
            }
        }
    }

    /// 提取文件名
    fn extract_filename(&self, url: &str, package_name: &str, version: &str) -> String {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(segments) = parsed.path_segments() {
                if let Some(filename) = segments.last() {
                    if !filename.is_empty() {
                        return filename.to_string();
                    }
                }
            }
        }

        format!("{}-{}.zip", package_name, version)
    }

    /// 验证校验和
    fn verify_checksum(&self, file_path: &PathBuf, expected: &str, checksum_type: Option<&str>) -> Result<(), String> {
        use std::io::Read;

        let mut file = std::fs::File::open(file_path)
            .map_err(|e| format!("打开文件失败: {}", e))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let actual = match checksum_type {
            Some("sha256") | Some("SHA256") => {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
            Some("sha1") | Some("SHA1") => {
                use sha1::{Sha1, Digest as Sha1Digest};
                let mut hasher = Sha1::new();
                hasher.update(&buffer);
                format!("{:x}", hasher.finalize())
            }
            _ => {
                format!("{:x}", md5::compute(&buffer))
            }
        };

        if actual != expected.to_lowercase() {
            return Err(format!("校验和不匹配: 期望 {}, 实际 {}", expected, actual));
        }

        tracing::info!("校验和验证通过: {}", file_path.display());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_info_new() {
        let info = RegistryInfo::new("https://registry.example.com".to_string());
        assert_eq!(info.url, "https://registry.example.com");
        assert!(info.name.is_empty());
    }

    #[test]
    fn test_extract_filename() {
        let fetcher = RegistryFetcher::new(
            RegistryInfo::new("https://registry.example.com".to_string()),
            PathBuf::from("/tmp"),
        );

        let filename = fetcher.extract_filename(
            "https://example.com/packages/my-plugin-1.0.0.zip",
            "my-plugin",
            "1.0.0"
        );
        assert_eq!(filename, "my-plugin-1.0.0.zip");

        let filename = fetcher.extract_filename(
            "https://example.com/download",
            "my-plugin",
            "1.0.0"
        );
        assert_eq!(filename, "my-plugin-1.0.0.zip");
    }
}
