//! # ZIP 解压器模块
//!
//! 提供 ZIP 文件的解压功能。

use crate::zip::{ZipError, ZipResult};
use fs_err::File;
use std::fs::DirBuilder;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/**
 * ZIP 解压器
 *
 * 用于解压 ZIP 文件到指定目录。
 */
pub struct ZipExtractor;

impl ZipExtractor {
    /**
     * 解压 ZIP 文件到指定目录
     *
     * # 参数
     * - `source`: 源 ZIP 文件路径
     * - `output_dir`: 输出目录路径
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     *
     * # 示例
     * ```rust,no_run
     * use cmx_utils::zip::ZipExtractor;
     * use std::path::Path;
     *
     * fn main() -> Result<(), Box<dyn std::error::Error>> {
     *     ZipExtractor::extract(
     *         Path::new("input.zip"),
     *         Path::new("output_dir"),
     *     )?;
     *     Ok(())
     * }
     * ```
     */
    pub fn extract(source: impl AsRef<Path>, output_dir: impl AsRef<Path>) -> ZipResult<()> {
        let source = source.as_ref();
        let output_dir = output_dir.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => output_dir.join(path),
                None => continue,
            };

            if file.name().ends_with('/') {
                DirBuilder::new().recursive(true).create(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent()
                    && !parent.exists()
                {
                    DirBuilder::new().recursive(true).create(parent)?;
                }

                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }

            // #[cfg(unix)]
            // {
            //     use std::os::unix::fs::PermissionsExt;
            //     if let Some(mode) = file.unix_mode() {
            //         fs_err::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            //     }
            // }
        }

        Ok(())
    }

    /**
     * 解压 ZIP 文件并返回解压后的文件列表
     *
     * # 参数
     * - `source`: 源 ZIP 文件路径
     * - `output_dir`: 输出目录路径
     *
     * # 返回
     * - `ZipResult<Vec<PathBuf>>`: 解压后的文件路径列表
     */
    pub fn extract_with_list(
        source: impl AsRef<Path>,
        output_dir: impl AsRef<Path>,
    ) -> ZipResult<Vec<PathBuf>> {
        let source = source.as_ref();
        let output_dir = output_dir.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        let mut extracted_files = Vec::new();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => output_dir.join(path),
                None => continue,
            };

            if file.name().ends_with('/') {
                DirBuilder::new().recursive(true).create(&outpath)?;
            } else {
                if let Some(parent) = outpath.parent()
                    && !parent.exists()
                {
                    DirBuilder::new().recursive(true).create(parent)?;
                }

                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
                extracted_files.push(outpath);
            }

            // #[cfg(unix)]
            // {
            //     use std::os::unix::fs::PermissionsExt;
            //     if let Some(mode) = file.unix_mode() {
            //         fs_err::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            //     }
            // }
        }

        Ok(extracted_files)
    }

    /**
     * 解压 ZIP 文件中的指定文件
     *
     * # 参数
     * - `source`: 源 ZIP 文件路径
     * - `file_name`: 要解压的文件名
     * - `output_path`: 输出文件路径
     *
     * # 返回
     * - `ZipResult<()>`: 成功返回 Ok(())
     */
    pub fn extract_file(
        source: impl AsRef<Path>,
        file_name: &str,
        output_path: impl AsRef<Path>,
    ) -> ZipResult<()> {
        let source = source.as_ref();
        let output_path = output_path.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        let mut zip_file = archive.by_name(file_name)?;

        if let Some(parent) = output_path.parent()
            && !parent.exists()
        {
            DirBuilder::new().recursive(true).create(parent)?;
        }

        let mut outfile = File::create(output_path)?;
        std::io::copy(&mut zip_file, &mut outfile)?;

        Ok(())
    }

    /**
     * 解压 ZIP 文件到内存并返回文件内容
     *
     * # 参数
     * - `source`: 源 ZIP 文件路径
     * - `file_name`: 要读取的文件名
     *
     * # 返回
     * - `ZipResult<Vec<u8>>`: 文件内容字节数组
     */
    pub fn extract_to_memory(source: impl AsRef<Path>, file_name: &str) -> ZipResult<Vec<u8>> {
        let source = source.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        let mut zip_file = archive.by_name(file_name)?;
        let mut buffer = Vec::new();
        zip_file.read_to_end(&mut buffer)?;

        Ok(buffer)
    }

    /**
     * 获取 ZIP 文件中的文件列表
     *
     * # 参数
     * - `source`: 源 ZIP 文件路径
     *
     * # 返回
     * - `ZipResult<Vec<String>>`: ZIP 内的文件列表
     */
    pub fn list_files(source: impl AsRef<Path>) -> ZipResult<Vec<String>> {
        let source = source.as_ref();

        if !source.exists() {
            return Err(ZipError::FileNotFound(source.display().to_string()));
        }

        if !source.is_file() {
            return Err(ZipError::NotFile(source.display().to_string()));
        }

        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;

        let mut files = Vec::new();
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            files.push(file.name().to_string());
        }

        Ok(files)
    }
}
