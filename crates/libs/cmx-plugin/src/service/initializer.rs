//! 插件初始化同步模块
//!
//! 负责程序启动时的插件加载和同步逻辑
//!
//! # 设计思路
//!
//! 启动时需要完成以下工作：
//! 1. 从 cmx_plugin 表获取期望安装的插件列表
//! 2. 扫描本地文件系统获取已安装的插件版本
//! 3. 对比得出需要执行的操作（安装/升级/降级/卸载）
//! 4. 根据 zip_source 构建 PluginSource 并执行操作
//! 5. 最后初始化内存中的 contexts

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, log};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
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
    /// 需要卸载（清理本地文件）
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

/// 插件初始化器依赖配置。
///
/// 参考其他 Service 的 Deps 模式，统一管理初始化器依赖的组件。
pub struct PluginInitializerDeps {
    /// 插件数据仓库
    pub repository: Arc<PluginRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 插件注册表
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    /// 插件上下文映射
    pub contexts: Arc<tokio::sync::RwLock<HashMap<String, PluginContext>>>,
    /// 安装服务
    pub install_service: InstallService,
    /// 升级服务
    pub upgrade_service: UpgradeService,
    /// 降级服务
    pub downgrade_service: DowngradeService,
    /// 卸载服务
    pub uninstall_service: UninstallService,
    /// 插件根目录
    pub plugin_root: PathBuf,
    /// 应用隔离标识
    pub app_id: String,
}

/// 插件初始化器
#[allow(dead_code)]
pub struct PluginInitializer {
    repository: Arc<PluginRepository>,
    version_history_repository: Arc<VersionHistoryRepository>,
    registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
    install_service: InstallService,
    upgrade_service: UpgradeService,
    downgrade_service: DowngradeService,
    uninstall_service: UninstallService,
    plugin_root: PathBuf,
    app_id: String,
}

impl PluginInitializer {
    /// 创建新的插件初始化器。
    ///
    /// # Arguments
    ///
    /// * `deps` - 初始化器依赖配置
    ///
    /// # Returns
    ///
    /// 返回初始化后的 `PluginInitializer` 实例。
    pub fn new(deps: PluginInitializerDeps) -> Self {
        Self {
            repository: deps.repository,
            version_history_repository: deps.version_history_repository,
            registry: deps.registry,
            contexts: deps.contexts,
            install_service: deps.install_service,
            upgrade_service: deps.upgrade_service,
            downgrade_service: deps.downgrade_service,
            uninstall_service: deps.uninstall_service,
            plugin_root: deps.plugin_root,
            app_id: deps.app_id,
        }
    }

    /// 执行插件同步
    ///
    /// 这是启动时的主要同步入口：
    /// 1. 查询 cmx_plugin 获取期望插件
    /// 2. 扫描本地文件系统获取已安装版本
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

        // 步骤1: 查询 cmx_plugin 获取当前 app_id 下需要安装的插件
        let filter = crate::domain::plugin::PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };
        let expected_plugins = self.repository.list_plugins(&filter).await?;
        let expected_map: HashMap<String, (String, Option<String>, Option<String>)> = expected_plugins
            .iter()
            .map(|p| (p.plugin_id.clone(), (p.version.clone(), p.zip_source_url.clone(), p.zip_source_type.clone())))
            .collect();

        // 步骤2: 扫描本地文件系统获取已安装的插件版本
        let local_plugins = self.scan_local_plugins().await?;

        // 步骤3: 生成操作计划
        let mut install_ops = Vec::new();
        let mut upgrade_ops = Vec::new();
        let mut downgrade_ops = Vec::new();
        let mut uninstall_ops = Vec::new();

        // 遍历期望插件,决定安装/升级/降级
        for (plugin_id, (expected_version, zip_source_url, zip_source_type)) in &expected_map {
            let source = build_plugin_source(
                zip_source_url.as_deref(),
                zip_source_type.as_deref()
            );

            if let Some(local_version) = local_plugins.get(plugin_id) {
                if expected_version > local_version {
                    // 期望版本高于本地版本,需要升级
                    log::info!("📦 插件 [{}] 需要升级: {} -> {}", plugin_id, local_version, expected_version);
                    upgrade_ops.push(PluginOperation::Upgrade {
                        plugin_id: plugin_id.clone(),
                        from_version: local_version.clone(),
                        to_version: expected_version.clone(),
                        source,
                    });
                } else if expected_version < local_version {
                    // 期望版本低于本地版本,需要降级
                    log::info!("⬇️  插件 [{}] 需要降级: {} -> {}", plugin_id, local_version, expected_version);
                    downgrade_ops.push(PluginOperation::Downgrade {
                        plugin_id: plugin_id.clone(),
                        from_version: local_version.clone(),
                        to_version: expected_version.clone(),
                        source,
                    });
                } else {
                    // 版本一致,跳过
                    log::debug!("✅ 插件 [{}] 版本一致 ({}), 无需操作", plugin_id, expected_version);
                    result.skipped.push(plugin_id.clone());
                }
            } else {
                // 本地不存在,需要安装
                log::info!("🆕 插件 [{}] 需要安装: 版本 {}", plugin_id, expected_version);
                install_ops.push(PluginOperation::Install {
                    plugin_id: plugin_id.clone(),
                    version: expected_version.clone(),
                    source,
                });
            }
        }

        // 遍历本地存在但数据库不存在的插件,需要卸载(清理)
        for (plugin_id, deployed_version) in &local_plugins {
            if !expected_map.contains_key(plugin_id) {
                info!("插件 [{}] 需要卸载: 版本 {}", plugin_id, deployed_version);
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

        // 执行降级
        for op in downgrade_ops {
            match self.execute_downgrade(op).await {
                Ok(plugin_id) => result.downgraded.push(plugin_id),
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
                if !version_entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
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

    /// 执行安装操作
    async fn execute_install(&self, op: PluginOperation) -> Result<String, (String, String)> {
        match op {
            PluginOperation::Install { plugin_id, version: _, source } => {
                let request = InstallRequest {
                    source,
                    db_id: None,
                    auto_activate: false,
                    version_constraint: None,
                    build_type: None,
                    marketplace_source_id: None,
                    app_id: Some(self.app_id.clone()),
                    send_event: true,
                };
                match self.install_service.install(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => {
                        error!("Failed to install plugin [{}]: {}", plugin_id, e);
                        Err((plugin_id, e.to_string()))
                    },
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
                    build_type: None,
                    marketplace_source_id: None,
                    app_id: Some(self.app_id.clone()),
                    send_event: true,
                };
                match self.upgrade_service.upgrade(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => {
                        error!("Failed to upgrade plugin [{}]: {}", plugin_id, e);
                        Err((plugin_id, e.to_string()))
                    },
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
                    app_id: Some(self.app_id.clone()),
                    send_event: true,
                };
                match self.downgrade_service.downgrade(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => {
                        error!("Failed to downgrade plugin [{}]: {}", plugin_id, e);
                        Err((plugin_id, e.to_string()))
                    },
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
                    app_id: Some(self.app_id.clone()),
                    send_event: true,
                };
                match self.uninstall_service.uninstall(request).await {
                    Ok(_) => Ok(plugin_id),
                    Err(e) => {
                        error!("Failed to uninstall plugin: {}", e);

                        Err((plugin_id, e.to_string()))
                    }




                }
            }
            _ => Err((String::new(), "Invalid operation".to_string())),
        }
    }

    /// 加载插件上下文到内存
   pub async fn load_contexts(&self) -> PluginResult<()> {
        let filter = crate::domain::plugin::PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };
        let records = self.repository.list_plugins(&filter).await?;

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
                app_id: record.app_id.clone(),
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
        Some("registry") | Some("marketplace") => {
            let plugin_id = zip_source_url.unwrap_or_default().to_string();
            PluginSource::Marketplace {
                marketplace_url: None,
                plugin_id,
            }
        }
        Some("storage") => {
            PluginSource::Storage {
                file_id: zip_source_url.unwrap_or_default().to_string(),
                checksum: None,
            }
        }
        _ => {
            let path = zip_source_url.map(PathBuf::from).unwrap_or_default();
            PluginSource::Local { path }
        }
    }
}
