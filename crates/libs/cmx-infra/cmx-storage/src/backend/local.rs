//! 本地文件系统存储后端实现
//!
//! 基于 OpenDAL Fs 服务提供本地文件系统存储能力。

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use opendal::services::Fs;
use opendal::{EntryMode, Operator};

use crate::config::StorageInstanceConfig;
use crate::error::{Error, Result};
use crate::types::*;

use super::StorageBackend;

/// 本地文件系统存储后端
///
/// 基于 OpenDAL Fs 服务实现，提供本地磁盘存储操作能力。
/// 适用于开发和测试环境，生产环境建议使用 S3 后端。
pub struct LocalBackend {
    /// 平台标识
    platform: String,
    /// OpenDAL Operator
    operator: Operator,
    /// 存储能力
    capabilities: StorageCapabilities,
}

impl LocalBackend {
    /// 创建本地文件系统后端
    ///
    /// 根据配置初始化 OpenDAL Fs 服务。
    ///
    /// # Arguments
    ///
    /// * `config` - 存储实例配置
    ///
    /// # Returns
    ///
    /// 成功时返回 `LocalBackend` 实例。
    ///
    /// # Errors
    ///
    /// 当配置无效或 OpenDAL 初始化失败时返回 `ConfigError`。
    pub fn new(config: &StorageInstanceConfig) -> Result<Self> {
        let root = config.get_root_path();

        let mut builder = Fs::default();
        builder = builder.root(&root);

        let operator = Operator::new(builder)
            .map_err(|e| Error::ConfigError(format!("创建本地存储后端失败: {}", e)))?
            .finish();

        let capabilities = StorageCapabilities {
            read: true,
            write: true,
            delete: true,
            list: true,
            copy: false,
            presign: false,
            presign_read: false,
            presign_write: false,
            create_dir: true,
            rename: true,
            multipart: false,
        };

        Ok(Self {
            platform: config.platform.clone(),
            operator,
            capabilities,
        })
    }
}

#[async_trait]
impl StorageBackend for LocalBackend {
    fn backend_type(&self) -> &str {
        "local"
    }

    fn platform(&self) -> &str {
        &self.platform
    }

    fn capabilities(&self) -> &StorageCapabilities {
        &self.capabilities
    }

    async fn write(&self, path: &str, data: Bytes, opts: WriteOptions) -> Result<WriteResult> {
        let mut writer = self.operator.writer_with(path);
        if let Some(ct) = opts.content_type {
            writer = writer.content_type(&ct);
        }
        if let Some(cd) = opts.content_disposition {
            writer = writer.content_disposition(&cd);
        }
        let mut w = writer.await?;
        w.write(data).await?;
        w.close().await?;

        let meta = self.operator.stat(path).await?;
        Ok(WriteResult {
            etag: meta.etag().map(|s| s.to_string()),
            content_length: meta.content_length(),
        })
    }

    async fn read(&self, path: &str) -> Result<Bytes> {
        let data = self.operator.read(path).await?;
        Ok(data.to_bytes())
    }

    async fn reader(&self, path: &str) -> Result<opendal::Reader> {
        let reader = self.operator.reader(path).await?;
        Ok(reader)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.operator.delete(path).await?;
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<ObjectMetadata> {
        let meta = self.operator.stat(path).await?;
        Ok(ObjectMetadata {
            path: path.to_string(),
            content_length: meta.content_length(),
            content_type: meta.content_type().map(|s| s.to_string()),
            etag: meta.etag().map(|s| s.to_string()),
            last_modified: meta.last_modified().map(|dt| {
                let ts = dt.into_inner();
                chrono::DateTime::from_timestamp(ts.as_second(), ts.subsec_nanosecond() as u32)
                    .map(|dt| dt.naive_utc())
                    .unwrap_or_default()
            }),
            user_metadata: meta.user_metadata().cloned(),
        })
    }

    async fn list(&self, prefix: &str, opts: ListOptions) -> Result<Vec<ListEntry>> {
        let mut lister = self.operator.list_with(prefix).recursive(opts.recursive);
        if let Some(limit) = opts.limit {
            lister = lister.limit(limit);
        }
        let entries = lister.await?;

        let mut result = Vec::new();
        for entry in entries {
            let meta = entry.metadata();
            result.push(ListEntry {
                path: entry.path().to_string(),
                is_dir: meta.mode() == EntryMode::DIR,
                content_length: meta.content_length(),
                content_type: meta.content_type().map(|s| s.to_string()),
                etag: meta.etag().map(|s| s.to_string()),
                last_modified: meta.last_modified().map(|dt| {
                    let ts = dt.into_inner();
                    chrono::DateTime::from_timestamp(ts.as_second(), ts.subsec_nanosecond() as u32)
                        .map(|dt| dt.naive_utc())
                        .unwrap_or_default()
                }),
            });
        }
        Ok(result)
    }

    async fn copy(&self, _from: &str, _to: &str) -> Result<()> {
        Err(Error::UnsupportedError(
            "本地存储不支持 copy 操作".to_string(),
        ))
    }

    async fn presign_read(&self, _path: &str, _expires: Duration) -> Result<String> {
        Err(Error::UnsupportedError(
            "本地存储不支持预签名 URL".to_string(),
        ))
    }

    async fn presign_write(&self, _path: &str, _expires: Duration) -> Result<String> {
        Err(Error::UnsupportedError(
            "本地存储不支持预签名 URL".to_string(),
        ))
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        self.operator.create_dir(path).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.operator.exists(path).await?)
    }
}
