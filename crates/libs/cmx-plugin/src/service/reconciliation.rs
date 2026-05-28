//! 插件定时一致性校验任务。
//!
//! 每 60s 对比数据库与本地 Registry，自动补偿差异。
//! 一致性校验按 `app_id` 过滤，仅处理当前应用的插件。
//!
//! # 设计原则
//!
//! 一致性校验任务只做运行时同步（下载文件 + 内存注册/卸载），
//! 不操作数据库，符合单一写入原则。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginFilter;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::event_publisher::EventPublisher;
use crate::service::runtime_ops::RuntimeOps;

/// 插件定时一致性校验任务。
///
/// 定期从数据库查询当前 `app_id` 下所有已安装插件，
/// 与本地内存 Registry 及文件系统对比，自动补偿差异。
///
/// # 与旧实现的区别
///
/// 旧实现使用 `RuntimeLoader`，新实现使用 `RuntimeOps`，
/// 统一了运行时操作的入口，并增加了事件发布能力。
///
/// # 文件缺失修复策略
///
/// 当 Registry 中存在插件但本地文件缺失时，一致性校验任务会先注销内存状态，
/// 再调用 `sync_and_register` 重新下载文件并注册。
/// 这是因为 `sync_and_register` 的幂等检查会跳过"已注册且版本一致"的插件，
/// 必须先注销才能绕过幂等检查触发文件下载。
pub struct ReconciliationTask {
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    runtime: Arc<RuntimeOps>,
    /// 事件发布器（预留，一致性校验补偿后可能需要发布事件）
    #[allow(dead_code)]
    event_publisher: EventPublisher,
    app_id: String,
    interval: Duration,
    /// 插件文件根目录，用于检查本地文件是否存在
    plugin_root: PathBuf,
    /// 标记是否为首次执行，仅首次输出开始日志
    first_run: AtomicBool,
}

impl ReconciliationTask {
    /// 创建一致性校验任务实例。
    pub fn new(
        repository: Arc<PluginRepository>,
        registry: Arc<RwLock<PluginRegistry>>,
        runtime: Arc<RuntimeOps>,
        event_publisher: EventPublisher,
        app_id: String,
        interval_secs: u64,
        plugin_root: PathBuf,
    ) -> Self {
        Self {
            repository,
            registry,
            runtime,
            event_publisher,
            app_id,
            interval: Duration::from_secs(interval_secs.max(10)),
            plugin_root,
            first_run: AtomicBool::new(true),
        }
    }

    /// 启动定时一致性校验任务。
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.interval);
            loop {
                interval.tick().await;
                if let Err(e) = self.reconcile().await {
                    tracing::error!(
                        "插件一致性校验失败 (app_id={}): {}",
                        self.app_id,
                        e
                    );
                }
            }
        });
    }

    /// 执行一次一致性校验。
    ///
    /// 对比数据库与本地 Registry 及文件系统，补偿差异：
    /// - DB 中存在但 Registry 中缺失的插件：调用 `RuntimeOps::register_from_db()`
    /// - DB 中存在且 Registry 中也存在，但本地文件不存在：调用 `RuntimeOps::sync_and_register()`
    /// - Registry 中存在但 DB 中不存在的插件：调用 `RuntimeOps::unregister_and_cleanup()`
    pub async fn reconcile(&self) -> crate::error::PluginResult<ReconcileResult> {
        if self.first_run.swap(false, Ordering::Relaxed) {
            tracing::info!("开始插件一致性校验 (app_id={})", self.app_id);
        }
        let mut result = ReconcileResult::default();

        // 1. 查询数据库中当前 app_id 下所有已安装插件
        let filter = PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };

        let db_plugins = self.repository.list_plugins(&filter).await?;
        let db_plugin_ids: HashMap<String, String> = db_plugins
            .iter()
            .map(|p| (p.plugin_id.clone(), p.version.clone()))
            .collect();

        // 2. 获取 Registry 中已注册的插件列表
        let registry_plugin_ids: Vec<String> = {
            let registry = self.registry.read().await;
            let filter = PluginFilter {
                app_id: Some(self.app_id.clone()),
                ..Default::default()
            };
            registry.filter(&filter).iter().map(|p| p.id.clone()).collect()
        };

        // 3. 补偿 Registry 中缺失的插件
        for (plugin_id, version) in &db_plugin_ids {
            let in_registry = registry_plugin_ids.contains(plugin_id);
            let local_path = self
                .plugin_root
                .join(&self.app_id)
                .join(plugin_id)
                .join(version);
            let local_exists = local_path.exists();

            if !in_registry {
                // Registry 中缺失，需要从数据库查询并注册
                tracing::info!(
                    "插件一致性校验: 加载缺失插件 {} v{} (app_id={})",
                    plugin_id,
                    version,
                    self.app_id
                );
                match self.runtime.sync_and_register(plugin_id, version).await {
                    Ok(()) => {
                        result.loaded.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "插件一致性校验: 加载插件 {} 失败: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            } else if !local_exists {
                // Registry 中存在但本地文件不存在，需要先注销再强制重新同步
                tracing::info!(
                    "插件一致性校验: 插件 {} v{} 在 Registry 中存在但本地文件缺失，先注销再重新同步 (app_id={})",
                    plugin_id,
                    version,
                    self.app_id
                );
                // 修复策略：先注销再重新同步。
                // sync_and_register 的幂等检查会跳过"已注册且版本一致"的插件，
                // 但此时本地文件缺失，必须绕过幂等检查才能重新下载文件。
                // 先注销内存状态，避免 sync_and_register 的幂等检查跳过
                let _ = self.runtime.unregister_plugin(plugin_id).await;
                match self.runtime.sync_and_register(plugin_id, version).await {
                    Ok(()) => {
                        result.synced.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "插件一致性校验: 同步插件 {} 文件失败: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            }
        }

        // 4. 清理 Registry 中存在但 DB 中不存在的孤立插件
        for plugin_id in &registry_plugin_ids {
            if !db_plugin_ids.contains_key(plugin_id) {
                tracing::info!(
                    "插件一致性校验: 卸载孤立插件 {} (app_id={})",
                    plugin_id,
                    self.app_id
                );
                match self.runtime.unregister_and_cleanup(plugin_id).await {
                    Ok(()) => {
                        result.unloaded.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "一致性校验: 卸载插件 {} 失败: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            }
        }

        if !result.loaded.is_empty() || !result.unloaded.is_empty() || !result.failed.is_empty() || !result.synced.is_empty() {
            tracing::info!(
                "插件一致性校验完成 (app_id={}): 加载={}, 同步={}, 卸载={}, 失败={}",
                self.app_id,
                result.loaded.len(),
                result.synced.len(),
                result.unloaded.len(),
                result.failed.len()
            );
        }

        Ok(result)
    }
}

/// 一致性校验结果。
#[derive(Debug, Clone, Default)]
pub struct ReconcileResult {
    /// 本次校验加载的插件（Registry 中缺失，已加载）
    pub loaded: Vec<String>,
    /// 本次校验同步的插件（Registry 中存在但本地文件缺失，已下载）
    pub synced: Vec<String>,
    /// 本次校验卸载的插件
    pub unloaded: Vec<String>,
    /// 本次校验失败的插件及错误信息
    pub failed: Vec<(String, String)>,
}
