//! 插件变更通知处理器模块
//!
//! 处理从 Redis Pub/Sub 接收到的插件变更通知，
//! 根据数据库最新状态执行本地同步操作。
//!
//! # 设计原则
//!
//! - 收到通知后从数据库查询最新状态，与本地对比
//! - 所有操作天然幂等（目录已存在则跳过）
//! - 不需要分布式锁或请求去重

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::cluster::notification::{PluginChangeAction, PluginChangeNotification};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginFilter;
use crate::error::PluginResult;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::deploy::{DeployRequest, DeployService};
use crate::service::initializer::build_plugin_source;

/// 插件变更通知处理器
///
/// 接收 Redis Pub/Sub 的插件变更通知，
/// 从数据库查询最新状态后执行本地同步操作。
#[derive(Clone)]
pub struct PluginChangeHandler {
    /// 数据库仓库
    repository: Arc<PluginRepository>,
    /// 部署服务（智能安装/升级）
    deploy_service: DeployService,
    /// 插件根目录
    plugin_root: PathBuf,
    /// 插件注册表
    registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文映射
    contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
}

impl PluginChangeHandler {
    /// 创建新的插件变更处理器
    pub fn new(
        repository: Arc<PluginRepository>,
        deploy_service: DeployService,
        plugin_root: PathBuf,
        registry: Arc<RwLock<PluginRegistry>>,
        contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
    ) -> Self {
        Self {
            repository,
            deploy_service,
            plugin_root,
            registry,
            contexts,
        }
    }

    /// 处理插件变更通知
    pub async fn handle(&self, notification: &PluginChangeNotification) {
        match &notification.action {
            PluginChangeAction::Changed => {
                self.handle_plugin_changed(&notification.plugin_id).await;
            }
            PluginChangeAction::Removed => {
                self.handle_plugin_removed(&notification.plugin_id).await;
            }
        }
    }

    /// 处理插件变更（安装/升级/降级）
    ///
    /// 从数据库查询最新版本，与本地文件系统对比后执行操作。
    async fn handle_plugin_changed(&self, plugin_id: &str) {
        // 从数据库查询最新插件记录
        let db_plugin = match self.repository.find_plugin(plugin_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                tracing::warn!(
                    "收到插件 {} 变更通知，但数据库中未找到记录",
                    plugin_id
                );
                return;
            }
            Err(e) => {
                tracing::error!("查询插件 {} 失败: {}", plugin_id, e);
                return;
            }
        };

        // 检查本地是否已是最新版本
        let local_path = self.plugin_root.join(plugin_id).join(&db_plugin.version);
        if local_path.exists() {
            tracing::debug!(
                "插件 {} 版本 {} 本地已存在，跳过同步",
                plugin_id,
                db_plugin.version
            );
            return;
        }

        // 根据 zip_source 构建 PluginSource
        let source = build_plugin_source(
            db_plugin.zip_source_url.as_deref(),
            db_plugin.zip_source_type.as_deref(),
        );

        // 执行部署（自动判断安装/升级）
        let request = DeployRequest {
            source,
            db_id: Some(db_plugin.db_id.clone()),
            force_reinstall: false,
            build_type: None,
        };

        match self.deploy_service.deploy(request).await {
            Ok(result) => {
                tracing::info!(
                    "插件 {} 远程同步完成: {} -> {}",
                    plugin_id,
                    result.old_version.as_deref().unwrap_or("无"),
                    result.new_version
                );
            }
            Err(e) => {
                tracing::error!("插件 {} 远程同步失败: {}", plugin_id, e);
            }
        }
    }

    /// 处理插件移除
    ///
    /// 从内存中移除插件信息，清理本地文件。
    async fn handle_plugin_removed(&self, plugin_id: &str) {
        // 从内存注册表移除
        {
            let mut registry = self.registry.write().await;
            registry.unregister(plugin_id);
        }

        // 从上下文映射移除
        {
            let mut contexts = self.contexts.write().await;
            contexts.remove(plugin_id);
        }

        // 清理本地文件目录
        let plugin_dir = self.plugin_root.join(plugin_id);
        if plugin_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&plugin_dir).await {
                tracing::error!("清理插件 {} 本地文件失败: {}", plugin_id, e);
            } else {
                tracing::info!("已清理插件 {} 本地文件", plugin_id);
            }
        }
    }

    /// 全量同步（启动时或收到全量同步请求时调用）
    ///
    /// 对比数据库中所有插件与本地文件系统状态，执行差异同步。
    pub async fn full_sync(&self) -> PluginResult<SyncResult> {
        let mut result = SyncResult::default();

        // 查询数据库中所有期望的插件
        let expected_plugins = self.repository.list_plugins(&PluginFilter::default()).await?;
        let expected_map: HashMap<String, (String, Option<String>, Option<String>)> =
            expected_plugins
                .iter()
                .map(|p| {
                    (
                        p.plugin_id.clone(),
                        (p.version.clone(), p.zip_source_url.clone(), p.zip_source_type.clone()),
                    )
                })
                .collect();

        // 扫描本地文件系统
        let local_plugins = self.scan_local_plugins().await?;

        // 对比差异
        for (plugin_id, (version, zip_url, zip_type)) in &expected_map {
            let local_version = local_plugins.get(plugin_id);

            match local_version {
                Some(ver) if ver == version => {
                    result.synced.push(plugin_id.clone());
                }
                Some(_) | None => {
                    // 本地版本不一致或不存在，需要同步
                    let source = build_plugin_source(zip_url.as_deref(), zip_type.as_deref());
                    let request = DeployRequest {
                        source,
                        db_id: None,
                        force_reinstall: false,
                        build_type: None,
                    };

                    match self.deploy_service.deploy(request).await {
                        Ok(_) => {
                            result.synced.push(plugin_id.clone());
                        }
                        Err(e) => {
                            tracing::error!("插件 {} 全量同步失败: {}", plugin_id, e);
                            result.failed.push((plugin_id.clone(), e.to_string()));
                        }
                    }
                }
            }
        }

        // 清理本地存在但数据库不存在的插件
        for plugin_id in local_plugins.keys() {
            if !expected_map.contains_key(plugin_id) {
                let plugin_dir = self.plugin_root.join(plugin_id);
                if let Err(e) = tokio::fs::remove_dir_all(&plugin_dir).await {
                    tracing::error!("清理插件 {} 本地文件失败: {}", plugin_id, e);
                }
                result.cleaned.push(plugin_id.clone());
            }
        }

        Ok(result)
    }

    /// 扫描本地文件系统，获取已安装的插件版本
    ///
    /// 目录结构: ${plugin_root}/${plugin_id}/${version}/
    /// 只要版本目录存在且包含 manifest.json，视为已安装
    async fn scan_local_plugins(&self) -> PluginResult<HashMap<String, String>> {
        let mut local_plugins = HashMap::new();

        if !self.plugin_root.exists() {
            return Ok(local_plugins);
        }

        let mut entries = match tokio::fs::read_dir(&self.plugin_root).await {
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
}

/// 同步结果
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// 已同步的插件列表
    pub synced: Vec<String>,
    /// 清理的插件列表
    pub cleaned: Vec<String>,
    /// 失败的插件及错误信息
    pub failed: Vec<(String, String)>,
}
