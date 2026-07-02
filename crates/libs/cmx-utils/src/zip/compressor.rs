//! # ZIP 压缩器模块
//!
//! 提供文件和目录的 ZIP 压缩功能。

use crate::zip::{ZipError, ZipResult};
use fs_err::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// ZIP 压缩器
///
/// 用于将文件和目录压缩为 ZIP 格式。
pub struct ZipCompressor;

impl ZipCompressor {
    /**
     * 压缩单个文件为 ZIP
     *
     * # 参数
     * - `source`: 源文件路径
     * - `output`: 输出 ZIP 文件路径
     * - `compression_level`: 压缩级别 (0-9)，0 为不压缩，9 为最大压缩
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     *
     * # 示例
     * ```rust,no_run
     * use cmx_utils::zip::ZipCompressor;
     * use std::path::Path;
     *
     * fn main() -> Result<(), Box<dyn std::error::Error>> {
     *     ZipCompressor::compress_file(
     *         Path::new("data.txt"),
     *         Path::new("output.zip"),
     *         6,
     *     )?;
     *     Ok(())
     * }
     * ```
     */
    pub fn compress_file(
        source: impl AsRef<Path>,
        output: impl AsRef<Path>,
        compression_level: u32,
    ) -> ZipResult<()> {
        let source = source.as_ref();
        let output = output.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::create(output)?;
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression_level as i64));

        let file_name = source
            .file_name()
            .ok_or_else(|| ZipError::Path("无法获取文件名".to_string()))?
            .to_string_lossy()
            .to_string();

        zip.start_file(file_name, options)?;

        let mut source_file = File::open(source)?;
        let mut buffer = Vec::new();
        source_file.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;

        zip.finish()?;

        Ok(())
    }

    /**
     * 压缩目录为 ZIP（递归）
     *
     * # 参数
     * - `source`: 源目录路径
     * - `output`: 输出 ZIP 文件路径
     * - `compression_level`: 压缩级别 (0-9)，0 为不压缩，9 为最大压缩
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     *
     * # 示例
     * ```rust,no_run
     * use cmx_utils::zip::ZipCompressor;
     * use std::path::Path;
     *
     * fn main() -> Result<(), Box<dyn std::error::Error>> {
     *     ZipCompressor::compress_dir(
     *         Path::new("data"),
     *         Path::new("output.zip"),
     *         6,
     *     )?;
     *     Ok(())
     * }
     * ```
     */
    pub fn compress_dir(
        source: impl AsRef<Path>,
        output: impl AsRef<Path>,
        compression_level: u32,
    ) -> ZipResult<()> {
        let source = source.as_ref();
        let output = output.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_dir() {
            return Err(ZipError::NotDirectory(source.display().to_string()));
        }

        let file = File::create(output)?;
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression_level as i64));

        let base_path = source.to_path_buf();
        let mut has_files = false;

        for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            if path == source {
                continue;
            }

            has_files = true;

            let relative_path = path
                .strip_prefix(&base_path)
                .map_err(|_| ZipError::Path("路径解析失败".to_string()))?;

            let relative_str = relative_path.to_string_lossy().replace('\\', "/");

            if path.is_dir() {
                let dir_name = format!("{}/", relative_str);
                zip.add_directory(&dir_name, options)?;
            } else {
                zip.start_file(&relative_str, options)?;

                let mut source_file = File::open(path)?;
                let mut buffer = Vec::new();
                source_file.read_to_end(&mut buffer)?;
                zip.write_all(&buffer)?;
            }
        }

        if !has_files {
            return Err(ZipError::EmptySource);
        }

        zip.finish()?;

        Ok(())
    }

    /// 压缩目录到内存中的 ZIP 字节。
    ///
    /// 与 `compress_dir` 功能相同，但输出到 `Vec<u8>` 而非文件系统。
    /// 适用于需要将 ZIP 数据直接传递给其他组件的场景（如 HTTP 上传）。
    ///
    /// # Arguments
    ///
    /// * `source` - 源目录路径，必须存在且为目录。
    /// * `compression_level` - 压缩级别 (0-9)，0 为不压缩，9 为最大压缩。
    ///
    /// # Returns
    ///
    /// 成功时返回包含 ZIP 数据的字节向量。
    ///
    /// # Errors
    ///
    /// 当源目录不存在、不是目录、为空或 IO 操作失败时返回错误。
    pub fn compress_dir_to_memory(
        source: impl AsRef<Path>,
        compression_level: u32,
    ) -> ZipResult<Vec<u8>> {
        let source = source.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_dir() {
            return Err(ZipError::NotDirectory(source.display().to_string()));
        }

        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(buf);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression_level as i64));

        let base_path = source.to_path_buf();
        let mut has_files = false;

        for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            if path == source {
                continue;
            }

            has_files = true;

            let relative_path = path
                .strip_prefix(&base_path)
                .map_err(|_| ZipError::Path("路径解析失败".to_string()))?;

            let relative_str = relative_path.to_string_lossy().replace('\\', "/");

            if path.is_dir() {
                let dir_name = format!("{}/", relative_str);
                zip.add_directory(&dir_name, options)?;
            } else {
                zip.start_file(&relative_str, options)?;

                let mut source_file = File::open(path)?;
                let mut buffer = Vec::new();
                source_file.read_to_end(&mut buffer)?;
                zip.write_all(&buffer)?;
            }
        }

        if !has_files {
            return Err(ZipError::EmptySource);
        }

        let buf = zip.finish()?;
        Ok(buf.into_inner())
    }

    /**
     * 压缩多个文件到同一个 ZIP（不包含目录结构）
     *
     * # 参数
     * - `sources`: 源文件路径列表
     * - `output`: 输出 ZIP 文件路径
     * - `compression_level`: 压缩级别 (0-9)，0 为不压缩，9 为最大压缩
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     *
     * # 示例
     * ```rust,no_run
     * use cmx_utils::zip::ZipCompressor;
     * use std::path::Path;
     *
     * fn main() -> Result<(), Box<dyn std::error::Error>> {
     *     ZipCompressor::compress_files(
     *         vec![Path::new("file1.txt"), Path::new("file2.txt")],
     *         Path::new("output.zip"),
     *         6,
     *     )?;
     *     Ok(())
     * }
     * ```
     */
    pub fn compress_files(
        sources: Vec<impl AsRef<Path>>,
        output: impl AsRef<Path>,
        compression_level: u32,
    ) -> ZipResult<()> {
        let output = output.as_ref();

        if sources.is_empty() {
            return Err(ZipError::EmptySource);
        }

        let file = File::create(output)?;
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression_level as i64));

        for source in sources {
            let source = source.as_ref();

            if !source.exists() {
                return Err(ZipError::FileNotFound(source.display().to_string()));
            }

            if !source.is_file() {
                return Err(ZipError::NotFile(source.display().to_string()));
            }

            let file_name = source
                .file_name()
                .ok_or_else(|| ZipError::Path("无法获取文件名".to_string()))?
                .to_string_lossy()
                .to_string();

            zip.start_file(&file_name, options)?;

            let mut source_file = File::open(source)?;
            let mut buffer = Vec::new();
            source_file.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
        }

        zip.finish()?;

        Ok(())
    }

    /**
     * 压缩文件并添加到已存在的 ZIP 文件中
     *
     * # 参数
     * - `source`: 源文件路径
     * - `output`: 输出 ZIP 文件路径（可与源相同）
     * - `entry_name`: ZIP 内的文件名
     * - `compression_level`: 压缩级别 (0-9)
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     */
    pub fn append_file(
        source: impl AsRef<Path>,
        output: impl AsRef<Path>,
        entry_name: &str,
        compression_level: u32,
    ) -> ZipResult<()> {
        let source = source.as_ref();
        let output = output.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::create(output)?;
        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(compression_level as i64));

        zip.start_file(entry_name, options)?;

        let mut source_file = File::open(source)?;
        let mut buffer = Vec::new();
        source_file.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;

        zip.finish()?;

        Ok(())
    }
}
