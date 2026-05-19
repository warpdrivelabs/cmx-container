//! 存储管理器模块
//!
//! 管理多个 StorageBackend 实例，提供统一的全局入口。
//! 通过 `DashMap` 实现并发安全的存储后端访问。

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tracing::info;

use crate::backend::{create_backend, StorageBackend};
use crate::config::{StorageInstanceConfig, StorageManagerConfig};
use crate::error::{Error, Result};

/// 存储管理器
///
/// 管理多个存储后端实例，提供统一的全局入口。
/// 支持通过 platform 标识获取对应的存储后端，实现多平台存储的统一管理。
pub struct StorageManager {
    /// 存储后端实例映射（platform -> backend）
    ///
    /// 使用 `DashMap` 实现高效的并发读写。
    backends: DashMap<String, Arc<dyn StorageBackend>>,
    /// 存储实例配置映射（platform -> config）
    configs: HashMap<String, StorageInstanceConfig>,
    /// 默认存储平台标识
    default_platform: Option<String>,
}

impl StorageManager {
    /// 从配置创建存储管理器
    ///
    /// 根据配置文件初始化所有已启用的存储后端实例。
    ///
    /// # Arguments
    ///
    /// * `config` - 存储管理器配置
    ///
    /// # Returns
    ///
    /// 成功时返回初始化后的 `StorageManager` 实例。
    ///
    /// # Errors
    ///
    /// 当任何存储后端初始化失败时返回错误。
    pub fn new(config: &StorageManagerConfig) -> Result<Self> {
        let backends = DashMap::new();
        let mut configs = HashMap::new();

        for instance in config.enabled_instances() {
            info!(platform = %instance.platform, storage_type = ?instance.storage_type, "初始化存储后端");

            match create_backend(instance) {
                Ok(backend) => {
                    backends.insert(instance.platform.clone(), Arc::from(backend));
                    configs.insert(instance.platform.clone(), instance.clone());
                }
                Err(e) => {
                    tracing::error!(platform = %instance.platform, error = %e, "初始化存储后端失败");
                    return Err(e);
                }
            }
        }

        let default_platform = config.get_default_platform().map(|s| s.to_string());

        Ok(Self {
            backends,
            configs,
            default_platform,
        })
    }

    /// 获取指定平台的存储后端
    ///
    /// # Arguments
    ///
    /// * `platform` - 存储平台标识，若为 `None` 则使用默认平台
    ///
    /// # Returns
    ///
    /// 返回对应平台的存储后端实例引用。
    ///
    /// # Errors
    ///
    /// * 当 `platform` 为 `None` 且未配置默认平台时返回 `ConfigError`
    /// * 当指定平台不存在时返回 `NotFoundError`
    pub fn get_backend(&self, platform: Option<&str>) -> Result<Arc<dyn StorageBackend>> {
        let platform = platform
            .or(self.default_platform.as_deref())
            .ok_or_else(|| Error::ConfigError("未配置默认存储平台".to_string()))?;

        self.backends
            .get(platform)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| Error::NotFoundError(format!("存储平台不存在: {}", platform)))
    }

    /// 获取默认存储后端
    ///
    /// # Returns
    ///
    /// 返回默认平台的存储后端实例引用。
    ///
    /// # Errors
    ///
    /// 当未配置默认平台时返回 `ConfigError`。
    pub fn get_default_backend(&self) -> Result<Arc<dyn StorageBackend>> {
        self.get_backend(None)
    }

    /// 获取指定平台的配置
    ///
    /// # Arguments
    ///
    /// * `platform` - 存储平台标识
    ///
    /// # Returns
    ///
    /// 若平台存在则返回对应的配置引用，否则返回 `None`。
    pub fn get_config(&self, platform: &str) -> Option<&StorageInstanceConfig> {
        self.configs.get(platform)
    }

    /// 获取默认平台标识
    ///
    /// # Returns
    ///
    /// 返回默认平台的标识字符串，若未配置则返回 `None`。
    pub fn get_default_platform(&self) -> Option<&str> {
        self.default_platform.as_deref()
    }

    /// 获取所有已注册的平台标识
    ///
    /// # Returns
    ///
    /// 返回所有已初始化存储后端的平台标识列表。
    pub fn get_platforms(&self) -> Vec<String> {
        self.backends.iter().map(|entry| entry.key().clone()).collect()
    }

    /// 检查指定平台是否已注册
    ///
    /// # Arguments
    ///
    /// * `platform` - 存储平台标识
    ///
    /// # Returns
    ///
    /// 若平台已注册则返回 `true`，否则返回 `false`。
    pub fn has_platform(&self, platform: &str) -> bool {
        self.backends.contains_key(platform)
    }

    /// 获取所有启用直接访问的本地存储配置。
    ///
    /// 筛选条件：`storage_type == Local` 且 `enable_access == true`。
    /// 用于为本地存储注册 axum 静态文件服务路由。
    ///
    /// # Returns
    ///
    /// 返回满足条件的配置列表，每项包含 `path_patterns`（路由路径）和 `storage_path`（物理目录）。
    pub fn get_local_access_configs(&self) -> Vec<(&str, &str)> {
        self.configs
            .iter()
            .filter(|(_, cfg)| {
                cfg.storage_type == crate::config::StorageType::Local && cfg.enable_access
            })
            .filter_map(|(_, cfg)| {
                let path_pattern = cfg.path_patterns.as_deref()?;
                let storage_path = cfg.storage_path.as_deref()?;
                Some((path_pattern, storage_path))
            })
            .collect()
    }
}
