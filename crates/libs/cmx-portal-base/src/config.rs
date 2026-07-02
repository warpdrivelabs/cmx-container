//! 门户数据根目录解析。
//!
//! 所有门户资源（menu-pages / html-pages / dict / meta / fact ...）都以 JSON 文件形式
//! 存放在「数据根目录」下，沿用 CMXPortalManager Node 后端的 `data/` 目录布局。
//!
//! 数据根来源优先级：
//! 1. 配置项 `portal.data_root`（`config.toml` 的 `[portal] data_root = "..."`）
//! 2. 环境变量 `CMX_PORTAL_DATA_ROOT`
//! 3. 回退默认 `./data`（相对进程工作目录）

use std::path::{Path, PathBuf};

use cmx_utils::ConfigManager;

/// 默认数据根（相对进程工作目录）。
const DEFAULT_DATA_ROOT: &str = "./data";

/// 解析门户数据根目录的绝对/相对路径。
///
/// 不做存在性校验——具体资源读写时若文件缺失再返回 [`crate::PortalError::NotFound`]。
pub fn data_root() -> PathBuf {
    // 1) 配置项 portal.data_root（用 try_global 避免配置未初始化时 panic）
    if let Some(cfg) = ConfigManager::try_global()
        && let Ok(p) = cfg.get_string("portal.data_root")
    {
        let p = p.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    // 2) 环境变量
    if let Ok(p) = std::env::var("CMX_PORTAL_DATA_ROOT") {
        let p = p.trim().to_string();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    // 3) 回退默认
    PathBuf::from(DEFAULT_DATA_ROOT)
}

/// 在数据根下按相对段拼接路径，例如 `data_path(["activities", "domains.json"])`。
pub fn data_path<I, S>(segments: I) -> PathBuf
where
    I: IntoIterator<Item = S>,
    S: AsRef<Path>,
{
    let mut p = data_root();
    for seg in segments {
        p.push(seg);
    }
    p
}
