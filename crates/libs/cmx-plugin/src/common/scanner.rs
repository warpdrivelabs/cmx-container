//! 本地插件扫描模块
//!
//! 提供扫描本地文件系统、获取已安装插件版本信息的通用操作。

use std::collections::HashMap;
use std::path::Path;

use crate::error::PluginResult;

/// 扫描本地文件系统，获取已安装插件的最大版本号。
///
/// 遍历 `plugin_root/app_id/` 目录下的所有插件目录，
/// 查找包含 `manifest.json` 的版本目录，返回每个插件的最大版本号。
///
/// 目录结构：`plugin_root/app_id/plugin_id/version/`，
/// 当版本目录存在且包含 `manifest.json` 时视为已安装。
///
/// # Arguments
///
/// * `plugin_root` - 插件根目录路径
/// * `app_id` - 应用标识，用于定位 `plugin_root` 下的应用子目录
///
/// # Returns
///
/// 返回 `HashMap<String, String>`，key 为 `plugin_id`，value 为该插件的最大版本号。
/// 当插件根目录不存在时返回空的 HashMap。
///
/// # Errors
///
/// 当读取目录失败时，跳过该目录并继续扫描，不会返回错误。
pub async fn scan_local_plugins(
    plugin_root: &Path,
    app_id: &str,
) -> PluginResult<HashMap<String, String>> {
    let mut local_plugins = HashMap::new();

    let app_path = plugin_root.join(app_id);
    if !app_path.exists() {
        return Ok(local_plugins);
    }

    let mut entries = match tokio::fs::read_dir(&app_path).await {
        Ok(entries) => entries,
        Err(_) => return Ok(local_plugins),
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let plugin_id = entry.file_name().to_string_lossy().to_string();
        let plugin_path = entry.path();

        let mut version_dir_entries = match tokio::fs::read_dir(&plugin_path).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut max_version = String::new();
        while let Ok(Some(version_entry)) = version_dir_entries.next_entry().await {
            if !version_entry
                .file_type()
                .await
                .map(|t| t.is_dir())
                .unwrap_or(false)
            {
                continue;
            }

            let version = version_entry.file_name().to_string_lossy().to_string();
            // 检查是否包含 manifest.json（验证是有效安装）
            let manifest_path = version_entry.path().join("manifest.json");
            if manifest_path.exists() && version > max_version {
                max_version = version;
            }
        }

        if !max_version.is_empty() {
            local_plugins.insert(plugin_id, max_version);
        }
    }

    Ok(local_plugins)
}
