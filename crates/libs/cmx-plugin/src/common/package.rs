//! 包处理工具模块
//!
//! 提供插件包获取、解压、复制等通用操作。
//!
//! # 功能概述
//!
//! - 从不同来源获取插件包（本地、远程、注册表）
//! - 解压 ZIP 格式的插件包
//! - 查找插件根目录
//! - 复制插件文件到目标目录

use std::path::{Path, PathBuf};

use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::fetcher::local::LocalFetcher;
use crate::fetcher::remote::RemoteFetcher;
use crate::infrastructure::storage::file::FileStorage;

/// 包处理工具依赖
///
/// 包含包处理工具运行所需的所有依赖项。
#[derive(Clone)]
pub struct PackageUtilsDeps {
    /// 插件根目录
    ///
    /// 用于存储已安装插件的根目录路径。
    /// 本地插件获取器会相对于此路径查找插件包。
    pub plugin_root: PathBuf,

    /// 临时目录
    ///
    /// 用于存储临时文件的目录路径。
    /// 远程插件下载、ZIP 解压等操作会使用此目录。
    pub temp_root: PathBuf,

    /// 文件存储
    ///
    /// 可选的文件存储实例，用于执行文件操作。
    /// 如果未提供，将使用备用方法执行文件操作。
    pub storage: Option<std::sync::Arc<FileStorage>>,
}

/// 包处理工具
///
/// 提供插件包获取、解压、复制等操作的统一接口。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::common::{PackageUtils, PackageUtilsDeps};
/// use std::path::PathBuf;
///
/// let utils = PackageUtils::new(PackageUtilsDeps {
///     plugin_root: PathBuf::from("./plugins"),
///     temp_root: PathBuf::from("./temp"),
///     storage: None,
/// });
/// ```
/// 包处理工具
#[derive(Clone)]
pub struct PackageUtils {
    deps: PackageUtilsDeps,
}

impl PackageUtils {
    /// 创建新的包处理工具
    ///
    /// # 参数
    ///
    /// * `deps` - 包处理工具的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的包处理工具实例
    pub fn new(deps: PackageUtilsDeps) -> Self {
        Self { deps }
    }

    /// 获取插件包
    ///
    /// 根据插件来源类型获取插件包路径。
    ///
    /// # 参数
    ///
    /// * `source` - 插件来源，支持以下类型：
    ///   - `Local { path }`: 本地文件路径，可以是 ZIP 文件或目录
    ///   - `Remote { url, checksum }`: 远程 URL，可选校验和
    ///   - `Registry { registry_url, package_name }`: 插件注册表
    /// * `version_constraint` - 版本约束，仅对注册表来源有效。
    ///   支持语义化版本约束，如 "^1.0.0"、">=2.0.0"
    /// * `error_context` - 错误上下文信息，用于错误消息中标识操作来源
    ///
    /// # 返回值
    ///
    /// 返回插件包的本地路径：
    /// - 本地来源：返回原始路径
    /// - 远程来源：返回下载后的临时文件路径
    /// - 注册表来源：返回下载后的临时文件路径
    ///
    /// # 错误
    ///
    /// - 本地插件获取失败
    /// - 远程下载失败
    /// - 注册表查询或下载失败
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use cmx_plugin::domain::plugin::PluginSource;
    ///
    /// # async fn example(utils: &cmx_plugin::common::PackageUtils) -> Result<(), Box<dyn std::error::Error>> {
    /// let source = PluginSource::Local {
    ///     path: std::path::PathBuf::from("./my-plugin.zip"),
    /// };
    /// let path = utils.fetch_package(&source, None, "安装").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_package(
        &self,
        source: &PluginSource,
        version_constraint: Option<&str>,
        error_context: &str,
    ) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let fetcher = LocalFetcher::new(&self.deps.plugin_root);
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取本地插件包失败: {} - {}", error_context, e)))
            }
            PluginSource::Remote { url, checksum } => {
                let fetcher = RemoteFetcher::new(self.deps.temp_root.clone());
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::remote(
                        url.clone(),
                        checksum.clone(),
                    ))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取远程插件包失败: {} - {}", error_context, e)))
            }
            PluginSource::Registry {
                registry_url,
                package_name,
            } => {
                let registry_info = crate::fetcher::registry::RegistryInfo::new(
                    registry_url.clone().unwrap_or_default(),
                );
                let fetcher = crate::fetcher::registry::RegistryFetcher::new(
                    registry_info,
                    self.deps.temp_root.clone(),
                );

                fetcher
                    .fetch_by_name(package_name, version_constraint.map(|s| s.to_string()))
                    .await
                    .map_err(|e| PluginError::Install(format!("从注册表获取插件包失败: {} - {}", error_context, e)))
            }
        }
    }

    /// 准备插件包用于验证
    ///
    /// 根据插件包类型进行相应处理，返回可用于验证的目录路径。
    ///
    /// # 参数
    ///
    /// * `package_path` - 插件包路径，可以是 ZIP 文件或目录
    /// * `temp_dir` - 临时目录路径，用于解压 ZIP 文件
    /// * `error_context` - 错误上下文信息，用于错误消息中标识操作来源
    ///
    /// # 返回值
    ///
    /// 返回元组 `(插件根目录路径, 是否需要清理临时目录)`：
    /// - 第一个元素是包含 manifest.json 的插件根目录路径
    /// - 第二个元素指示是否需要在操作完成后清理临时目录
    ///   - `true`: ZIP 解压产生的临时目录，需要清理
    ///   - `false`: 原始目录，不需要清理
    ///
    /// # 错误
    ///
    /// - 创建临时目录失败
    /// - 解压 ZIP 文件失败
    /// - 不支持的插件包格式
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use std::path::PathBuf;
    /// # fn example(utils: &cmx_plugin::common::PackageUtils) -> Result<(), cmx_plugin::error::PluginError> {
    /// let package_path = PathBuf::from("./my-plugin.zip");
    /// let temp_dir = PathBuf::from("./temp/extract");
    /// let (extract_path, needs_cleanup) = utils.prepare_package_for_validation(
    ///     &package_path,
    ///     &temp_dir,
    ///     "安装"
    /// )?;
    ///
    /// if needs_cleanup {
    ///     // 操作完成后需要清理 temp_dir
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn prepare_package_for_validation(
        &self,
        package_path: &Path,
        temp_dir: &Path,
        error_context: &str,
    ) -> PluginResult<(PathBuf, bool)> {
        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            std::fs::create_dir_all(temp_dir)
                .map_err(|e| PluginError::Install(format!("创建临时目录失败: {} - {}", error_context, e)))?;

            self.extract_zip(package_path, temp_dir, error_context)?;

            let extract_path = Self::find_plugin_root_in_dir(temp_dir)?;

            tracing::info!("插件包已解压到临时目录: {}", extract_path.display());

            Ok((extract_path, true))
        } else if package_path.is_dir() {
            Ok((package_path.to_path_buf(), false))
        } else {
            Err(PluginError::Install(format!(
                "不支持的插件包格式: {} - {}",
                error_context,
                package_path.display()
            )))
        }
    }

    /// 在解压目录中查找插件根目录
    ///
    /// 递归查找包含 manifest.json 的目录作为插件根目录。
    ///
    /// # 参数
    ///
    /// * `dir` - 要搜索的起始目录
    ///
    /// # 返回值
    ///
    /// 返回找到的插件根目录路径。如果未找到 manifest.json，
    /// 则返回原始目录路径。
    ///
    /// # 查找逻辑
    ///
    /// 1. 检查当前目录是否包含 manifest.json
    /// 2. 如果包含，返回当前目录
    /// 3. 否则，遍历子目录递归查找
    /// 4. 如果所有子目录都不包含 manifest.json，返回原始目录
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use std::path::Path;
    /// use cmx_plugin::common::PackageUtils;
    ///
    /// let extract_dir = Path::new("./temp/extract");
    /// let plugin_root = PackageUtils::find_plugin_root_in_dir(extract_dir)?;
    /// println!("插件根目录: {}", plugin_root.display());
    /// # Ok::<(), cmx_plugin::error::PluginError>(())
    /// ```
    pub fn find_plugin_root_in_dir(dir: &Path) -> PluginResult<PathBuf> {
        if dir.join("manifest.json").exists() {
            return Ok(dir.to_path_buf());
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("manifest.json").exists() {
                        return Ok(path);
                    }
                    if let Ok(found) = Self::find_plugin_root_in_dir(&path) {
                        return Ok(found);
                    }
                }
            }
        }

        Ok(dir.to_path_buf())
    }

    /// 解压 ZIP 文件
    ///
    /// 将 ZIP 格式的插件包解压到指定目录。
    ///
    /// # 参数
    ///
    /// * `zip_path` - ZIP 文件路径
    /// * `target` - 解压目标目录
    /// * `error_context` - 错误上下文信息，用于错误消息中标识操作来源
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - ZIP 文件不存在或无法读取
    /// - 解压过程中发生错误
    /// - 目标目录权限不足
    pub fn extract_zip(&self, zip_path: &Path, target: &Path, error_context: &str) -> PluginResult<()> {
        cmx_utils::zip::ZipExtractor::extract(zip_path, target)
            .map_err(|e| PluginError::Install(format!("解压插件包失败: {} - {}", error_context, e)))?;

        Ok(())
    }

    /// 复制插件文件
    ///
    /// 将源目录的插件文件复制到目标目录。
    ///
    /// # 参数
    ///
    /// * `source` - 源目录路径
    /// * `target` - 目标目录路径
    /// * `error_context` - 错误上下文信息，用于错误消息中标识操作来源
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - 源目录不存在
    /// - 创建目标目录失败
    /// - 复制文件失败
    ///
    /// # 说明
    ///
    /// 如果设置了 `storage` 依赖，则使用 FileStorage 进行复制；
    /// 否则使用备用方法进行递归复制。
    pub fn copy_plugin_files(
        &self,
        source: &Path,
        target: &Path,
        error_context: &str,
    ) -> PluginResult<()> {
        if source.is_dir() {
            if let Some(ref storage) = self.deps.storage {
                storage
                    .copy_dir(source, target)
                    .map_err(|e| PluginError::Install(format!("复制插件文件失败: {} - {}", error_context, e)))?;
            } else {
                Self::copy_dir_fallback(source, target, error_context)?;
            }
        }
        Ok(())
    }

    /// 备用目录复制方法
    ///
    /// 当没有 FileStorage 依赖时使用的目录复制方法。
    ///
    /// # 参数
    ///
    /// * `source` - 源目录路径
    /// * `target` - 目标目录路径
    /// * `error_context` - 错误上下文信息
    fn copy_dir_fallback(source: &Path, target: &Path, error_context: &str) -> PluginResult<()> {
        if !source.exists() {
            return Err(PluginError::Install(format!(
                "源目录不存在: {} - {}",
                error_context,
                source.display()
            )));
        }

        std::fs::create_dir_all(target)
            .map_err(|e| PluginError::Install(format!("创建目标目录失败: {} - {}", error_context, e)))?;

        fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
            if src.is_dir() {
                std::fs::create_dir_all(dst)?;
                for entry in std::fs::read_dir(src)? {
                    let entry = entry?;
                    let src_path = entry.path();
                    let dst_path = dst.join(entry.file_name());
                    copy_dir_recursive(&src_path, &dst_path)?;
                }
            } else {
                std::fs::copy(src, dst)?;
            }
            Ok(())
        }

        copy_dir_recursive(source, target)
            .map_err(|e| PluginError::Install(format!("复制目录失败: {} - {}", error_context, e)))?;

        Ok(())
    }
}

impl Default for PackageUtils {
    /// 创建默认配置的包处理工具
    ///
    /// 默认配置：
    /// - plugin_root: "./plugins"
    /// - temp_root: "./temp"
    /// - storage: None
    fn default() -> Self {
        Self::new(PackageUtilsDeps {
            plugin_root: PathBuf::from("./plugins"),
            temp_root: PathBuf::from("./temp"),
            storage: None,
        })
    }
}
