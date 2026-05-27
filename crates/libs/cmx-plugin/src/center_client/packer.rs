//! ZIP 打包工具。
//!
//! 将插件安装目录下的数据子目录打包为 ZIP 字节，用于发送到外部服务中心。
//! 底层复用 `cmx_utils::zip::ZipCompressor` 的 `compress_dir_to_memory` 方法。

use std::path::Path;
use cmx_utils::zip::ZipCompressor;
use crate::error::{PluginError, PluginResult};

/// 默认 ZIP 压缩级别。
const DEFAULT_COMPRESSION_LEVEL: u32 = 6;

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
