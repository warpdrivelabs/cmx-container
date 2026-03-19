/*
 * @Author: yqs
 * @Date: 2026-03-16 15:30:35
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-19 10:00:00
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
pub mod core;
pub mod domain;
pub mod service;
pub mod infrastructure;
pub mod cluster;
pub mod security;
pub mod runtime;
pub mod config;
pub mod audit;
pub mod fetcher;
pub mod error;

// 导出错误类型
pub use error::PluginError;

// 导出 cmx-core 中的基础类型
pub use cmx_core::model::meta::plugin::{
    PluginDefinition, PluginManifest, PluginManifestSigningPayload,
    supported_db, supported_lang,
};

// 导出核心模块类型
pub use core::manager::PluginManager;
pub use core::registry::PluginRegistry;
pub use core::context::PluginContext;
pub use core::lifecycle::{LifecycleState, LifecycleStateMachine};

// 导出领域模块类型
pub use domain::plugin::{PluginInfo, PluginSource, PluginStatus, PluginFilter, PluginConfig, PluginDatabaseConfig};
pub use domain::version::{SemanticVersion, PreRelease, VersionConstraint, VersionRelation, VersionParseError};
pub use domain::dependency::{DependencyCheckResult, DependencyResolution, DependencyGraph, DependencyNode, Dependency, MissingDependency, DependencyConflict};
pub use domain::status::{PluginStatus as DomainPluginStatus, StatusTransition};

// 导出服务模块类型
pub use service::install::InstallService;
pub use service::uninstall::UninstallService;
pub use service::activate::ActivateService;
pub use service::upgrade::UpgradeService;
pub use service::downgrade::DowngradeService;
pub use service::rollback::RollbackService;

// 导出基础设施模块类型
pub use infrastructure::database::schema::SchemaManager;
pub use infrastructure::database::repository::PluginRepository;
pub use infrastructure::database::migration::{MigrationManager, MigrationStatus};
pub use infrastructure::cache::memory::MemoryCache;
pub use infrastructure::cache::layered::{LayeredCacheManager, CacheValue, CacheStrategy};
pub use infrastructure::storage::file::FileStorage;
pub use infrastructure::storage::backup::{BackupManager, BackupInfo};
pub use infrastructure::messaging::queue::{MessageQueue, Message, MessageQueueManager};
pub use infrastructure::messaging::event::{EventBus, Event, EventType};

// 导出集群模块类型
pub use cluster::node::{NodeManager, NodeInfo, NodeStatus};
pub use cluster::deployment::{DeploymentCoordinator, DeploymentStrategy, DeploymentStatus, DeploymentTask};
pub use cluster::sync::{SyncManager, PluginStateRecord};

// 导出安全模块类型
pub use security::validator::{SecurityValidator, ValidationResult};
pub use security::signature::SignatureValidator;
pub use security::permission::{PermissionManager, Permission};

// 导出运行时模块类型
pub use runtime::activation::ActivationManager;
pub use runtime::service_registry::{ServiceRegistry, ServiceDefinition, ServiceHandle};
pub use runtime::feature::{FeatureManager, Feature, FeatureType};

// 导出配置模块类型
pub use config::settings::{PluginManagerSettings, CacheSettings, ClusterSettings, PluginSettings};
pub use config::loader::ConfigLoader;

// 导出审计模块类型
pub use audit::logger::AuditLogger;
pub use audit::record::{AuditRecord, OperationType as AuditOperationType, OperationResult};

// 导出获取器模块类型
pub use fetcher::source::PluginSource as FetcherPluginSource;
pub use fetcher::local::LocalFetcher;
pub use fetcher::remote::RemoteFetcher;
pub use fetcher::registry::{RegistryFetcher, RegistryInfo};
