//! 缩略图生成
//!
//! 提供 `DefaultStorageService::generate_thumbnail` 方法，使用 `image` crate 解码原始图片
//! 并生成最大 200x200 的 JPEG 缩略图（保持宽高比）。

use std::io::Cursor;

use crate::mime_detect::is_thumbnail_supported;
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 生成图片缩略图
    ///
    /// 使用 `image` crate 解码原始图片并生成最大 200x200 的缩略图（保持宽高比），
    /// 输出 JPEG 格式。
    ///
    /// # Arguments
    ///
    /// * `data` - 原始图片二进制数据
    /// * `content_type` - 原始图片 MIME 类型
    ///
    /// # Returns
    ///
    /// 成功返回 `Ok(Some(ThumbnailData))`，非图片或生成失败返回 `Ok(None)`（不报错）。
    pub(super) fn generate_thumbnail(data: &[u8], content_type: &str) -> Option<ThumbnailData> {
        if !is_thumbnail_supported(content_type) {
            return None;
        }

        let img = match image::load_from_memory(data) {
            Ok(img) => img,
            Err(e) => {
                tracing::warn!("缩略图生成失败（解码错误）: {}", e);
                return None;
            }
        };

        let thumb = img.thumbnail(200, 200);

        let mut buffer = Vec::new();
        {
            let mut cursor = Cursor::new(&mut buffer);
            if let Err(e) = thumb.write_to(&mut cursor, image::ImageFormat::Jpeg) {
                tracing::warn!("缩略图编码失败: {}", e);
                return None;
            }
        }

        Some(ThumbnailData {
            data: bytes::Bytes::from(buffer),
            content_type: "image/jpeg".to_string(),
        })
    }
}
