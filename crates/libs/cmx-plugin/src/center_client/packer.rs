//! ZIP 打包工具。
//!
//! 将插件安装目录下的数据子目录打包为 ZIP 字节，用于发送到外部服务中心。
//! 底层复用 `cmx_utils::zip::ZipCompressor` 的 `compress_dir_to_memory` 方法。
//!
//! 另提供 `pack_definitions_to_zip`，把结构化定义列表序列化为 JSON 文件后打包，
//! 供 Remote 定义导入器经 gRPC 传输结构化数据(非目录)。

use crate::error::{PluginError, PluginResult};
use cmx_utils::zip::ZipCompressor;
use serde::Serialize;
use std::io::Write;
use std::path::Path;

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

/// 把结构化定义列表序列化为 JSON 文件后打包为 ZIP 字节。
///
/// 供 Remote 定义导入器复用:把 `&[FormDefinition]` / `&[MenuDefinition]` 等
/// 序列化为 `{prefix}_0.json`、`{prefix}_1.json` ... 文件,再压缩为 ZIP,
/// 经 gRPC(`ResourceDataClient::import_resource_data`)发送到远程中心。
/// 远程接收端解压 ZIP → 解析 JSON → 调用 Local 实现入库。
///
/// # Arguments
///
/// * `definitions` - 结构化定义列表(需实现 `Serialize`)
/// * `prefix` - 文件名前缀(如 `"form"`、`"menu"`),生成 `form_0.json` 等
///
/// # Returns
///
/// 成功时返回 ZIP 字节数组。
///
/// # Errors
///
/// 序列化或压缩失败时返回 `PluginError::CenterData`。
pub fn pack_definitions_to_zip<T: Serialize>(
    definitions: &[T],
    prefix: &str,
) -> PluginResult<Vec<u8>> {
    // 写入内存中的 ZIP(不落盘,避免临时文件管理)
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(DEFAULT_COMPRESSION_LEVEL as i64));

    for (i, def) in definitions.iter().enumerate() {
        let json = serde_json::to_string_pretty(def).map_err(|e| {
            PluginError::CenterData(format!("序列化定义 {prefix}_{i} 失败: {e}"))
        })?;
        let file_name = format!("{prefix}_{i}.json");
        zip.start_file(&file_name, options)
            .map_err(|e| PluginError::CenterData(format!("ZIP 写入 {file_name} 失败: {e}")))?;
        zip.write_all(json.as_bytes())
            .map_err(|e| PluginError::CenterData(format!("ZIP 写入 {file_name} 内容失败: {e}")))?;
    }

    let cursor = zip
        .finish()
        .map_err(|e| PluginError::CenterData(format!("ZIP 完成失败: {e}")))?;
    Ok(cursor.into_inner())
}

/// 把结构化定义列表打包为「JSON 数组」形式的 ZIP(单文件 `definitions.json`)。
///
/// 与 [`pack_definitions_to_zip`] 的区别:多个定义合并为一个 JSON 数组文件,
/// 适合接收端期望整体解析的场景(如 `{"tables": [...]}` / `{"permissions": [...]}`)。
///
/// # Arguments
///
/// * `payload` - 任意可序列化的载荷(通常是 `json!({...})`)
/// * `file_name` - ZIP 内文件名(如 `module_tables.json`)
pub fn pack_payload_to_zip<T: Serialize>(payload: &T, file_name: &str) -> PluginResult<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(DEFAULT_COMPRESSION_LEVEL as i64));

    let json = serde_json::to_string_pretty(payload)
        .map_err(|e| PluginError::CenterData(format!("序列化载荷失败: {e}")))?;
    zip.start_file(file_name, options)
        .map_err(|e| PluginError::CenterData(format!("ZIP 写入 {file_name} 失败: {e}")))?;
    zip.write_all(json.as_bytes())
        .map_err(|e| PluginError::CenterData(format!("ZIP 写入 {file_name} 内容失败: {e}")))?;

    let cursor = zip
        .finish()
        .map_err(|e| PluginError::CenterData(format!("ZIP 完成失败: {e}")))?;
    Ok(cursor.into_inner())
}

