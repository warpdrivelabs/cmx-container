//! Amazon S3 存储后端实现
//!
//! 基于 OpenDAL S3 服务提供 Amazon S3 及兼容服务的存储能力。
//! 支持 S3、MinIO、腾讯云 COS、阿里云 OSS 等 S3 兼容存储服务。

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use opendal::services::S3;
use opendal::{EntryMode, Operator};

use crate::config::StorageInstanceConfig;
use crate::error::{Error, Result};
use crate::types::*;

use super::StorageBackend;

/// Amazon S3 存储后端
///
/// 基于 OpenDAL S3 服务实现，提供完整的对象存储操作能力。
/// 支持所有标准的存储操作，包括读取、写入、删除、列举、复制和预签名 URL。
pub struct S3Backend {
    /// 平台标识
    platform: String,
    /// OpenDAL Operator
    operator: Operator,
    /// 存储能力
    capabilities: StorageCapabilities,
}

impl S3Backend {
    /// 创建 S3 存储后端
    ///
    /// 根据配置初始化 OpenDAL S3 服务。
    ///
    /// # Arguments
    ///
    /// * `config` - 存储实例配置
    ///
    /// # Returns
    ///
    /// 成功时返回 `S3Backend` 实例。
    ///
    /// # Errors
    ///
    /// 当配置无效或 OpenDAL 初始化失败时返回 `ConfigError`。
    pub fn new(config: &StorageInstanceConfig) -> Result<Self> {
        let mut builder = S3::default();

        if let Some(ref bucket) = config.bucket_name {
            builder = builder.bucket(bucket);
        }
        if let Some(ref region) = config.region {
            builder = builder.region(region);
        }
        if let Some(ref endpoint) = config.endpoint {
            builder = builder.endpoint(endpoint);
        }
        if let Some(ref ak) = config.access_key {
            builder = builder.access_key_id(ak);
        }
        if let Some(ref sk) = config.secret_key {
            builder = builder.secret_access_key(sk);
        }

        let base_path = config.base_path.trim_end_matches('/');
        if !base_path.is_empty() {
            builder = builder.root(&format!("{}/", base_path));
        }

        let operator = Operator::new(builder)
            .map_err(|e| Error::ConfigError(format!("创建S3存储后端失败: {}", e)))?
            .finish();

        let capabilities = StorageCapabilities {
            read: true,
            write: true,
            delete: true,
            list: true,
            copy: true,
            presign: true,
            presign_read: true,
            presign_write: true,
            create_dir: true,
            rename: true,
            multipart: true,
        };

        Ok(Self {
            platform: config.platform.clone(),
            operator,
            capabilities,
        })
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    fn backend_type(&self) -> &str {
        "s3"
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
        if let Some(cc) = opts.cache_control {
            writer = writer.cache_control(&cc);
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

    async fn copy(&self, from: &str, to: &str) -> Result<()> {
        self.operator.copy(from, to).await?;
        Ok(())
    }

    async fn presign_read(&self, path: &str, expires: Duration) -> Result<String> {
        let req = self.operator.presign_read(path, expires).await?;
        Ok(req.uri().to_string())
    }

    async fn presign_write(&self, path: &str, expires: Duration) -> Result<String> {
        let req = self.operator.presign_write(path, expires).await?;
        Ok(req.uri().to_string())
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        self.operator.create_dir(path).await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.operator.exists(path).await?)
    }
}
