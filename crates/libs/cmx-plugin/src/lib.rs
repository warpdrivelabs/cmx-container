/*
 * @Author: yqs
 * @Date: 2026-03-16 15:30:35
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-18 07:54:13
 */
//! cmx-plugin — 插件注册表、ZIP 加载、签名验证、生命周期管理
//!
//! 基础结构体（PluginDefinition、PluginManifest 等）定义在 cmx-core 中。
//! 本 crate 提供插件管理的具体实现。

pub mod error;
pub mod registry;
pub mod types;
pub mod version;
pub mod fetcher;
pub mod manager;
pub mod deployment;
pub mod activation;
pub mod audit;
pub mod security;
pub mod repository;
pub mod transaction;
pub mod db;
pub mod db_impl;
pub mod config;
pub mod cache;
pub mod service;
pub mod permission;
pub mod node;
pub mod message;
pub mod memory_cache;

pub use error::PluginError;
pub use registry::{PluginRegistry, VerifySignatureConfig};
pub use types::{
    ActivateRequest, ActivateResponse, DeactivateRequest, DeactivateResponse,
    DefaultPluginsConfig, DependencyCheckResult, DependencyResolution, DeploymentStatus,
    DeploymentStrategy, DeployRequest, DowngradeRequest, DowngradeResponse, InitResult, InstallRequest,
    InstallResponse, MissingDependency, NodeDeploymentResult, OperationStatus, OperationType,
    PluginConfig, PluginDatabaseConfig, PluginFilter, PluginInfo, PluginManagerConfig,
    PluginSource, PluginStatus, RollbackRequest, RollbackResponse, SettingsConfig,
    SystemPluginConfig, UninstallRequest, UninstallResponse, UpgradePath, UpgradeRequest,
    UpgradeResponse, VersionRelation,
};
pub use version::{
    SemanticVersion, PreRelease, VersionManager, DependencyResolver,
    VersionConstraint, VersionConstraintParser,
};
pub use manager::PluginManager;
pub use deployment::{DeploymentCoordinator, DeploymentNodeInfo, DeploymentNodeStatus, DeployResult, SyncResult, SyncStatus};
pub use activation::ActivationManager;
pub use audit::{AuditLogger, AuditLogBuilder, AuditLogEntry, AuditLogFilter, AuditLogPageResult};
pub use security::{SecurityValidator, SecurityValidatorConfig, ValidationResult, CheckResult};
pub use transaction::{TransactionManager, TransactionManagerConfig, TransactionContext, TransactionGuard, TransactionState};
pub use db::{PluginDbService, PluginDatabase, PluginDbRecord, PluginUpdateFields, VersionDbRecord, AuditDbRecord, DeploymentDbRecord, RollbackDbRecord, PluginDbError};
pub use db_impl::CmxPluginDatabase;
pub use config::{SystemPluginsConfig, PluginSettings, RequiredPlugin, OptionalPlugin, PluginSourceConfig, ConfigError};
pub use cache::{PluginCacheManager, PluginCacheKey, PluginCacheValue, PluginCacheError, CACHE_KEY_PREFIX};
pub use fetcher::{PluginSourceFetcher, RegistryConfig, RegistryPluginInfo, RegistryPluginVersion};
pub use service::{ServiceRegistry, ServiceDescriptor, ServiceInstance, ServiceHandle, ServiceCallRequest, ServiceCallResponse, ServiceError};
pub use permission::{PermissionChecker, Permission, PermissionType, PermissionPolicy, PermissionCheckResult, PermissionError, PluginPermissionConfig};
pub use node::{NodeManager, NodeManagerConfig, NodeInfo, NodeState, NodeType, NodeCapability, NodeStats, NodeSelectionStrategy, NodeError};
pub use message::{
    MessageQueue, MessageQueueConfig, MessageQueueBuilder, MessageQueueError,
    Message, PluginEvent, PluginEventType, DeploymentEvent, DeploymentEventType,
    NodeEvent, NodeEventType, SystemEvent, SystemEventType,
    CHANNEL_PLUGIN_EVENTS, CHANNEL_DEPLOYMENT_EVENTS, CHANNEL_NODE_EVENTS, CHANNEL_SYSTEM_EVENTS,
};
pub use memory_cache::{
    MemoryCache, MemoryCacheConfig, MemoryCacheStats,
    PluginMemoryCache, MemoryCacheValue, PluginMemoryCacheManager, CacheKeyBuilder,
};

pub use cmx_core::model::meta::plugin::{
    PluginDefinition, PluginManifest, PluginManifestSigningPayload,
    supported_db, supported_lang,
};
