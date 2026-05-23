//! 插件定时对账任务。
//!
//! 每 60s 对比数据库与本地 Registry，自动补偿差异。
//! 对账按 `app_id` 过滤，仅处理当前应用的插件。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginFilter;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::runtime_loader::RuntimeLoader;

/// 插件定时对账任务。
///
/// 定期从数据库查询当前 `app_id` 下所有已安装插件，
/// 与本地内存 Registry 及文件系统对比，自动补偿差异。
pub struct ReconciliationTask {
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    runtime_loader: Arc<RuntimeLoader>,
    app_id: String,
    interval: Duration,
    /// 插件文件根目录，用于检查本地文件是否存在
    plugin_root: PathBuf,
}

impl ReconciliationTask {
    /// 创建对账任务实例。
    ///
    /// # Arguments
    ///
    /// * `repository` - 插件数据仓库
    /// * `registry` - 插件注册表
    /// * `runtime_loader` - 运行时加载器
    /// * `app_id` - 当前应用ID
    /// * `interval_secs` - 对账间隔秒数（默认 60）
    pub fn new(
        repository: Arc<PluginRepository>,
        registry: Arc<RwLock<PluginRegistry>>,
        runtime_loader: Arc<RuntimeLoader>,
        app_id: String,
        interval_secs: u64,
        plugin_root: PathBuf,
    ) -> Self {
        Self {
            repository,
            registry,
            runtime_loader,
            app_id,
            interval: Duration::from_secs(interval_secs.max(10)),
            plugin_root,
        }
    }

    /// 启动定时对账任务。
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            //interval 的首次触发是立即的
            // tokio::time::interval 的设计逻辑是：
            // 首次调用 tick().await 会立即返回，后续调用才会等待指定间隔。
            // 这意味着：
            // 创建 interval 后，第一次 interval.tick().await 不会等待，直接执行后续代码。
            // 第二次及之后的 tick().await 才会等待 self.interval（60 秒）。
            let mut interval = tokio::time::interval(self.interval);
            loop {
                interval.tick().await;
                if let Err(e) = self.reconcile().await {
                    tracing::error!(
                        "插件对账失败 (app_id={}): {}",
                        self.app_id,
                        e
                    );
                }
            }
        });
    }

    /// 执行一次对账。
    ///
    /// 对比数据库与本地 Registry 及文件系统，补偿差异：
    /// - DB 中存在但 Registry 中缺失的插件：调用 `RuntimeLoader::load_plugin()`
    /// - DB 中存在且 Registry 中也存在，但本地文件不存在：调用 `RuntimeLoader::load_plugin()` 下载文件
    /// - Registry 中存在但 DB 中不存在的插件：调用 `RuntimeLoader::unload_plugin()`
    pub async fn reconcile(&self) -> crate::error::PluginResult<ReconcileResult> {
        tracing::info!("开始对账 (app_id={})", self.app_id);
        let mut result = ReconcileResult::default();

        let filter = PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };

        let db_plugins = self.repository.list_plugins(&filter).await?;
        let db_plugin_ids: HashMap<String, String> = db_plugins
            .iter()
            .map(|p| (p.plugin_id.clone(), p.version.clone()))
            .collect();

        let registry_plugin_ids: Vec<String> = {
            let registry = self.registry.read().await;
            let filter = PluginFilter {
                app_id: Some(self.app_id.clone()),
                ..Default::default()
            };
            registry.filter(&filter).iter().map(|p| p.id.clone()).collect()
        };

        for (plugin_id, version) in &db_plugin_ids {
            let in_registry = registry_plugin_ids.contains(plugin_id);
            let local_path = self
                .plugin_root
                .join(&self.app_id)
                .join(plugin_id)
                .join(version);
            let local_exists = local_path.exists();

            if !in_registry {
                // Registry 中缺失，需要加载
                tracing::info!(
                    "对账: 加载缺失插件 {} v{} (app_id={})",
                    plugin_id,
                    version,
                    self.app_id
                );
                match self.runtime_loader.load_plugin(plugin_id, version).await {
                    Ok(()) => {
                        result.loaded.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "对账: 加载插件 {} 失败: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            } else if !local_exists {
                // Registry 中存在但本地文件不存在，需要下载文件
                tracing::info!(
                    "对账: 插件 {} v{} 在 Registry 中存在但本地文件缺失，重新下载 (app_id={})",
                    plugin_id,
                    version,
                    self.app_id
                );
                match self.runtime_loader.load_plugin(plugin_id, version).await {
                    Ok(()) => {
                        result.synced.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "对账: 下载插件 {} 文件失败: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            }
        }

        for plugin_id in &registry_plugin_ids {
            if !db_plugin_ids.contains_key(plugin_id) {
                tracing::info!(
                    "对账: 卸载孤立插件 {} (app_id={})",
                    plugin_id,
                    self.app_id
                );
                match self.runtime_loader.unload_plugin(plugin_id).await {
                    Ok(()) => {
                        result.unloaded.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "对账: 卸载插件 {} 失败: {}",
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
                "对账完成 (app_id={}): 加载={}, 同步={}, 卸载={}, 失败={}",
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

/// 对账结果。
#[derive(Debug, Clone, Default)]
pub struct ReconcileResult {
    /// 本次对账加载的插件（Registry 中缺失，已加载）
    pub loaded: Vec<String>,
    /// 本次对账同步的插件（Registry 中存在但本地文件缺失，已下载）
    pub synced: Vec<String>,
    /// 本次对账卸载的插件
    pub unloaded: Vec<String>,
    /// 本次对账失败的插件及错误信息
    pub failed: Vec<(String, String)>,
}
