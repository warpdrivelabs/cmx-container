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
//! 4. 通过 RuntimeOps 执行运行时同步（单写原则：启动仅做运行时同步，不操作数据库）
//! 5. 最后初始化内存中的 contexts

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tracing::{error, info};

use crate::common::scanner::scan_local_plugins;
use crate::domain::plugin::PluginFilter;
use crate::error::PluginResult;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::service::event_publisher::EventPublisher;
use crate::service::runtime_ops::RuntimeOps;

/// 插件操作计划
#[derive(Debug, Clone)]
pub enum PluginOperation {
    /// 需要安装
    Install {
        plugin_id: String,
        version: String,
    },
    /// 需要升级
    Upgrade {
        plugin_id: String,
        from_version: String,
        to_version: String,
    },
    /// 需要降级
    Downgrade {
        plugin_id: String,
        from_version: String,
        to_version: String,
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
    /// 运行时操作层
    pub runtime: Arc<RuntimeOps>,
    /// 统一事件发布器
    pub event_publisher: EventPublisher,
    /// 插件根目录
    pub plugin_root: PathBuf,
    /// 应用隔离标识，用于过滤非本应用的插件
    pub app_id: String,
}

/// 插件初始化器
#[allow(dead_code)]
pub struct PluginInitializer {
    repository: Arc<PluginRepository>,
    version_history_repository: Arc<VersionHistoryRepository>,
    runtime: Arc<RuntimeOps>,
    event_publisher: EventPublisher,
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
            runtime: deps.runtime,
            event_publisher: deps.event_publisher,
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
    /// 4. 通过 RuntimeOps 执行运行时同步（单写原则）
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
        let filter = PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };
        let expected_plugins = self.repository.list_plugins(&filter).await?;
        let expected_map: HashMap<String, String> = expected_plugins
            .iter()
            .map(|p| (p.plugin_id.clone(), p.version.clone()))
            .collect();

        // 步骤2: 扫描本地文件系统获取已安装的插件版本
        let local_plugins = scan_local_plugins(&self.plugin_root, &self.app_id).await?;

        // 步骤3: 生成操作计划
        let mut sync_ops = Vec::new(); // 需要同步的插件（安装或版本不一致）
        let mut uninstall_ops = Vec::new();

        // 遍历期望插件，决定安装/版本同步
        for (plugin_id, expected_version) in &expected_map {
            if let Some(local_version) = local_plugins.get(plugin_id) {
                if local_version != expected_version {
                    // 版本不一致，需要重新同步（不区分升级/降级，避免字符串字典序比较的 Bug）
                    // NOTE: 使用等值比较（!=）而非大小比较（> / <），因为字符串字典序
                    // 无法正确处理语义化版本（如 "9.0.0" > "10.0.0" 为 true）。
                    // 启动同步不需要区分升级/降级方向，只需确保版本一致即可。
                    info!(
                        plugin_id = plugin_id,
                        local_version = local_version,
                        expected_version = expected_version,
                        "插件版本不一致，需要重新同步"
                    );
                    sync_ops.push((plugin_id.clone(), expected_version.clone()));
                } else {
                    // 版本一致，跳过
                    info!(
                        plugin_id = plugin_id,
                        version = expected_version,
                        "插件版本一致，无需操作"
                    );
                    result.skipped.push(plugin_id.clone());
                }
            } else {
                // 本地不存在，需要安装
                info!(
                    plugin_id = plugin_id,
                    version = expected_version,
                    "插件需要安装"
                );
                sync_ops.push((plugin_id.clone(), expected_version.clone()));
            }
        }

        // 遍历本地存在但数据库不存在的插件，需要卸载(清理)
        for (plugin_id, deployed_version) in &local_plugins {
            if !expected_map.contains_key(plugin_id) {
                info!(
                    plugin_id = plugin_id,
                    version = deployed_version,
                    "插件需要卸载"
                );
                uninstall_ops.push(plugin_id.clone());
            }
        }

        // 步骤4: 通过 RuntimeOps 执行运行时同步（单写原则：启动仅做运行时同步，不操作数据库）
        for (plugin_id, version) in &sync_ops {
            match self.runtime.sync_and_register(plugin_id, version).await {
                Ok(()) => result.installed.push(plugin_id.clone()),
                Err(e) => {
                    error!(plugin_id = plugin_id, error = %e, "插件同步失败");
                    result.failed.push((plugin_id.clone(), e.to_string()));
                }
            }
        }

        // 卸载使用 unregister_and_cleanup
        for plugin_id in &uninstall_ops {
            match self.runtime.unregister_and_cleanup(plugin_id).await {
                Ok(()) => result.uninstalled.push(plugin_id.clone()),
                Err(e) => {
                    error!(plugin_id = plugin_id, error = %e, "插件卸载清理失败");
                    result.failed.push((plugin_id.clone(), e.to_string()));
                }
            }
        }

        // 步骤5: 加载 contexts 到内存
        self.load_contexts().await?;

        Ok(result)
    }

    /// 加载插件上下文到内存
    ///
    /// 通过 RuntimeOps 的 register_from_db 将数据库中的插件注册到运行时。
    pub async fn load_contexts(&self) -> PluginResult<()> {
        let filter = PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };
        let records = self.repository.list_plugins(&filter).await?;

        for record in &records {
            if let Err(e) = self.runtime.register_from_db(&record.plugin_id, &record.version).await {
                error!(
                    plugin_id = %record.plugin_id,
                    version = %record.version,
                    error = %e,
                    "从数据库注册插件到运行时失败"
                );
            }
        }

        Ok(())
    }
}
