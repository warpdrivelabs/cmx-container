//! Nacos 配置中心适配器

use async_trait::async_trait;
use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse, ConfigServiceBuilder};
use nacos_sdk::api::props::ClientProps;
use std::sync::Arc;
use tracing::info;

use crate::config::NacosConfigCenterConfig;
use crate::error::ConfigCenterError;

use super::trait_rs::{ConfigCenter, ConfigChangeCallback};

/// 适配器：将 ConfigChangeCallback 适配为 nacos_sdk 的 ConfigChangeListener
struct NacosListenerAdapter {
    callback: ConfigChangeCallback,
}

impl ConfigChangeListener for NacosListenerAdapter {
    fn notify(&self, config_resp: ConfigResponse) {
        info!(
            "收到 Nacos 配置变更通知: data_id={}, group={}",
            config_resp.data_id(),
            config_resp.group()
        );
        (self.callback)(config_resp.content());
    }
}

/// Nacos 配置中心实现
pub struct NacosConfigCenter {
    config_service: nacos_sdk::api::config::ConfigService,
}

impl NacosConfigCenter {
    /// 创建 Nacos 配置中心实例
    pub fn new(config: &NacosConfigCenterConfig) -> Result<Self, ConfigCenterError> {
        let mut client_props = ClientProps::new()
            .server_addr(&config.server_addr)
            .namespace(&config.namespace)
            .app_name(&config.app_name);

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client_props = client_props.auth_username(username).auth_password(password);
        }

        let config_service = ConfigServiceBuilder::new(client_props)
            .build()
            .map_err(|e| {
                ConfigCenterError::InitFailed(format!("配置中心初始化失败: {}", e))
            })?;

        info!("Nacos 配置中心初始化成功");

        Ok(Self { config_service })
    }

    /// Nacos 特有：获取远程配置并解析为 config::Value
    ///
    /// 远程配置内容应为 TOML 格式。
    pub async fn get_config_as_source(
        &self,
        data_id: &str,
        group: &str,
    ) -> Result<config::Value, ConfigCenterError> {
        let content = self.get_config(data_id, group).await?;
        let toml_value: toml::Value = toml::from_str(&content)
            .map_err(|e| ConfigCenterError::ParseFailed(format!("TOML 解析失败: {}", e)))?;
        Self::toml_to_config_value(toml_value)
    }

    fn toml_to_config_value(toml_val: toml::Value) -> Result<config::Value, ConfigCenterError> {
        use std::collections::HashMap as StdMap;
        match toml_val {
            toml::Value::Table(table) => {
                let mut map = StdMap::new();
                for (k, v) in table {
                    map.insert(k, Self::toml_to_config_value(v)?);
                }
                Ok(config::Value::new(None, map))
            }
            other => Ok(Self::toml_primitive_to_value(other)),
        }
    }

    fn toml_primitive_to_value(toml_val: toml::Value) -> config::Value {
        match toml_val {
            toml::Value::String(s) => config::Value::new(None, s),
            toml::Value::Integer(i) => config::Value::new(None, i),
            toml::Value::Float(f) => config::Value::new(None, f),
            toml::Value::Boolean(b) => config::Value::new(None, b),
            toml::Value::Array(arr) => {
                let vec: Vec<config::Value> =
                    arr.into_iter().map(Self::toml_primitive_to_value).collect();
                config::Value::new(None, vec)
            }
            toml::Value::Datetime(dt) => config::Value::new(None, dt.to_string()),
            toml::Value::Table(_) => unreachable!(),
        }
    }
}

#[async_trait]
impl ConfigCenter for NacosConfigCenter {
    async fn get_config(&self, data_id: &str, group: &str) -> Result<String, ConfigCenterError> {
        let response = self
            .config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .map_err(|e| ConfigCenterError::GetFailed(e.to_string()))?;

        Ok(response.content().to_string())
    }

    async fn listen(
        &self,
        data_id: &str,
        group: &str,
        callback: ConfigChangeCallback,
    ) -> Result<(), ConfigCenterError> {
        let adapter = Arc::new(NacosListenerAdapter { callback });
        self.config_service
            .add_listener(data_id.to_string(), group.to_string(), adapter)
            .await
            .map_err(|e| ConfigCenterError::ListenFailed(e.to_string()))?;

        info!("已添加配置监听: {}/{}", group, data_id);
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        true
    }
}
