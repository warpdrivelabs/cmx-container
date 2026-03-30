//! 本地获取模块
//!
//! 从本地文件获取插件

use std::path::{Path, PathBuf};

use super::source::PluginSource;
use crate::error::{PluginError, PluginResult};

/// 本地插件获取器
pub struct LocalFetcher {
    /// 基础路径
    base_path: PathBuf,
}

impl LocalFetcher {
    /// 创建新的本地获取器
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
        }
    }

    /// 获取插件
    pub async fn fetch(&self, source: &PluginSource) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let full_path = if path.is_absolute() {
                    path.clone()
                } else {
                    self.base_path.join(path)
                };

                if !full_path.exists() {
                    return Err(PluginError::Fetcher(format!("插件文件不存在: {:?}", full_path)));
                }

                Ok(full_path)
            }
            _ => Err(PluginError::Fetcher("来源类型不是本地文件".to_string())),
        }
    }

    /// 检查文件是否存在
    pub fn exists(&self, path: &Path) -> bool {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        };
        full_path.exists()
    }

    /// 获取文件大小
    pub fn file_size(&self, path: &Path) -> PluginResult<u64> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        };

        let metadata = std::fs::metadata(&full_path)
            .map_err(|e| PluginError::Fetcher(format!("获取文件元数据失败: {}", e)))?;

        Ok(metadata.len())
    }

    /// 复制文件到目标目录
    pub async fn copy_to(&self, source: &Path, target: &Path) -> PluginResult<PathBuf> {
        let source_path = if source.is_absolute() {
            source.to_path_buf()
        } else {
            self.base_path.join(source)
        };

        if !source_path.exists() {
            return Err(PluginError::Fetcher(format!("源文件不存在: {:?}", source_path)));
        }

        std::fs::create_dir_all(target.parent().unwrap())
            .map_err(|e| PluginError::Fetcher(format!("创建目标目录失败: {}", e)))?;

        let target_path = target.to_path_buf();
        std::fs::copy(&source_path, &target_path)
            .map_err(|e| PluginError::Fetcher(format!("复制文件失败: {}", e)))?;

        Ok(target_path)
    }
}
