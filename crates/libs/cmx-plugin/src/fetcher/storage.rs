//! cmx-storage 存储获取器
//!
//! 通过 cmx-storage 的 GlobalStorageService 下载插件包文件。

use std::path::{Path, PathBuf};

use super::source::PluginSource;
use crate::error::{PluginError, PluginResult};

/// cmx-storage 存储获取器
///
/// 通过 cmx-storage 的 GlobalStorageService 全局单例下载文件到临时目录。
pub struct StorageFetcher {
    temp_dir: PathBuf,
}

impl StorageFetcher {
    /// 创建新的存储获取器
    ///
    /// # Arguments
    ///
    /// * `temp_dir` - 临时文件目录路径
    pub fn new(temp_dir: PathBuf) -> Self {
        Self { temp_dir }
    }

    /// 从 cmx-storage 获取插件包文件
    ///
    /// 通过 GlobalStorageService 下载文件到本地临时目录。
    ///
    /// # Arguments
    ///
    /// * `source` - 插件来源，必须是 `PluginSource::Storage`
    ///
    /// # Returns
    ///
    /// 下载后的本地文件路径。
    ///
    /// # Errors
    ///
    /// * 当来源类型不是 Storage 时返回错误
    /// * 当 cmx-storage 下载失败时返回错误
    /// * 当文件写入失败时返回错误
    pub async fn fetch(&self, source: &PluginSource) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Storage { file_id, checksum } => {
                let service = cmx_storage::global::GlobalStorageService::get().service();
                let download = service.download(file_id).await.map_err(|e| {
                    PluginError::Fetcher(format!("从 cmx-storage 下载文件失败: {}", e))
                })?;

                let filename = download
                    .file_info
                    .original_filename
                    .unwrap_or_else(|| format!("{}.zip", file_id));
                let target_path = self.temp_dir.join(&filename);

                std::fs::create_dir_all(&self.temp_dir)?;

                std::fs::write(&target_path, &download.data)?;

                if let Some(expected_checksum) = checksum {
                    self.verify_checksum(&target_path, expected_checksum)?;
                }

                tracing::info!(
                    "从存储服务下载完成: {} -> {} ({} bytes)",
                    file_id,
                    target_path.display(),
                    download.data.len()
                );

                Ok(target_path)
            }
            _ => Err(PluginError::Fetcher("来源类型不是 cmx-storage".to_string())),
        }
    }

    /// 验证 MD5 校验和
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
}
