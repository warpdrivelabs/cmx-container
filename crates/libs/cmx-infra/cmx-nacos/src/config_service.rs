//! Nacos 配置中心客户端封装
//!
//! 封装 nacos_sdk::api::config 的配置获取、监听功能

use std::sync::Arc;

use nacos_sdk::api::config::{ConfigChangeListener, ConfigService};

use crate::config_source::NacosConfigSource;
use crate::error::NacosError;

/// 配置中心客户端
///
/// 封装 Nacos 配置服务，提供配置获取、监听和转换功能
pub struct ConfigClient {
    /// Nacos 配置服务实例
    config_service: ConfigService,
}

impl ConfigClient {
    /// 创建配置中心客户端
    ///
    /// # 参数
    /// - `config_service`: Nacos 配置服务实例
    pub fn new(config_service: ConfigService) -> Self {
        Self { config_service }
    }

    /// 获取远程配置内容
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    ///
    /// # 返回值
    /// 成功返回配置内容字符串，失败返回 NacosError
    pub async fn get_config(&self, data_id: &str, group: &str) -> Result<String, NacosError> {
        let response = self
            .config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .map_err(|e| NacosError::ConfigGetFailed(e.to_string()))?;
        Ok(response.content().to_string())
    }

    /// 获取远程配置并转换为 NacosConfigSource
    ///
    /// 远程配置内容应为 TOML 格式，会被解析为 config::Value 树，
    /// 可通过 ConfigBuilder::add_source() 注入配置系统。
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    ///
    /// # 返回值
    /// 成功返回 NacosConfigSource 实例，失败返回 NacosError
    pub async fn get_config_source(
        &self,
        data_id: &str,
        group: &str,
    ) -> Result<NacosConfigSource, NacosError> {
        let content = self.get_config(data_id, group).await?;
        NacosConfigSource::from_toml_str(&content)
    }

    /// 添加配置变更监听器
    ///
    /// # 参数
    /// - `data_id`: 配置标识
    /// - `group`: 配置分组
    /// - `listener`: 配置变更监听器（Arc 包装）
    pub async fn add_listener(
        &self,
        data_id: &str,
        group: &str,
        listener: Arc<dyn ConfigChangeListener>,
    ) -> Result<(), NacosError> {
        self.config_service
            .add_listener(data_id.to_string(), group.to_string(), listener)
            .await
            .map_err(|e| NacosError::ConfigListenFailed(e.to_string()))?;
        tracing::info!("已添加配置监听: {}/{}", group, data_id);
        Ok(())
    }
}
