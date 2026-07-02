//! 插件运行时操作层。
//!
//! 负责插件的内存注册/卸载、缓存更新和运行时文件同步，
//! 不涉及任何数据库写操作。
//!
//! # 核心职责
//!
//! - 将插件注册到内存（Registry + Contexts + Cache）
//! - 从内存注销插件
//! - 从数据库查询并注册插件（其他节点通知场景）
//! - 同步文件并注册（跨节点通知场景）
//! - 强制重新同步并注册（覆盖安装场景）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::common::source_utils::build_plugin_source;
use crate::common::{PackageUtils, PackageUtilsDeps};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::service::persistence::PersistResult;

/// 运行时操作层依赖。
///
/// 精简的依赖结构，仅包含运行时层所需的最小依赖集合。
#[derive(Clone)]
pub struct RuntimeOpsDeps {
    /// 插件数据仓库（仅用于查询，不执行写操作）
    pub repository: Arc<PluginRepository>,
    /// 插件注册表
    pub registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文映射
    pub contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
    /// 多层缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 插件文件根目录
    pub plugin_root: PathBuf,
    /// 临时文件目录
    pub temp_root: PathBuf,
    /// 当前应用ID
    pub app_id: String,
}

/// 插件运行时操作。
///
/// 提供插件在运行时的注册、注销、文件同步等操作，
/// 所有方法均为内存级别操作，不涉及数据库写操作。
pub struct RuntimeOps {
    deps: RuntimeOpsDeps,
    package_utils: PackageUtils,
}

impl RuntimeOps {
    /// 创建运行时操作实例。
    pub fn new(deps: RuntimeOpsDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: None,
        });
        Self {
            deps,
            package_utils,
        }
    }

    /// 注册插件到内存。
    ///
    /// 将 PersistResult 中的插件信息写入 Registry、Contexts 和 Cache，
    /// 用于当前节点完成持久化后的本地注册。
    ///
    /// # 幂等性
    ///
    /// Registry 的 `register` 和 HashMap 的 `insert` 天然幂等，
    /// 重复注册会覆盖旧值。
    pub async fn register_plugin(&self, result: &PersistResult) -> PluginResult<()> {
        let local_path = &result.install_path;

        // 1. 构建 PluginInfo
        let plugin_info = PluginInfo {
            id: result.plugin_id.clone(),
            name: result.plugin_name.clone().unwrap_or_default(),
            version: result.version.clone(),
            description: result.description.clone(),
            author: None,
            source: PluginSource::Local {
                path: local_path.clone(),
            },
            status: PluginStatus::Installed,
            installed_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
            install_path: local_path.clone(),
            plugin_type: result.plugin_type.clone().unwrap_or_default(),
            source_path: result.source_path.clone(),
            domain_code: result.domain_code.clone(),
            application_code: result.application_code.clone(),
            module_code: result.module_code.clone(),
            app_id: result.app_id.clone(),
        };

        // 2. 注册到 Registry
        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info);
        }

        // 3. 写入 Contexts
        let ctx = PluginContext::new(result.plugin_id.clone(), result.version.clone());
        let ctx = PluginContext {
            app_id: result.app_id.clone(),
            install_path: result.install_path.clone(),
            wasm_path: PathBuf::from(&result.wasm_path),
            plugin_type: result.plugin_type.clone(),
            source_path: result.source_path.clone(),
            ..ctx
        };
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.insert(result.plugin_id.clone(), ctx);
        }

        // 4. 写入 Cache
        let cache_key = format!("plugin:{}:{}", result.app_id, result.plugin_id);
        let cache_value =
            crate::infrastructure::cache::layered::CacheValue::Json(serde_json::json!({
                "plugin_id": result.plugin_id,
                "app_id": result.app_id,
                "version": result.version,
            }));
        self.deps.cache.set(&cache_key, cache_value, None).await;

        tracing::info!(
            plugin_id = %result.plugin_id,
            version = %result.version,
            app_id = %result.app_id,
            "插件已注册到运行时"
        );

        Ok(())
    }

    /// 更新插件内存信息。
    ///
    /// 用于升级/降级后版本变更场景，覆盖 Registry 和 Contexts 中的旧数据，
    /// 并刷新 Cache。
    pub async fn update_plugin(&self, result: &PersistResult) -> PluginResult<()> {
        // 更新操作与注册操作逻辑一致：覆盖旧值
        self.register_plugin(result).await
    }

    /// 同步文件并注册插件。
    ///
    /// 其他节点收到 Installed/Upgraded/Downgraded 通知时使用，
    /// 包含幂等检查：如果插件已注册且版本一致，跳过。
    pub async fn sync_and_register(&self, plugin_id: &str, version: &str) -> PluginResult<()> {
        // 1. 幂等检查：已注册且版本一致则跳过
        {
            let registry = self.deps.registry.read().await;
            if let Some(existing) = registry.get(plugin_id)
                && existing.version == version
            {
                tracing::info!(
                    plugin_id = plugin_id,
                    version = version,
                    "插件已注册且版本一致，跳过同步注册"
                );
                return Ok(());
            }
        }
        let record = self
            .deps
            .repository
            .find_plugin(plugin_id, &self.deps.app_id)
            .await?;

        let record = match record {
            Some(r) => r,
            None => {
                tracing::warn!(
                    plugin_id = plugin_id,
                    app_id = %self.deps.app_id,
                    "数据库中未找到插件记录，跳过同步注册"
                );
                return Ok(());
            }
        };

        let local_path = self
            .deps
            .plugin_root
            .join(&self.deps.app_id)
            .join(plugin_id)
            .join(version);

        // 3. 同步文件（仅在本地路径不存在时）
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

        // 4. 注册到 Registry
        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info);
        }

        // 5. 写入 Contexts
        let ctx = PluginContext::from_db_record(&record);
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.insert(plugin_id.to_string(), ctx);
        }

        tracing::info!(
            plugin_id = plugin_id,
            version = version,
            app_id = %self.deps.app_id,
            "插件同步文件并注册到运行时"
        );

        Ok(())
    }

    /// 强制重新同步并注册插件。
    ///
    /// 用于 Reinstalled 场景，不检查本地路径是否存在，
    /// 始终从来源重新同步文件。
    pub async fn force_resync_and_register(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<()> {
        // 1. 查询数据库获取来源信息
        let record = self
            .deps
            .repository
            .find_plugin(plugin_id, &self.deps.app_id)
            .await?;

        let record = match record {
            Some(r) => r,
            None => {
                tracing::warn!(
                    plugin_id = plugin_id,
                    app_id = %self.deps.app_id,
                    "数据库中未找到插件记录，跳过强制同步注册"
                );
                return Ok(());
            }
        };

        let source = build_plugin_source(
            record.zip_source_url.as_deref(),
            record.zip_source_type.as_deref(),
        );

        // 2. 清理本地目录
        let local_path = self
            .deps
            .plugin_root
            .join(&self.deps.app_id)
            .join(plugin_id)
            .join(version);

        if local_path.exists() {
            let _ = tokio::fs::remove_dir_all(&local_path).await;
        }

        // 3. 强制同步文件
        self.sync_plugin_files(plugin_id, version, &source).await?;

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

        // 4. 注册到 Registry
        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info);
        }

        // 5. 写入 Contexts
        let ctx = PluginContext::from_db_record(&record);
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.insert(plugin_id.to_string(), ctx);
        }

        tracing::info!(
            plugin_id = plugin_id,
            version = version,
            app_id = %self.deps.app_id,
            "插件强制重新同步并注册到运行时"
        );

        Ok(())
    }

    /// 从内存注销插件。
    ///
    /// 从 Registry、Contexts 和 Cache 中移除插件信息。
    /// 对不存在的 key 无副作用，天然幂等。
    pub async fn unregister_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        // 1. 从 Registry 移除
        {
            let mut registry = self.deps.registry.write().await;
            registry.unregister(plugin_id);
        }

        // 2. 从 Contexts 移除
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.remove(plugin_id);
        }

        // 3. 从 Cache 删除
        let cache_key = format!("plugin:{}:{}", self.deps.app_id, plugin_id);
        self.deps.cache.delete(&cache_key).await;

        tracing::info!(
            plugin_id = plugin_id,
            app_id = %self.deps.app_id,
            "插件已从运行时注销"
        );

        Ok(())
    }

    /// 注销并清理本地文件。
    ///
    /// 用于 Removed 场景，先从内存注销，再删除本地插件文件目录。
    pub async fn unregister_and_cleanup(&self, plugin_id: &str) -> PluginResult<()> {
        // 1. 从内存注销
        self.unregister_plugin(plugin_id).await?;

        // 2. 删除本地文件目录
        let local_path = self
            .deps
            .plugin_root
            .join(&self.deps.app_id)
            .join(plugin_id);

        if local_path.exists() {
            tokio::fs::remove_dir_all(&local_path).await?;
            tracing::info!(
                plugin_id = plugin_id,
                path = %local_path.display(),
                "已清理插件本地文件"
            );
        }

        Ok(())
    }

    /// 从指定来源同步插件文件到本地目录。
    ///
    /// 使用原子性下载策略：先下载到 `.downloading` 临时目录，
    /// 完成后 rename 到正式目录，确保下载中断不会留下不完整文件。
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
    pub async fn sync_plugin_files(
        &self,
        plugin_id: &str,
        version: &str,
        source: &PluginSource,
    ) -> PluginResult<PathBuf> {
        let target_dir = self
            .deps
            .plugin_root
            .join(&self.deps.app_id)
            .join(plugin_id)
            .join(version);

        if target_dir.exists() {
            return Ok(target_dir);
        }

        let package_path = self
            .package_utils
            .fetch_package(source, None, "运行时同步")
            .await?;

        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            let temp_extract_dir = self
                .deps
                .plugin_root
                .join(".downloading")
                .join(plugin_id)
                .join(version);

            tokio::fs::create_dir_all(&temp_extract_dir).await?;

            self.package_utils.prepare_package_for_validation(
                &package_path,
                &temp_extract_dir,
                "运行时同步",
            )?;

            let plugin_root_in_temp = PackageUtils::find_plugin_root_in_dir(&temp_extract_dir)?;

            if let Some(parent) = target_dir.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            // rename 在跨文件系统（如 tmpfs → 持久卷）时会失败，fallback 到 copy + delete
            if tokio::fs::rename(&plugin_root_in_temp, &target_dir)
                .await
                .is_err()
            {
                tracing::debug!("rename 跨文件系统失败，fallback 到 copy + delete");
                tokio::fs::copy(&plugin_root_in_temp, &target_dir).await?;
                let _ = tokio::fs::remove_dir_all(&plugin_root_in_temp).await;
            }

            let _ = tokio::fs::remove_dir_all(&temp_extract_dir).await;
        } else if package_path.is_dir() {
            if let Some(parent) = target_dir.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::create_dir_all(&target_dir).await?;
        }

        tracing::info!(
            plugin_id = plugin_id,
            version = version,
            path = %target_dir.display(),
            "插件文件已同步到本地"
        );

        Ok(target_dir)
    }
}
