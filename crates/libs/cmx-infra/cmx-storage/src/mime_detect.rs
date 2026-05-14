//! MIME Type 检测模块
//!
//! 提供基于文件魔数和扩展名的 MIME Type 检测功能。
//! 支持三级检测策略：手动指定 > 文件魔数 > 扩展名推断。

/// 检测文件的 MIME Type
///
/// 按照优先级检测并返回文件的 MIME 类型。
///
/// # Arguments
///
/// * `data` - 文件的二进制数据
/// * `filename` - 文件名（用于扩展名推断）
/// * `manual_content_type` - 手动指定的 MIME 类型（最高优先级）
///
/// # Returns
///
/// 检测到的 MIME 类型字符串，按优先级依次为：
/// 1. 手动指定的 `manual_content_type`（若非空）
/// 2. 文件魔数检测结果（`infer` 库）
/// 3. 扩展名推断结果（`mime_guess` 库）
/// 4. 默认值 `application/octet-stream`
///
/// # Examples
///
/// ```
/// use cmx_storage::mime_detect::detect_mime;
///
/// let data = b"GIF89a...";
/// let mime = detect_mime(data, Some("image.gif"), None);
/// assert_eq!(mime, "image/gif");
/// ```
pub fn detect_mime(
    data: &[u8],
    filename: Option<&str>,
    manual_content_type: Option<&str>,
) -> String {
    if let Some(ct) = manual_content_type
        && !ct.is_empty()
    {
        return ct.to_string();
    }

    if let Some(result) = infer::get(data) {
        return result.mime_type().to_string();
    }

    if let Some(name) = filename
        && let Some(mime) = mime_guess::from_path(name).first()
    {
        return mime.to_string();
    }

    "application/octet-stream".to_string()
}

/// 从文件扩展名推断 MIME Type
///
/// 根据给定的扩展名（不含前导点号）推断对应的 MIME 类型。
///
/// # Arguments
///
/// * `extension` - 文件扩展名（不含点号），如 `"jpg"`、`"png"`
///
/// # Returns
///
/// 推断出的 MIME 类型字符串，若无法识别则返回 `application/octet-stream`。
///
/// # Examples
///
/// ```
/// use cmx_storage::mime_detect::mime_from_extension;
///
/// assert_eq!(mime_from_extension("jpg"), "image/jpeg");
/// assert_eq!(mime_from_extension("unknown"), "application/octet-stream");
/// ```
pub fn mime_from_extension(extension: &str) -> String {
    mime_guess::from_ext(extension)
        .first()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}
