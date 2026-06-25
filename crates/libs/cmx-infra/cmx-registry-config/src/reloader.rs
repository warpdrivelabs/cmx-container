//! 配置重载器。
//!
//! 该模块是配置热更新的核心执行单元，负责接收远程配置变更通知，
//! 解析新配置并原子替换全局 `ConfigManager`。
//!
//! # 工作流程
//!
//! 1. 记录当前全局配置的 key 集合用于 diff 计算。
//! 2. 按 `本地 TOML → 新远程配置 → 环境变量` 的优先级合并构建新配置。
//! 3. 计算新旧配置的变更 key 列表（新增、删除、值变化）。
//! 4. 调用 `ConfigManager::reload()` 原子替换全局配置实例。
//! 5. 失败时保留旧配置，确保服务可用性。
//!
//! 详细机制说明参见 `docs/配置变更事件订阅发布机制.md`。

use std::collections::HashSet;
use std::sync::Arc;

use cmx_utils::{ConfigBuilder, ConfigManager};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::config_source::RemoteConfigSource;
use crate::error::ConfigCenterError;

/// 配置重载器。
///
/// 注册到 `GlobalChangeNotifier` 后，当远程配置变更时自动执行热更新。
/// 持有本地配置文件路径（可选），用于 reload 时保持与启动时一致的合并策略。
///
/// # 并发互斥
///
/// 内部使用 `tokio::sync::Mutex` 串行化 reload 执行，避免配置变更频繁时
/// 多个 reload task 并发执行导致 `changed_keys` 漏报（基于过期 old_config 计算 diff）。
/// `reload` 保持为 `async fn`，内部调用同步实现 `reload_inner`。
pub struct ConfigReloader {
    /// 本地 TOML 配置文件路径，用于 reload 时合并。
    config_file_path: Option<String>,
    /// reload 串行化锁，确保同一时刻只有一个 reload task 在执行。
    reload_lock: Arc<Mutex<()>>,
}

impl ConfigReloader {
    /// 创建配置重载器。
    ///
    /// # Arguments
    ///
    /// * `config_file_path` - 本地 TOML 配置文件路径（如 `CONFIG_FILE` 环境变量的值）。
    ///   传 `None` 表示仅使用远程配置 + 环境变量。
    ///
    /// # Returns
    ///
    /// 返回新的 `ConfigReloader` 实例。
    pub fn new(config_file_path: Option<String>) -> Self {
        Self {
            config_file_path,
            reload_lock: Arc::new(Mutex::new(())),
        }
    }

    /// 执行配置重载（异步入口，串行化）。
    ///
    /// 通过 `tokio::sync::Mutex` 串行化 reload 执行，避免并发引发的 changed_keys 漏报。
    /// 实际的同步 reload 逻辑在 [`reload_inner`] 中实现。
    ///
    /// # Arguments
    ///
    /// * `new_content` - 新的远程 TOML 配置内容。
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - 成功时返回变更的配置键列表（新增、删除、值变化）。
    /// * `Err(ConfigCenterError)` - 解析失败、构建失败或全局替换失败。
    ///
    /// # Errors
    ///
    /// * `ConfigCenterError::ParseFailed` - 本地配置加载、TOML 解析或 config-rs 构建失败。
    /// * `ConfigCenterError::ReloadFailed` - 全局配置替换失败（`ConfigManager::reload()`）。
    pub async fn reload(&self, new_content: &str) -> Result<Vec<String>, ConfigCenterError> {
        let _guard = self.reload_lock.lock().await;
        self.reload_inner(new_content)
    }

    /// 执行配置重载（同步实现）。
    ///
    /// 解析新的远程配置内容，与本地配置文件和环境变量合并后，
    /// 原子替换全局配置。失败时保留旧配置，不影响服务运行。
    ///
    /// 该函数保持同步实现，因为整个流程都是同步 CPU 密集操作：
    /// TOML 解析、`HashSet` diff 计算、`ConfigManager::reload()` 原子替换。
    /// 调用方应在 `tokio::spawn` 中调用 [`reload`]，避免阻塞 Nacos 监听线程。
    ///
    /// # Arguments
    ///
    /// * `new_content` - 新的远程 TOML 配置内容。
    fn reload_inner(&self, new_content: &str) -> Result<Vec<String>, ConfigCenterError> {
        // 1. 记录旧配置的 key 集合，用于后续 diff 计算。
        let old_config = ConfigManager::global();
        let old_keys: HashSet<String> = old_config.keys().collect();

        // 2. 构建新配置：本地 TOML + 新远程配置 + 环境变量。
        let mut builder = ConfigBuilder::new();

        // 2a. 本地 TOML 文件（保持与启动时一致的优先级）。
        if let Some(path) = &self.config_file_path {
            builder = builder.add_toml_file(path).map_err(|e| {
                ConfigCenterError::ParseFailed(format!("本地配置文件加载失败: {}", e))
            })?;
        }

        // 2b. 新的远程配置内容（替换旧的远程配置）。
        let source = RemoteConfigSource::from_toml_str(new_content)?;
        builder = builder.add_source(source);

        // 2c. 环境变量（保持最高优先级，确保运维覆盖仍然生效）。
        builder = builder.add_env();

        // 3. 构建并验证新配置。
        let new_config = builder.build().map_err(|e| {
            ConfigCenterError::ParseFailed(format!("配置重载构建失败: {}", e))
        })?;

        // 4. 计算变更 key 列表（对称差 + 值变化的交集）。
        let new_keys: HashSet<String> = new_config.keys().collect();
        let changed_keys: Vec<String> = old_keys
            .symmetric_difference(&new_keys)
            .chain(
                old_keys
                    .intersection(&new_keys)
                    .filter(|k| old_config.get(k) != new_config.get(k)),
            )
            .cloned()
            .collect();

        // 5. 原子替换全局配置。
        match ConfigManager::reload(new_config) {
            Ok(_) => {
                info!(
                    "配置热更新成功，变更 {} 个 key: {:?}",
                    changed_keys.len(),
                    changed_keys
                );
                Ok(changed_keys)
            }
            Err(e) => {
                warn!("配置热更新失败: {}，保留当前配置", e);
                Err(ConfigCenterError::ReloadFailed(format!(
                    "全局配置替换失败: {}",
                    e
                )))
            }
        }
    }
}
