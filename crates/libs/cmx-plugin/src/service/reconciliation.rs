//! 插件定时对账任务。
//!
//! 每 60s 对比数据库与本地 Registry，自动补偿差异。
//! 对账按 `app_id` 过滤，仅处理当前应用的插件。

use std::collections::HashMap;
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
/// 与本地内存 Registry 对比，自动补偿差异（缺失的加载、多余的卸载）。
pub struct ReconciliationTask {
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    runtime_loader: Arc<RuntimeLoader>,
    app_id: String,
    interval: Duration,
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
    ) -> Self {
        Self {
            repository,
            registry,
            runtime_loader,
            app_id,
            interval: Duration::from_secs(interval_secs.max(10)),
        }
    }

    /// 启动定时对账任务。
    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.interval);
            loop {
                interval.tick().await;
                if let Err(e) = self.reconcile().await {
                    tracing::error!(
                        "Plugin reconciliation failed (app_id={}): {}",
                        self.app_id,
                        e
                    );
                }
            }
        });
    }

    /// 执行一次对账。
    ///
    /// 对比数据库与本地 Registry，补偿差异：
    /// - DB 中存在但 Registry 中缺失的插件：调用 `RuntimeLoader::load_plugin()`
    /// - Registry 中存在但 DB 中不存在的插件：调用 `RuntimeLoader::unload_plugin()`
    pub async fn reconcile(&self) -> crate::error::PluginResult<ReconcileResult> {
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
            if !registry_plugin_ids.contains(plugin_id) {
                tracing::info!(
                    "Reconciliation: loading missing plugin {} v{} (app_id={})",
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
                            "Reconciliation: failed to load plugin {}: {}",
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
                    "Reconciliation: unloading orphan plugin {} (app_id={})",
                    plugin_id,
                    self.app_id
                );
                match self.runtime_loader.unload_plugin(plugin_id).await {
                    Ok(()) => {
                        result.unloaded.push(plugin_id.clone());
                    }
                    Err(e) => {
                        tracing::error!(
                            "Reconciliation: failed to unload plugin {}: {}",
                            plugin_id,
                            e
                        );
                        result.failed.push((plugin_id.clone(), e.to_string()));
                    }
                }
            }
        }

        if !result.loaded.is_empty() || !result.unloaded.is_empty() || !result.failed.is_empty() {
            tracing::info!(
                "Reconciliation complete (app_id={}): loaded={}, unloaded={}, failed={}",
                self.app_id,
                result.loaded.len(),
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
    /// 本次对账加载的插件
    pub loaded: Vec<String>,
    /// 本次对账卸载的插件
    pub unloaded: Vec<String>,
    /// 本次对账失败的插件及错误信息
    pub failed: Vec<(String, String)>,
}
