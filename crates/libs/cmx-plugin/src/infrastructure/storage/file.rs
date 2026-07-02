//! 文件存储模块
//!
//! 提供文件和目录操作功能

use std::path::{Path, PathBuf};

/// 文件存储管理器
pub struct FileStorage {
    /// 基础路径
    base_path: PathBuf,
}

impl FileStorage {
    /// 创建新的文件存储
    pub fn new(base_path: &Path) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
        }
    }

    /// 获取基础路径
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// 获取插件安装路径
    pub fn plugin_path(&self, plugin_id: &str) -> PathBuf {
        self.base_path.join("plugins").join(plugin_id)
    }

    /// 检查路径是否存在
    pub fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    /// 创建目录
    pub fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    /// 复制目录
    pub fn copy_dir(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        if !src.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("源目录不存在: {:?}", src),
            ));
        }

        self.create_dir(dst)?;

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if ty.is_dir() {
                self.copy_dir(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }

    /// 删除目录
    pub fn remove_dir(&self, path: &Path) -> std::io::Result<()> {
        if path.exists() {
            std::fs::remove_dir_all(path)
        } else {
            Ok(())
        }
    }

    /// 列出目录内容
    pub fn list_directory(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        for entry in std::fs::read_dir(path)? {
            result.push(entry?.path());
        }
        Ok(result)
    }

    /// 获取目录大小
    pub fn get_dir_size(&self, path: &Path) -> std::io::Result<u64> {
        if !path.exists() {
            return Ok(0);
        }

        let mut size = 0;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let ty = entry.file_type()?;

            if ty.is_dir() {
                size += self.get_dir_size(&entry.path())?;
            } else {
                size += entry.metadata()?.len();
            }
        }
        Ok(size)
    }
}
