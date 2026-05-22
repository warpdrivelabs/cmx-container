//! 插件运行时加载器。
//!
//! 负责从多种来源（Local/Remote/Marketplace/Storage）下载插件文件到本地
//! 并加载 Service 和 WASM 运行时，不执行任何数据库变更操作。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::{PackageUtils, PackageUtilsDeps};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginFilter, PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::initializer::build_plugin_source;

/// 插件运行时加载器。
///
/// 从数据库查询插件状态，按需从多种来源（Local/Remote/Marketplace/Storage）
/// 下载插件文件到本地，然后加载 Service 定义并通知 WASM Runtime 热加载。
pub struct RuntimeLoader {
    repository: Arc<PluginRepository>,
    registry: Arc<RwLock<PluginRegistry>>,
    contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
    plugin_root: PathBuf,
    app_id: String,
    package_utils: PackageUtils,
}

impl RuntimeLoader {
    /// 创建 RuntimeLoader 实例。
    ///
    /// # Arguments
    ///
    /// * `repository` - 插件数据仓库
    /// * `registry` - 插件注册表
    /// * `contexts` - 插件上下文映射
    /// * `plugin_root` - 插件文件根目录
    /// * `app_id` - 当前应用ID
    /// * `temp_root` - 临时文件目录，用于插件包下载和解压
    pub fn new(
        repository: Arc<PluginRepository>,
        registry: Arc<RwLock<PluginRegistry>>,
        contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
        plugin_root: PathBuf,
        app_id: String,
        temp_root: PathBuf,
    ) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: plugin_root.clone(),
            temp_root,
            storage: None,
        });
        Self {
            repository,
            registry,
            contexts,
            plugin_root,
            app_id,
            package_utils,
        }
    }

    /// 加载插件到运行时。
    ///
    /// 从数据库查询插件状态，检查本地缓存，按需下载，
    /// 然后注册到内存 Registry 并加载 Service 定义。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件唯一标识
    /// * `version` - 插件版本号
    pub async fn load_plugin(&self, plugin_id: &str, version: &str) -> PluginResult<()> {
        // 使用 app_id 过滤查询插件记录，确保只加载当前应用的插件
        let filter = PluginFilter {
            app_id: Some(self.app_id.clone()),
            ..Default::default()
        };
        let records = self.repository.list_plugins(&filter).await?;
        let record = match records.iter().find(|r| r.plugin_id == plugin_id) {
            Some(r) => r.clone(),
            None => {
                tracing::warn!(
                    "Plugin {} not found in database for app_id={}, skipping load",
                    plugin_id,
                    self.app_id
                );
                return Ok(());
            }
        };

        // 检查 DDL 执行状态：所有 cmx_meta_table_define 的 ddl_status 必须为 completed
        // let ddl_ok = self.repository.check_ddl_completed(plugin_id).await;
        // match ddl_ok {
        //     Ok(true) => {}
        //     Ok(false) => {
        //         tracing::warn!(
        //             "Plugin {} DDL not completed, skipping runtime load",
        //             plugin_id
        //         );
        //         return Ok(());
        //     }
        //     Err(e) => {
        //         tracing::warn!(
        //             "Plugin {} DDL status check failed: {}, proceeding with load",
        //             plugin_id,
        //             e
        //         );
        //     }
        // }

        let local_path = self
            .plugin_root
            .join(&self.app_id)
            .join(plugin_id)
            .join(version);

        if !local_path.exists() {
            let source = build_plugin_source(
                record.zip_source_url.as_deref(),
                record.zip_source_type.as_deref(),
            );
            self.sync_plugin_files(plugin_id, version, &source).await?;
        }

        let plugin_info = PluginInfo {
            id: record.plugin_id.clone(),
            name: record.name.clone(),
            version: record.version.clone(),
            description: record.description.clone(),
            author: record.vendor_name.clone(),
            source: PluginSource::Local {
                path: local_path.clone(),
            },
            status: PluginStatus::Installed,
            installed_at: Some(record.create_time),
            updated_at: Some(record.update_time),
            install_path: local_path.clone(),
            plugin_type: record.plugin_type.clone().unwrap_or_default(),
            source_path: record.source_path.clone(),
            domain_code: record.domain_code.clone().unwrap_or_default(),
            application_code: record.application_code.clone().unwrap_or_default(),
            module_code: record.module_code.clone().unwrap_or_default(),
            app_id: record.app_id.clone(),
        };

        {
            let mut registry = self.registry.write().await;
            registry.register(plugin_info);
        }

        let ctx = PluginContext::from_db_record(&record);
        {
            let mut contexts = self.contexts.write().await;
            contexts.insert(plugin_id.to_string(), ctx);
        }

        tracing::info!(
            "Plugin {} v{} loaded into runtime (app_id={})",
            plugin_id,
            version,
            self.app_id
        );

        // TODO: Notify WASM Runtime to hot-load (will be implemented with lifecycle events)
        // TODO: Notify Service Registry to load service definitions from DB

        Ok(())
    }

    /// 卸载插件的运行时。
    ///
    /// 从内存 Registry 移除插件信息，清理 PluginContext，
    /// 可选清理本地缓存文件。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件唯一标识
    pub async fn unload_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        {
            let mut registry = self.registry.write().await;
            registry.unregister(plugin_id);
        }

        {
            let mut contexts = self.contexts.write().await;
            contexts.remove(plugin_id);
        }

        tracing::info!(
            "Plugin {} unloaded from runtime (app_id={})",
            plugin_id,
            self.app_id
        );

        // TODO: Notify WASM Runtime to unload
        // TODO: Notify Service Registry to remove service definitions

        Ok(())
    }

    /// 从指定来源同步插件文件到本地目录。
    ///
    /// 使用原子性下载策略：先下载到临时目录，完成后 rename 到正式目录。
    /// 支持 Local/Remote/Marketplace/Storage 四种来源。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件唯一标识
    /// * `version` - 插件版本号
    /// * `source` - 插件来源，决定下载方式
    ///
    /// # Returns
    ///
    /// 成功时返回插件文件的目标目录路径。
    ///
    /// # Errors
    ///
    /// 当插件包获取失败、解压失败或文件系统操作失败时返回错误。
    pub async fn sync_plugin_files(
        &self,
        plugin_id: &str,
        version: &str,
        source: &PluginSource,
    ) -> PluginResult<PathBuf> {
        let target_dir = self
            .plugin_root
            .join(&self.app_id)
            .join(plugin_id)
            .join(version);

        if target_dir.exists() {
            return Ok(target_dir);
        }

        let package_path = self
            .package_utils
            .fetch_package(source, None, "运行时加载")
            .await?;

        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            let temp_extract_dir = self
                .plugin_root
                .join(".downloading")
                .join(plugin_id)
                .join(version);

            tokio::fs::create_dir_all(&temp_extract_dir).await?;

            self.package_utils
                .prepare_package_for_validation(
                    &package_path,
                    &temp_extract_dir,
                    "运行时加载",
                )?;

            let plugin_root_in_temp =
                PackageUtils::find_plugin_root_in_dir(&temp_extract_dir)?;

            if let Some(parent) = target_dir.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::rename(&plugin_root_in_temp, &target_dir).await?;

            let _ = tokio::fs::remove_dir_all(&temp_extract_dir).await;
        } else if package_path.is_dir() {
            if let Some(parent) = target_dir.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::create_dir_all(&target_dir).await?;
        }

        tracing::info!(
            "Plugin {} v{} files synced to {:?}",
            plugin_id,
            version,
            target_dir
        );

        Ok(target_dir)
    }
}
