//! 备份管理模块
//! 
//! 提供插件备份和恢复功能

use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::file::FileStorage;

/// 备份信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// 备份路径
    pub path: PathBuf,
    /// 版本
    pub version: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 大小（字节）
    pub size: u64,
}

/// 备份管理器
pub struct BackupManager {
    /// 备份根目录
    backup_root: PathBuf,
    /// 文件存储
    file_storage: Arc<FileStorage>,
}

impl BackupManager {
    /// 创建新的备份管理器
    pub fn new(backup_root: PathBuf) -> Self {
        Self {
            backup_root,
            file_storage: Arc::new(FileStorage::new(Path::new(""))),
        }
    }
    
    /// 获取备份目录
    fn backup_dir(&self, plugin_id: &str) -> PathBuf {
        self.backup_root.join(plugin_id)
    }
    
    /// 创建备份
    pub async fn create_backup(
        &self,
        plugin_id: &str,
        version: &str,
        source_path: &Path,
    ) -> std::io::Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("{}_{}", version, timestamp);
        let backup_path = self.backup_dir(plugin_id).join(&backup_name);
        
        self.file_storage.create_dir(backup_path.parent().unwrap())?;
        self.file_storage.copy_dir(source_path, &backup_path)?;
        
        Ok(backup_path)
    }
    
    /// 恢复备份
    pub async fn restore_backup(
        &self,
        backup_path: &Path,
        target_path: &Path,
    ) -> std::io::Result<()> {
        if !backup_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("备份不存在: {:?}", backup_path),
            ));
        }
        
        if target_path.exists() {
            self.file_storage.remove_dir(target_path)?;
        }
        
        self.file_storage.copy_dir(backup_path, target_path)
    }
    
    /// 列出所有备份
    pub async fn list_backups(&self, plugin_id: &str) -> std::io::Result<Vec<BackupInfo>> {
        let backup_dir = self.backup_dir(plugin_id);
        
        if !backup_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut backups = Vec::new();
        
        for entry in std::fs::read_dir(&backup_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                let parts: Vec<&str> = name.splitn(2, '_').collect();
                let version = parts.first().unwrap_or(&"unknown").to_string();
                
                let size = self.file_storage.get_dir_size(&path)?;
                let metadata = entry.metadata()?;
                let created_at = metadata.modified()
                    .map(|t| {
                        let datetime: DateTime<Utc> = t.into();
                        datetime
                    })
                    .unwrap_or_else(|_| Utc::now());
                
                backups.push(BackupInfo {
                    path,
                    version,
                    created_at,
                    size,
                });
            }
        }

        backups.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(backups)
    }
    
    /// 删除备份
    pub async fn delete_backup(&self, backup_path: &Path) -> std::io::Result<()> {
        self.file_storage.remove_dir(backup_path)
    }
    
    /// 清理过期备份
    pub async fn cleanup_old_backups(
        &self,
        plugin_id: &str,
        keep_count: usize,
    ) -> std::io::Result<usize> {
        let backups = self.list_backups(plugin_id).await?;
        
        if backups.len() <= keep_count {
            return Ok(0);
        }
        
        let to_delete = &backups[keep_count..];
        let mut deleted = 0;
        
        for backup in to_delete {
            self.delete_backup(&backup.path).await?;
            deleted += 1;
        }
        
        Ok(deleted)
    }
}
