//! 文件路径生成策略模块
//!
//! 提供自动文件路径生成功能，基于日期目录和 UUID 文件名
//! 生成唯一的存储路径，避免文件名冲突。

use chrono::Local;
use uuid::Uuid;

use crate::config::StorageType;

/// 生成文件存储路径
///
/// 根据基础路径、对象类型、扩展名和存储类型生成唯一的存储路径。
///
/// # Arguments
///
/// * `base_path` - 存储路径的基础前缀
/// * `object_type` - 对象类型，用于分组管理（如 `"avatar"`、`"document"`）
/// * `extension` - 文件扩展名（可带或不带点号）
/// * `storage_type` - 存储类型，决定日期目录格式
///
/// # Returns
///
/// 返回元组 `(完整路径, 文件名)`：
/// - Local: `{base_path}/{object_type}/{yyyyMM}/{uuid}.{ext}`
/// - S3: `{base_path}/{object_type}/{yyyyMMdd}/{uuid}.{ext}`
/// - 文件名格式：`{uuid}.{ext}`
///
/// # Examples
///
/// ```
/// use cmx_storage::path_gen::generate_storage_path;
/// use cmx_storage::config::StorageType;
///
/// let (path, filename) = generate_storage_path("s3/", Some("avatar"), "jpg", &StorageType::S3);
/// assert!(path.starts_with("s3/avatar/20"));
/// assert!(filename.ends_with(".jpg"));
/// ```
pub fn generate_storage_path(
    base_path: &str,
    object_type: Option<&str>,
    extension: &str,
    storage_type: &StorageType,
) -> (String, String) {
    let now = Local::now();
    let date_path = match storage_type {
        StorageType::Local => now.format("%Y%m").to_string(),
        StorageType::S3 => now.format("%Y%m%d").to_string(),
    };
    let file_id = Uuid::new_v4().to_string();

    let object_type_path = object_type.unwrap_or("default");

    let ext = if extension.starts_with('.') {
        extension.to_string()
    } else if extension.is_empty() {
        String::new()
    } else {
        format!(".{}", extension)
    };

    let filename = format!("{}{}", file_id, ext);
    let path = if base_path.is_empty() {
        format!("{}/{}/{}", object_type_path, date_path, filename)
    } else {
        let base = if base_path.ends_with('/') {
            base_path.to_string()
        } else {
            format!("{}/", base_path)
        };
        format!("{}{}/{}/{}", base, object_type_path, date_path, filename)
    };

    (path, filename)
}

/// 从文件名中提取扩展名
///
/// 返回最后一个点号之后的部分（不含点号）。
///
/// # Arguments
///
/// * `filename` - 原始文件名
///
/// # Returns
///
/// 文件扩展名（不含点号），若无扩展名则返回空字符串。
///
/// # Examples
///
/// ```
/// use cmx_storage::path_gen::extract_extension;
///
/// assert_eq!(extract_extension("photo.jpg"), "jpg");
/// assert_eq!(extract_extension("document.pdf"), "pdf");
/// assert_eq!(extract_extension("noext"), "");
/// assert_eq!(extract_extension("multi.tar.gz"), "gz");
/// ```
pub fn extract_extension(filename: &str) -> String {
    if let Some(pos) = filename.rfind('.') {
        let ext = &filename[pos + 1..];
        ext.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageType;

    #[test]
    fn test_extract_extension() {
        assert_eq!(extract_extension("photo.jpg"), "jpg");
        assert_eq!(extract_extension("document.pdf"), "pdf");
        assert_eq!(extract_extension("noext"), "");
        assert_eq!(extract_extension("multi.tar.gz"), "gz");
    }

    #[test]
    fn test_generate_storage_path_s3() {
        let (path, filename) =
            generate_storage_path("s3/", Some("avatar"), "jpg", &StorageType::S3);
        assert!(path.starts_with("s3/avatar/20"));
        assert!(filename.ends_with(".jpg"));
        assert!(filename.len() > 36);
    }

    #[test]
    fn test_generate_storage_path_local() {
        let (path, filename) =
            generate_storage_path("uploads/", Some("avatar"), "jpg", &StorageType::Local);
        assert!(path.starts_with("uploads/avatar/202"));
        assert!(filename.ends_with(".jpg"));
    }
}
