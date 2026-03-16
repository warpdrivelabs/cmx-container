//! # ZIP 压缩/解压模块
//!
//! 提供 ZIP 文件的压缩和解压功能，支持文件和目录的压缩，以及解压到指定目录。
//!
//! ## 功能特性
//!
//! - 压缩单个文件为 ZIP
//! - 压缩目录为 ZIP（递归）
//! - 解压 ZIP 文件到指定目录
//! - 支持指定压缩级别
//! - 保留文件时间戳
//!
//! ## 快速开始
//!
//! ```rust,no_run
//! use cmx_utils::zip::{ZipCompressor, ZipExtractor};
//! use std::path::Path;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 压缩目录
//!     ZipCompressor::compress_dir(
//!         Path::new("data"),
//!         Path::new("output.zip"),
//!         6,
//!     )?;
//!
//!     // 解压到目录
//!     ZipExtractor::extract(
//!         Path::new("output.zip"),
//!         Path::new("extracted"),
//!     )?;
//!
//!     Ok(())
//! }
//! ```

mod compressor;
mod error;
mod extractor;

pub use compressor::ZipCompressor;
pub use error::ZipError;
pub use extractor::ZipExtractor;

pub type ZipResult<T> = Result<T, ZipError>;
