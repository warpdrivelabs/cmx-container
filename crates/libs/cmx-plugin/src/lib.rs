/*
 * @Author: yqs
 * @Date: 2026-03-16 15:30:35
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-20 01:18:41
 */
//! cmx-plugin — 插件注册表、ZIP 加载、签名验证、生命周期管理
//!
//! 基础结构体（PluginDefinition、PluginManifest 等）定义在 cmx-core 中。
//! 本 crate 提供插件管理的具体实现。
//!
//! # 模块结构
//!
//! - `core`: 核心模块，包含插件管理器、注册表、上下文和生命周期管理
//! - `domain`: 领域模型，包含插件定义、版本、依赖和状态
//! - `service`: 服务层，包含安装、卸载、激活、升级、降级和回滚服务
//! - `infrastructure`: 基础设施层，包含数据库、缓存、存储和消息
//! - `cluster`: 集群模块，包含节点管理、部署协调和状态同步
//! - `security`: 安全模块，包含验证器、签名和权限管理
//! - `runtime`: 运行时模块，包含激活管理、服务注册表和功能管理
//! - `config`: 配置模块，包含配置设置和加载器
//! - `audit`: 审计模块，包含日志记录器和审计记录
//! - `fetcher`: 获取器模块，包含来源定义和各类获取器

// 模块声明
pub mod audit;
pub mod cluster;
pub mod common;
pub mod config;
pub mod core;
pub mod domain;
pub mod error;
pub mod fetcher;
pub mod host_functions;
pub mod infrastructure;
pub mod marketplace;
pub mod runtime;
pub mod security;
pub mod service;
pub mod traits_impl;

// 导出错误类型
pub use error::PluginError;

// 导出 cmx-core 中的基础类型
pub use cmx_core::model::meta::plugin::{
    PluginDefinition, PluginManifest, PluginManifestSigningPayload, supported_db, supported_lang,
};

// 导出核心模块类型
pub use core::context::PluginContext;
pub use core::manager::PluginManager;
pub use core::manager::PluginManagerBuilder;
pub use core::registry::PluginRegistry;

// 导出领域模块类型
pub use domain::dependency::{
    Dependency, DependencyCheckResult, DependencyConflict, DependencyResolution, MissingDependency,
};
pub use domain::plugin::{
    PluginConfig, PluginDatabaseConfig, PluginFilter, PluginInfo, PluginSource, PluginStatus,
};
pub use domain::version::{
    PreRelease, SemanticVersion, VersionConstraint, VersionParseError, VersionRelation,
};

// 导出服务模块类型
pub use service::deploy::{DeployAction, DeployRequest, DeployResponse};
pub use service::downgrade::DowngradeService;
pub use service::install::InstallService;
pub use service::uninstall::UninstallService;
pub use service::upgrade::UpgradeService;
// pub use service::control::{
//     ControlService, ControlDeployRequest, ControlDeployResponse, ControlInstallRequest,
//     ControlUpgradeRequest, ControlDowngradeRequest, ControlUninstallRequest,
// };
pub use service::auto_install::{
    AutoInstallConfig, AutoInstallPlugin, AutoInstallResult, AutoInstallService, InstallAction,
};

// 导出基础设施模块类型
pub use infrastructure::cache::layered::{CacheStrategy, CacheValue, LayeredCacheManager};
pub use infrastructure::cache::memory::MemoryCache;
pub use infrastructure::database::plugin::PluginRepository;
pub use infrastructure::database::schema::SchemaManager;
pub use infrastructure::storage::backup::{BackupInfo, BackupManager};
pub use infrastructure::storage::file::FileStorage;

// 导出集群模块类型
pub use cluster::node::{NodeInfo, NodeManager, NodeStatus};
pub use cluster::notification::{PluginChangeAction, PluginChangeNotification, PluginNotifier};

// 导出安全模块类型
pub use security::signature::SignatureValidator;
pub use security::validator::{SecurityValidator, ValidationResult};

// 导出运行时模块类型
pub use runtime::service_registry::{ServiceDefinition, ServiceHandle, ServiceRegistry};

// 导出配置模块类型
pub use config::loader::ConfigLoader;
pub use config::settings::{CacheSettings, ClusterSettings, PluginManagerSettings, PluginSettings};

// 导出审计模块类型
pub use audit::logger::AuditLogger;
pub use audit::record::{AuditRecord, OperationResult, OperationType as AuditOperationType};

// 导出获取器模块类型
pub use fetcher::local::LocalFetcher;
pub use fetcher::marketplace_fetcher::{
    MarketplaceFetcher, MarketplacePackageDetail, MarketplaceSearchResult, MarketplaceSourceInfo,
};
pub use fetcher::remote::RemoteFetcher;
pub use fetcher::source::PluginSource as FetcherPluginSource;

pub use host_functions::PluginHostFunctions;

// 导出插件市场模块类型
pub use marketplace::model::{
    CategoryInfo, MarketplaceDownloadStats, MarketplacePlugin, MarketplacePluginBmc,
    MarketplacePluginFilter, MarketplacePluginForCreate, MarketplacePluginForUpdate,
    MarketplacePluginVersion, MarketplacePluginVersionBmc, MarketplacePluginVersionFilter,
    MarketplacePluginVersionForCreate, MarketplaceRating, MarketplaceRatingBmc,
    MarketplaceRatingFilter, MarketplaceRatingForCreate,
};
pub use marketplace::repository::MarketplaceRepository;
pub use marketplace::service::MarketplaceService;
pub use marketplace::stats::StatsService;

// ==================== 全局单例 ====================

use std::sync::{Arc, OnceLock};

/// 全局插件管理器
///
/// 提供应用级别的单例访问，确保整个应用共享同一个 PluginManager 实例。
///
/// # 设计说明
///
/// `PluginManager` 内部已使用细粒度锁（如 `registry: Arc<RwLock<...>>`），
/// 所有公共方法都使用 `&self`，实现了内部可变性。因此外层不需要 `RwLock`。
///
/// # 使用示例
///
/// ```rust,no_run
/// use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
/// use cmx_database::DatabaseManager;
/// use cmx_buffer::{CacheManager, LockManager, PubSubOps, RedisClient, RedisConfig};
/// use std::sync::Arc;
///
/// async fn init() {
///     // 方式1：使用默认配置初始化
///     GlobalPluginManager::initialize(Default::default()).await.unwrap();
///     
///     // 方式2：使用自定义配置初始化
///     let settings = PluginManagerSettings {
///         plugin_root: std::path::PathBuf::from("./plugins"),
///         ..Default::default()
///     };
///     GlobalPluginManager::initialize(settings).await.unwrap();
///     
///     // 方式3：注入外部依赖
///     let db_manager = Arc::new(DatabaseManager::new(Default::default()));
///     let cache_manager = Arc::new(CacheManager::new(
///         RedisClient::new(RedisConfig::default()).await.unwrap(),
///     ));
///     GlobalPluginManager::initialize_with_deps(
///         Default::default(),
///         Some(db_manager),
///         Some(cache_manager),
///         None,
///         None,
///     ).await.unwrap();
///     
///     // 获取全局实例
///     let manager = GlobalPluginManager::get();
/// }
/// ```
pub struct GlobalPluginManager;

/// 全局插件管理器单例
///
/// 注意：`PluginManager` 内部已实现细粒度锁，外层不需要 `RwLock`。
static GLOBAL_PLUGIN_MANAGER: OnceLock<Arc<PluginManager>> = OnceLock::new();

impl GlobalPluginManager {
    /// 初始化全局插件管理器
    ///
    /// 使用默认配置创建并初始化全局 PluginManager 实例。
    ///
    /// # 参数
    /// * `settings` - 插件管理器配置
    ///
    /// # 返回值
    /// * `Ok(())` - 初始化成功
    /// * `Err(PluginError)` - 初始化失败
    ///
    /// # 错误
    /// 如果已经初始化，将返回错误。
    pub async fn initialize(settings: PluginManagerSettings) -> error::PluginResult<()> {
        let manager = PluginManager::new(settings).await?;

        GLOBAL_PLUGIN_MANAGER
            .set(Arc::new(manager))
            .map_err(|_| error::PluginError::Plugin("全局插件管理器已初始化".to_string()))?;

        Ok(())
    }

    /// 使用外部依赖初始化全局插件管理器
    ///
    /// 允许注入外部创建的数据库管理器、缓存管理器等依赖。
    ///
    /// # 参数
    /// * `settings` - 插件管理器配置
    /// * `db_manager` - 数据库管理器（可选，如不提供将使用默认配置创建）
    /// * `cache_manager` - 缓存管理器（可选，如不提供将使用默认配置创建）
    /// * `lock_manager` - 分布式锁管理器（可选）
    /// * `pubsub` - 消息订阅发布（可选）
    ///
    /// # 返回值
    /// * `Ok(())` - 初始化成功
    /// * `Err(PluginError)` - 初始化失败
    ///
    /// # 示例
    /// ```rust,no_run
/// use cmx_plugin::GlobalPluginManager;
/// use cmx_database::DatabaseManager;
/// use cmx_buffer::{CacheManager, RedisClient, RedisConfig};
/// use std::sync::Arc;
///
/// async fn init() {
///     let db = Arc::new(DatabaseManager::new(Default::default()));
///     let cache = Arc::new(CacheManager::new(
///         RedisClient::new(RedisConfig::default()).await.unwrap(),
///     ));
    ///     
    ///     GlobalPluginManager::initialize_with_deps(
    ///         Default::default(),
    ///         Some(db),
    ///         Some(cache),
    ///         None,
    ///         None,
    ///     ).await.unwrap();
    /// }
    /// ```
    pub async fn initialize_with_deps(
        settings: PluginManagerSettings,
        db_manager: Option<Arc<cmx_database::DatabaseManager>>,
        cache_manager: Option<Arc<cmx_buffer::CacheManager>>,
        lock_manager: Option<Arc<cmx_buffer::LockManager>>,
        pubsub: Option<Arc<cmx_buffer::PubSubOps>>,
    ) -> error::PluginResult<()> {
        let mut builder = core::manager::PluginManagerBuilder::new(settings);

        if let Some(db) = db_manager {
            builder = builder.with_database(db);
        }
        if let Some(cache) = cache_manager {
            builder = builder.with_cache(cache);
        }
        if let Some(lock) = lock_manager {
            builder = builder.with_lock_manager(lock);
        }
        if let Some(ps) = pubsub {
            builder = builder.with_pubsub(ps);
        }

        let manager = builder.build().await?;

        GLOBAL_PLUGIN_MANAGER
            .set(Arc::new(manager))
            .map_err(|_| error::PluginError::Plugin("全局插件管理器已初始化".to_string()))?;

        Ok(())
    }

    /// 获取全局插件管理器引用
    ///
    /// 返回 `&'static PluginManager`，直接访问插件管理器。
    /// 由于 `PluginManager` 内部使用细粒度锁，此方法不需要 `await`。
    ///
    /// # 返回值
    /// * `&'static PluginManager` - 插件管理器静态引用
    ///
    /// # Panics
    /// 如果未初始化则 panic，请确保先调用 `initialize` 或 `initialize_with_deps`。
    pub fn get() -> &'static PluginManager {
        GLOBAL_PLUGIN_MANAGER.get().expect(
            "插件管理器未初始化，请先调用 GlobalPluginManager::initialize() 或 GlobalPluginManager::initialize_with_deps()"
        )
    }

    /// 检查是否已初始化
    ///
    /// # 返回值
    /// * `bool` - 是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_PLUGIN_MANAGER.get().is_some()
    }

    /// 获取全局插件管理器 Arc 引用
    ///
    /// 返回 Arc 引用，允许在异步任务中共享所有权。
    ///
    /// # 返回值
    /// * `Arc<PluginManager>` - 插件管理器 Arc 引用
    ///
    /// # Panics
    /// 如果未初始化则 panic。
    pub fn get_arc() -> Arc<PluginManager> {
        GLOBAL_PLUGIN_MANAGER.get().expect(
            "插件管理器未初始化，请先调用 GlobalPluginManager::initialize() 或 GlobalPluginManager::initialize_with_deps()"
        ).clone()
    }

    /// 获取全局插件管理器作为 PluginQuery trait 对象
    ///
    /// 返回 `Arc<dyn PluginQuery>`，用于依赖注入场景。
    ///
    /// # 返回值
    /// * `Arc<dyn PluginQuery>` - 插件查询接口的 trait 对象
    ///
    /// # Panics
    /// 如果未初始化则 panic。
    pub fn get_as_plugin_query() -> Arc<dyn cmx_traits::plugin::PluginQuery> {
        Self::get_arc()
    }

    /// 关闭全局插件管理器
    ///
    /// 执行清理操作，包括停用所有插件、释放资源等。
    ///
    /// # 返回值
    /// * `Ok(())` - 关闭成功
    /// * `Err(PluginError)` - 关闭失败
    pub async fn shutdown() -> error::PluginResult<()> {
        let manager = Self::get();
        manager.shutdown().await
    }
}
