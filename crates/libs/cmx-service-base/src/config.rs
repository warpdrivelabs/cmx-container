//! 基础配置加载。
//!
//! [`BaseConfig`] 是解耦于 ConfigManager 的可反序列化结构，承载各微服务共需的基础资源配置
//! （数据库数组 + 可选 Redis）。两个构造器满足两种配置制度：
//! - [`BaseConfig::from_toml_path`]（轻量，default 可用）：直接解析 toml 文件的
//!   `[[databases]]` + 可选 `[redis]` 段。给 flow 这类不用 ConfigManager 的服务。
//! - [`BaseConfig::from_config_manager`]（feature `config-manager`）：读全局 ConfigManager
//!   （CONFIG_FILE toml + Nacos + env），供 portal 复用其既有多源配置。

use std::path::Path;

use cmx_database_pg::DbConfig;

use crate::{BaseError, Result};

/// 微服务基础资源配置。
///
/// `databases` 用 pg 形态（`cmx_database_pg::DbConfig`，跨 workspace 安全）。需要 sqlx 数据源的
/// portal 侧自行把 pg 形态映射回 sqlx 形态（见 `register_sqlx_datasources` 调用点）。
/// 注：`cmx_database_pg::DbConfig` 未实现 Debug，故本结构不 derive Debug。
#[derive(Clone, Default, serde::Deserialize)]
pub struct BaseConfig {
    /// 数据源数组（toml `[[databases]]`）。
    #[serde(default)]
    pub databases: Vec<DbConfig>,
    /// Redis 配置（toml `[redis]`，可选；仅 `redis` feature 下有意义）。
    #[cfg(feature = "redis")]
    #[serde(default)]
    pub redis: Option<cmx_buffer::RedisConfig>,
}

impl BaseConfig {
    /// 轻量加载：解析给定 toml 文件的 `[[databases]]`（+ `[redis]`，若开 `redis` feature）。
    /// 文件不存在 → 返回 Default（空 databases）。供 flow 等不走 ConfigManager 的服务。
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Ok(Self::default()), // 文件缺失：回退空配置（各服务可 env 兜底）
        };
        toml::from_str::<Self>(&text)
            .map_err(|e| BaseError::Config(format!("解析 {} 失败: {e}", path.display())))
    }

    /// 重量加载：从全局 ConfigManager 读 `databases` + `redis`。供 portal 复用其多源配置
    /// （CONFIG_FILE toml + Nacos + env）——调用前 ConfigManager 须已 initialize。
    #[cfg(feature = "config-manager")]
    pub fn from_config_manager() -> Result<Self> {
        let cm = cmx_utils::ConfigManager::global();
        let databases: Vec<DbConfig> = cm
            .get_as("databases")
            .map_err(|e| BaseError::Config(format!("读取 databases 配置失败: {e}")))?;
        #[cfg(feature = "redis")]
        let redis = cm.get_as::<cmx_buffer::RedisConfig>("redis").ok();
        Ok(Self {
            databases,
            #[cfg(feature = "redis")]
            redis,
        })
    }
}
