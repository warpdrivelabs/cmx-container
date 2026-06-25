//! ZIP 打包工具。
//!
//! 将插件安装目录下的数据子目录打包为 ZIP 字节，用于发送到外部服务中心。
//! 底层复用 `cmx_utils::zip::ZipCompressor` 的 `compress_dir_to_memory` 方法。

use std::path::Path;
use cmx_utils::zip::ZipCompressor;
use crate::error::{PluginError, PluginResult};

/// 默认 ZIP 压缩级别。
const DEFAULT_COMPRESSION_LEVEL: u32 = 6;

/// 检查目录是否包含至少一个 `.json` 或 `.csv` 文件（递归检查子目录）。
///
/// 扩展名匹配大小写不敏感，例如 `.JSON`、`.Csv` 同样视为有效。
///
/// # Arguments
///
/// * `dir` - 待检查的目录路径。
///
/// # Returns
///
/// 目录中存在至少一个扩展名为 `json` 或 `csv`（忽略大小写）的文件时返回 `true`，
/// 否则（目录不存在、为空或仅含其他类型文件）返回 `false`。
pub fn has_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path() != dir && e.file_type().is_file())
        .any(|e| {
            e.path()
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(|ext| matches!(ext.to_lowercase().as_str(), "json" | "csv"))
                .unwrap_or(false)
        })
}

/// 将指定目录下的所有文件递归打包为 ZIP 字节。
///
/// 内部调用 `ZipCompressor::compress_dir_to_memory`，保留相对路径结构。
///
/// # Arguments
///
/// * `dir` - 数据目录路径（如 `menudata/`）。
///
/// # Returns
///
/// 成功时返回 ZIP 字节数组。
///
/// # Errors
///
/// 当目录不存在、为空或 IO 操作失败时返回 `PluginError::CenterData`。
pub fn pack_directory_to_zip(dir: &Path) -> PluginResult<Vec<u8>> {
    ZipCompressor::compress_dir_to_memory(dir, DEFAULT_COMPRESSION_LEVEL)
        .map_err(|e| PluginError::CenterData(format!("ZIP 打包失败: {}", e)))
}
