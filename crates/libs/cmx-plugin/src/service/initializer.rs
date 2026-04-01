//! 插件初始化同步模块
//!
//! 负责程序启动时的插件加载和同步逻辑
//!
//! # 设计思路
//!
//! 启动时需要完成以下工作：
//! 1. 从 cmx_plugin 表获取期望安装的插件列表
//! 2. 从 cmx_plugin_deployments 表获取当前节点已部署的插件版本
//! 3. 对比得出需要执行的操作（安装/升级/降级/卸载）
//! 4. 根据 zip_source 构建 PluginSource 并执行操作
//! 5. 最后初始化内存中的 contexts

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::service::downgrade::{DowngradeRequest, DowngradeService};
use crate::service::install::{InstallRequest, InstallService};
use crate::service::uninstall::{UninstallRequest, UninstallService};
use crate::service::upgrade::{UpgradeRequest, UpgradeService};

/// 插件操作计划
#[derive(Debug, Clone)]
pub enum PluginOperation {
    /// 需要安装
    Install {
        plugin_id: String,
        version: String,
        source: PluginSource,
    },
    /// 需要升级
    Upgrade {
        plugin_id: String,
        from_version: String,
        to_version: String,
        source: PluginSource,
    },
    /// 需要降级
    Downgrade {
        plugin_id: String,
        from_version: String,
        to_version: String,
        source: PluginSource,
    },
    /// 需要卸载
    Uninstall {
        plugin_id: String,
        version: String,
    },
    /// 无需操作
    None,
}

/// 插件同步结果
#[derive(Debug, Clone)]
pub struct PluginSyncResult {
    /// 成功安装的插件数
    pub installed: Vec<String>,
    /// 成功升级的插件数
    pub upgraded: Vec<String>,
    /// 成功降级的插件数
    pub downgraded: Vec<String>,
    /// 成功卸载的插件数
    pub uninstalled: Vec<String>,
    /// 跳过的插件数
    pub skipped: Vec<String>,
    /// 失败的插件及错误信息
    pub failed: Vec<(String, String)>,
}

/// 插件初始化器
pub struct PluginInitializer {
    repository: Arc<PluginRepository>,
    deployment_repository: Arc<DeploymentRepository>,
    version_history_repository: Arc<VersionHistoryRepository>,
    registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
    install_service: InstallService,
    upgrade_service: UpgradeService,
    downgrade_service: DowngradeService,
    uninstall_service: UninstallService,
    node_id: String,
}

impl PluginInitializer {
    /// 创建新的插件初始化器
    pub fn new(
        repository: Arc<PluginRepository>,
        deployment_repository: Arc<DeploymentRepository>,
        version_history_repository: Arc<VersionHistoryRepository>,
        registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
        contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
        install_service: InstallService,
        upgrade_service: UpgradeService,
        downgrade_service: DowngradeService,
        uninstall_service: UninstallService,
        node_id: String,
    ) -> Self {
        Self {
            repository,
            deployment_repository,
            version_history_repository,
            registry,
            contexts,
            install_service,
            upgrade_service,
            downgrade_service,
            uninstall_service,
            node_id,
        }
    }

    /// 执行插件同步
    ///
    /// 这是启动时的主要同步入口：
    /// 1. 查询 cmx_plugin 获取期望插件
    /// 2. 查询 cmx_plugin_deployments 获取当前节点部署
    /// 3. 对比生成操作计划
    /// 4. 执行计划
    /// 5. 加载 contexts
    pub async fn sync_plugins(&self) -> PluginResult<PluginSyncResult> {
        let mut result = PluginSyncResult {
            installed: Vec::new(),
            upgraded: Vec::new(),
            downgraded: Vec::new(),
            uninstalled: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
        };

        // 步骤1: 查询 cmx_plugin 获取所有需要安装的插件
        let expected_plugins = self.repository.list_plugins(&crate::domain::plugin::PluginFilter::default()).await?;
        let expected_map: HashMap<String, (String, Option<String>, Option<String>)> = expected_plugins
            .iter()
            .map(|p| (p.plugin_id.clone(), (p.version.clone(), p.zip_source_url.clone(), p.zip_source_type.clone())))
            .collect();

        // 步骤2: 查询 cmx_plugin_deployments 获取当前节点的部署
        let current_deployments = self.deployment_repository.list_node_deployments(&self.node_id).await?;

        // 按 plugin_id 取最高版本（同一插件可能有多条部署记录）
        let mut deployed_map: HashMap<String, String> = HashMap::new();
        for d in &current_deployments {
            if let Some(existing_version) = deployed_map.get(&d.plugin_id) {
                if &d.version > existing_version {
                    deployed_map.insert(d.plugin_id.clone(), d.version.clone());
                }
            } else {
                deployed_map.insert(d.plugin_id.clone(), d.version.clone());
            }
        }

        // 步骤3: 生成操作计划
        let mut install_ops = Vec::new();
        let mut upgrade_ops = Vec::new();
        let mut uninstall_ops = Vec::new();

        // 遍历期望插件，决定安装/升级
        for (plugin_id, (expected_version, zip_source_url, zip_source_type)) in &expected_map {
            let source = build_plugin_source(
                zip_source_url.as_deref(),
                zip_source_type.as_deref()
            );

            if let Some(deployed_version) = deployed_map.get(plugin_id) {
                if expected_version > deployed_version {
                    // 期望版本高于已部署版本，需要升级
                    upgrade_ops.push(PluginOperation::Upgrade {
                        plugin_id: plugin_id.clone(),
                        from_version: deployed_version.clone(),
                        to_version: expected_version.clone(),
                        source,
                    });
                } else {
                    // 期望版本 <= 已部署版本，无需操作
                    result.skipped.push(plugin_id.clone());
                }
            } else {
                // 节点上没有部署，需要安装
                install_ops.push(PluginOperation::Install {
                    plugin_id: plugin_id.clone(),
                    version: expected_version.clone(),
                    source,
                });
            }
        }

        // 遍历已部署但不在期望列表中的插件，需要卸载
        for (plugin_id, deployed_version) in &deployed_map {
            if !expected_map.contains_key(plugin_id) {
                uninstall_ops.push(PluginOperation::Uninstall {
                    plugin_id: plugin_id.clone(),
                    version: deployed_version.clone(),
                });
            }
        }

        // 步骤4: 执行计划 - 先处理安装
        for op in install_ops {
            match self.execute_install(op).await {
                Ok(plugin_id) => result.installed.push(plugin_id),
                Err((plugin_id, err)) => result.failed.push((plugin_id, err)),
            }
        }

        // 执行升级
        for op in upgrade_ops {
            match self.execute_upgrade(op).await {
                Ok(plugin_id) => result.upgraded.push(plugin_id),
                Err((plugin_id, err)) => result.failed.push((plugin_id, err)),
            }
        }

        // 执行卸载
        for op in uninstall_ops {
            match self.execute_uninstall(op).await {
                Ok(plugin_id) => result.uninstalled.push(plugin_id),
                Err((plugin_id, err)) => result.failed.push((plugin_id, err)),
            }
        }

        // 步骤5: 加载 contexts 到内存
        self.load_contexts().await?;

        Ok(result)
    }

    /// 执行安装操作
    async fn execute_install(&self, op: PluginOperation) -> Result<String, (String, String)> {
        match op {
            PluginOperation::Install { plugin_id, version: _, source } => {
                let request = InstallRequest {
                    source,
                    db_id: None,
                    auto_activate: false,
                    version_constraint: None,
                };
                match self.install_service.install(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => Err((plugin_id, e.to_string())),
                }
            }
            _ => Err((String::new(), "Invalid operation".to_string())),
        }
    }

    /// 执行升级操作
    async fn execute_upgrade(&self, op: PluginOperation) -> Result<String, (String, String)> {
        match op {
            PluginOperation::Upgrade { plugin_id, from_version: _, to_version: _, source } => {
                let request = UpgradeRequest {
                    plugin_id: plugin_id.clone(),
                    source,
                    version_constraint: None,
                    force: false,
                    operator: Some("system".to_string()),
                };
                match self.upgrade_service.upgrade(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => Err((plugin_id, e.to_string())),
                }
            }
            _ => Err((String::new(), "Invalid operation".to_string())),
        }
    }

    /// 执行降级操作
    async fn execute_downgrade(&self, op: PluginOperation) -> Result<String, (String, String)> {
        match op {
            PluginOperation::Downgrade { plugin_id, from_version: _, to_version, source } => {
                let request = DowngradeRequest {
                    plugin_id: plugin_id.clone(),
                    target_version: to_version,
                    source: Some(source),
                    operator: Some("system".to_string()),
                };
                match self.downgrade_service.downgrade(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => Err((plugin_id, e.to_string())),
                }
            }
            _ => Err((String::new(), "Invalid operation".to_string())),
        }
    }

    /// 执行卸载操作
    async fn execute_uninstall(&self, op: PluginOperation) -> Result<String, (String, String)> {
        match op {
            PluginOperation::Uninstall { plugin_id, version: _ } => {
                let request = UninstallRequest {
                    plugin_id: plugin_id.clone(),
                    force: false,
                    operator: "system".to_string(),
                };
                match self.uninstall_service.uninstall(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => Err((plugin_id, e.to_string())),
                }
            }
            _ => Err((String::new(), "Invalid operation".to_string())),
        }
    }

    /// 加载插件上下文到内存
    async fn load_contexts(&self) -> PluginResult<()> {
        let records = self.repository.list_plugins(&crate::domain::plugin::PluginFilter::default()).await?;

        let mut registry = self.registry.write().await;
        let mut contexts = self.contexts.write().await;

        for record in records {
            let context = PluginContext::from_db_record(&record);
            contexts.insert(record.plugin_id.clone(), context);

            let source = build_plugin_source(
                record.zip_source_url.as_deref(),
                record.zip_source_type.as_deref()
            );

            let info = PluginInfo {
                id: record.plugin_id.clone(),
                name: record.name.clone(),
                version: record.version.clone(),
                description: record.description.clone(),
                author: record.vendor_name.clone(),
                source,
                status: PluginStatus::Installed,
                installed_at: Some(record.create_time),
                updated_at: Some(record.update_time),
                install_path: PathBuf::from(&record.install_path),
                domain_code: record.domain_code.unwrap_or_default(),
                application_code: record.application_code.unwrap_or_default(),
                module_code: record.module_code.unwrap_or_default(),
                plugin_type: record.plugin_type.clone().unwrap_or_default(),
                source_path: record.source_path.clone(),
            };
            registry.register(info);
        }

        Ok(())
    }
}

/// 根据 zip_source 构建 PluginSource
pub fn build_plugin_source(zip_source_url: Option<&str>, zip_source_type: Option<&str>) -> PluginSource {
    match zip_source_type {
        Some("local") => {
            let path = zip_source_url.map(PathBuf::from).unwrap_or_default();
            PluginSource::Local { path }
        }
        Some("url") | Some("remote") => {
            let url = zip_source_url.unwrap_or_default().to_string();
            PluginSource::Remote { url, checksum: None }
        }
        Some("registry") => {
            let package_name = zip_source_url.unwrap_or_default().to_string();
            PluginSource::Registry {
                registry_url: None,
                package_name,
            }
        }
        _ => {
            let path = zip_source_url.map(PathBuf::from).unwrap_or_default();
            PluginSource::Local { path }
        }
    }
}
