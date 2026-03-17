//! 插件源获取器 - 根据来源类型获取插件包

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reqwest::Client;

use crate::error::PluginError;
use crate::types::PluginSource;

/// 注册表配置
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub registry_url: String,
    pub auth_token: Option<String>,
    pub timeout_seconds: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.example.com".to_string(),
            auth_token: None,
            timeout_seconds: 60,
        }
    }
}

/// 注册表插件信息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegistryPluginInfo {
    pub plugin_id: String,
    pub name: String,
    pub versions: Vec<RegistryPluginVersion>,
}

/// 注册表插件版本信息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegistryPluginVersion {
    pub version: String,
    pub download_url: String,
    pub checksum: Option<String>,
    pub published_at: Option<String>,
}

/// 插件源获取器 - 根据来源类型获取插件包
pub struct PluginSourceFetcher {
    http_client: Client,
    temp_dir: PathBuf,
    registry_config: RegistryConfig,
}

impl PluginSourceFetcher {
    /// 创建新的插件源获取器
    pub fn new(temp_dir: PathBuf) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("创建 HTTP 客户端失败"),
            temp_dir,
            registry_config: RegistryConfig::default(),
        }
    }
    
    /// 创建新的插件源获取器（带注册表配置）
    pub fn with_registry_config(temp_dir: PathBuf, registry_config: RegistryConfig) -> Self {
        Self {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(registry_config.timeout_seconds))
                .build()
                .expect("创建 HTTP 客户端失败"),
            temp_dir,
            registry_config,
        }
    }
    
    /// 根据来源类型获取插件包，返回临时目录路径
    pub async fn fetch(&self, source: &PluginSource) -> Result<PathBuf, PluginError> {
        match source {
            PluginSource::Zip { path } => {
                self.fetch_from_local(path).await
            }
            PluginSource::Url { url, headers } => {
                self.fetch_from_url(url, headers).await
            }
            PluginSource::Registry { plugin_id, version } => {
                self.fetch_from_registry(plugin_id, version.as_deref()).await
            }
            PluginSource::Directory { path } => {
                self.fetch_from_directory(path).await
            }
        }
    }
    
    /// 从本地路径获取
    pub async fn fetch_from_local(&self, path: &str) -> Result<PathBuf, PluginError> {
        let path = Path::new(path);
        if !path.exists() {
            return Err(PluginError::Install(format!("文件不存在: {}", path.display())));
        }
        
        // 检查是否为 ZIP 文件
        if path.extension().map_or(false, |ext| ext == "zip") {
            // 解压到临时目录
            let extract_dir = self.temp_dir.join("extracted");
            self.extract_zip(path, &extract_dir).await?;
            return Ok(extract_dir);
        }
        
        // 返回父目录作为临时目录
        Ok(path.parent().unwrap_or(path).to_path_buf())
    }
    
    /// 从 URL 下载
    pub async fn fetch_from_url(
        &self, 
        url: &str, 
        headers: &HashMap<String, String>,
    ) -> Result<PathBuf, PluginError> {
        // 验证 URL
        let parsed_url = url::Url::parse(url)
            .map_err(|e| PluginError::Install(format!("无效的 URL: {}", e)))?;
        
        if !matches!(parsed_url.scheme(), "http" | "https") {
            return Err(PluginError::Install("仅支持 HTTP/HTTPS 协议".to_string()));
        }
        
        // 构建请求
        let mut request = self.http_client.get(url);
        for (key, value) in headers {
            request = request.header(key, value);
        }
        
        // 下载文件
        let response = request.send().await
            .map_err(|e| PluginError::Network(format!("下载失败: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Install(format!(
                "下载失败，HTTP状态码: {}", response.status()
            )));
        }
        
        // 保存到临时文件
        let temp_file = self.temp_dir.join("download.zip");
        let bytes = response.bytes().await
            .map_err(|e| PluginError::Network(format!("读取响应失败: {}", e)))?;
        
        tokio::fs::write(&temp_file, &bytes).await
            .map_err(|e| PluginError::Io(e))?;
        
        // 解压到临时目录
        let temp_extract_dir = self.temp_dir.join("extracted");
        self.extract_zip(&temp_file, &temp_extract_dir).await?;
        
        Ok(temp_extract_dir)
    }
    
    /// 从注册表获取
    pub async fn fetch_from_registry(
        &self, 
        plugin_id: &str, 
        version: Option<&str>,
    ) -> Result<PathBuf, PluginError> {
        // 1. 查询注册表获取插件信息
        let plugin_info = self.query_registry(plugin_id).await?;
        
        // 2. 确定要下载的版本
        let target_version = match version {
            Some(v) => v.to_string(),
            None => {
                // 获取最新版本
                plugin_info.versions
                    .first()
                    .map(|v| v.version.clone())
                    .ok_or_else(|| PluginError::NotFound(format!(
                        "插件 {} 没有可用的版本",
                        plugin_id
                    )))?
            }
        };
        
        // 3. 查找对应版本的信息
        let version_info = plugin_info.versions
            .into_iter()
            .find(|v| v.version == target_version)
            .ok_or_else(|| PluginError::NotFound(format!(
                "插件 {} 版本 {} 不存在",
                plugin_id, target_version
            )))?;
        
        // 4. 下载插件包
        let headers = HashMap::new();
        self.fetch_from_url(&version_info.download_url, &headers).await
    }
    
    /// 查询注册表获取插件信息
    async fn query_registry(&self, plugin_id: &str) -> Result<RegistryPluginInfo, PluginError> {
        let url = format!("{}/api/plugins/{}", self.registry_config.registry_url, plugin_id);
        
        let mut request = self.http_client.get(&url);
        
        // 添加认证头
        if let Some(token) = &self.registry_config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await
            .map_err(|e| PluginError::Network(format!("查询注册表失败: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Network(format!(
                "注册表查询失败，HTTP状态码: {}",
                response.status()
            )));
        }
        
        let plugin_info: RegistryPluginInfo = response.json().await
            .map_err(|e| PluginError::Network(format!("解析JSON失败: {}", e)))?;
        
        Ok(plugin_info)
    }
    
    /// 搜索注册表中的插件
    pub async fn search_registry(&self, query: &str) -> Result<Vec<RegistryPluginInfo>, PluginError> {
        let url = format!(
            "{}/api/plugins/search?q={}",
            self.registry_config.registry_url,
            urlencoding::encode(query)
        );
        
        let mut request = self.http_client.get(&url);
        
        if let Some(token) = &self.registry_config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await
            .map_err(|e| PluginError::Network(format!("搜索注册表失败: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(PluginError::Network(format!(
                "注册表搜索失败，HTTP状态码: {}",
                response.status()
            )));
        }
        
        let results: Vec<RegistryPluginInfo> = response.json().await
            .map_err(|e| PluginError::Network(format!("解析JSON失败: {}", e)))?;
        
        Ok(results)
    }
    
    /// 从目录获取
    pub async fn fetch_from_directory(&self, path: &str) -> Result<PathBuf, PluginError> {
        let dir_path = Path::new(path);
        if !dir_path.exists() || !dir_path.is_dir() {
            return Err(PluginError::Install(format!("目录不存在: {}", path)));
        }
        
        // 验证目录中是否存在 manifest.json
        let manifest_path = dir_path.join("manifest.json");
        if !manifest_path.exists() {
            return Err(PluginError::Install("目录中不存在 manifest.json".to_string()));
        }
        
        // 返回目录路径（已解压）
        Ok(dir_path.to_path_buf())
    }
    
    /// 解压 ZIP 文件 (使用同步操作)
    pub async fn extract_zip(&self, zip_path: &Path, dest_dir: &Path) -> Result<(), PluginError> {
        if dest_dir.exists() {
            std::fs::remove_dir_all(dest_dir).ok();
        }
        std::fs::create_dir_all(dest_dir)
            .map_err(|e| PluginError::Io(e))?;
        
        // 使用同步的 zip crate
        let file = std::fs::File::open(zip_path)
            .map_err(|e| PluginError::Io(e))?;
        
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| PluginError::Zip(e.to_string()))?;
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| PluginError::Zip(e.to_string()))?;
            
            let outpath = dest_dir.join(file.mangled_name());
            
            if file.is_dir() {
                std::fs::create_dir_all(&outpath)
                    .map_err(|e| PluginError::Io(e))?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(p)
                            .map_err(|e| PluginError::Io(e))?;
                    }
                }
                
                if outpath.exists() {
                    std::fs::remove_file(&outpath).ok();
                }
                
                let mut outfile = std::fs::File::create(&outpath)
                    .map_err(|e| PluginError::Io(e))?;
                
                std::io::copy(&mut file, &mut outfile)
                    .map_err(|e| PluginError::Io(e))?;
            }
        }
        
        Ok(())
    }
}
