//! 远程获取模块
//!
//! 从URL获取插件

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::source::PluginSource;
use crate::error::{PluginError, PluginResult};

/// 远程插件获取器
///
/// 从远程 URL 下载插件包。
pub struct RemoteFetcher {
    /// 临时目录
    temp_dir: PathBuf,
    /// 超时时间（秒）
    timeout_seconds: u64,
    /// 最大重试次数
    max_retries: u32,
}

impl RemoteFetcher {
    /// 创建新的远程获取器
    pub fn new(temp_dir: PathBuf) -> Self {
        Self {
            temp_dir,
            timeout_seconds: 60,
            max_retries: 3,
        }
    }

    /// 设置超时时间
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 获取插件
    ///
    /// 从远程 URL 下载插件包到临时目录。
    pub async fn fetch(&self, source: &PluginSource) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Remote { url, checksum } => {
                let file_name = self.extract_filename(url)?;
                let target_path = self.temp_dir.join(&file_name);

                std::fs::create_dir_all(&self.temp_dir)
                    .map_err(|e| PluginError::Fetcher(format!("创建临时目录失败: {}", e)))?;

                self.download_file_with_retry(url, &target_path).await?;

                if let Some(expected_checksum) = checksum {
                    self.verify_checksum(&target_path, expected_checksum)?;
                }

                Ok(target_path)
            }
            _ => Err(PluginError::Fetcher("来源类型不是远程URL".to_string())),
        }
    }

    /// 从URL提取文件名
    fn extract_filename(&self, url: &str) -> PluginResult<String> {
        let parsed = url::Url::parse(url)
            .map_err(|e| PluginError::Fetcher(format!("解析URL失败: {}", e)))?;

        let path_segments: Vec<&str> = parsed
            .path_segments()
            .map(|s| s.collect())
            .unwrap_or_default();

        if let Some(filename) = path_segments.last()
            && !filename.is_empty()
        {
            return Ok(filename.to_string());
        }

        // 如果无法提取，生成随机文件名
        Ok(format!("plugin_{}.zip", uuid::Uuid::new_v4()))
    }

    /// 带重试的下载文件
    async fn download_file_with_retry(&self, url: &str, target: &Path) -> PluginResult<()> {
        let mut last_error = String::new();

        for attempt in 0..self.max_retries {
            match self.download_file(url, target).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = e.to_string();
                    if attempt < self.max_retries - 1 {
                        tracing::warn!("下载失败，第 {} 次重试: {}", attempt + 1, url);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        Err(PluginError::Fetcher(format!(
            "下载失败，已重试 {} 次: {}",
            self.max_retries, last_error
        )))
    }

    /// 下载文件
    ///
    /// 使用 HTTP 客户端下载文件。
    async fn download_file(&self, url: &str, target: &Path) -> PluginResult<()> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_seconds))
            .user_agent("CMX-Plugin-Manager/1.0")
            .build()
            .map_err(|e| PluginError::Fetcher(format!("创建HTTP客户端失败: {}", e)))?;

        tracing::info!("开始下载: {}", url);

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| PluginError::Fetcher(format!("HTTP请求失败: {}", e)))?;

        if !response.status().is_success() {
            return Err(PluginError::Fetcher(format!(
                "HTTP响应错误: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| PluginError::Fetcher(format!("读取响应体失败: {}", e)))?;

        std::fs::write(target, &bytes)
            .map_err(|e| PluginError::Fetcher(format!("写入文件失败: {}", e)))?;

        tracing::info!(
            "下载完成: {} -> {} ({} bytes)",
            url,
            target.display(),
            bytes.len()
        );

        Ok(())
    }

    /// 验证校验和
    ///
    /// 验证下载文件的 MD5 校验和。
    fn verify_checksum(&self, file_path: &Path, expected: &str) -> PluginResult<()> {
        use std::io::Read;

        let mut file = std::fs::File::open(file_path)
            .map_err(|e| PluginError::Fetcher(format!("打开文件失败: {}", e)))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| PluginError::Fetcher(format!("读取文件失败: {}", e)))?;

        let actual = format!("{:x}", md5::compute(&buffer));

        if actual != expected.to_lowercase() {
            return Err(PluginError::Fetcher(format!(
                "校验和不匹配: 期望 {}, 实际 {}",
                expected, actual
            )));
        }

        tracing::info!("校验和验证通过: {}", file_path.display());

        Ok(())
    }

    /// 验证 SHA256 校验和
    ///
    /// 验证下载文件的 SHA256 校验和。
    #[allow(dead_code)]
    fn verify_sha256(&self, file_path: &Path, expected: &str) -> PluginResult<()> {
        use sha2::{Digest, Sha256};
        use std::io::Read;

        let mut file = std::fs::File::open(file_path)
            .map_err(|e| PluginError::Fetcher(format!("打开文件失败: {}", e)))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| PluginError::Fetcher(format!("读取文件失败: {}", e)))?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let result = hasher.finalize();
        let actual = format!("{:x}", result);

        if actual != expected.to_lowercase() {
            return Err(PluginError::Fetcher(format!(
                "SHA256校验和不匹配: 期望 {}, 实际 {}",
                expected, actual
            )));
        }

        tracing::info!("SHA256校验和验证通过: {}", file_path.display());

        Ok(())
    }

    /// 获取远程文件信息
    ///
    /// 发送 HEAD 请求获取文件信息（大小、类型等）。
    pub async fn get_file_info(&self, url: &str) -> PluginResult<RemoteFileInfo> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("CMX-Plugin-Manager/1.0")
            .build()
            .map_err(|e| PluginError::Fetcher(format!("创建HTTP客户端失败: {}", e)))?;

        let response = client
            .head(url)
            .send()
            .await
            .map_err(|e| PluginError::Fetcher(format!("HEAD请求失败: {}", e)))?;

        if !response.status().is_success() {
            return Err(PluginError::Fetcher(format!(
                "HTTP响应错误: {}",
                response.status()
            )));
        }

        let headers = response.headers();
        let content_length = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(RemoteFileInfo {
            url: url.to_string(),
            content_length,
            content_type,
        })
    }
}

/// 远程文件信息
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    /// 文件 URL
    pub url: String,
    /// 文件大小（字节）
    pub content_length: Option<u64>,
    /// 内容类型
    pub content_type: Option<String>,
}
