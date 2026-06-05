//! Nacos 配置中心适配器。
//!
//! 该模块基于 `nacos-sdk` 实现 [`ConfigCenter`] trait，
//! 提供与 Nacos 配置中心的远程配置获取和变更监听能力。
//!
//! # SDK 适配
//!
//! 通过 `NacosListenerAdapter` 将 `nacos-sdk` 的 `ConfigChangeListener` 适配为
//! cmx-container 的 [`ConfigChangeCallback`]，解耦 SDK 与应用层回调签名差异。

use async_trait::async_trait;
use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse, ConfigServiceBuilder};
use nacos_sdk::api::props::ClientProps;
use std::sync::Arc;
use tracing::info;

use crate::config::NacosConfigCenterConfig;
use crate::error::ConfigCenterError;

use super::trait_rs::{ConfigCenter, ConfigChangeCallback};

/// 适配器：将 `ConfigChangeCallback` 适配为 `nacos_sdk` 的 `ConfigChangeListener`。
///
/// SDK 的 `notify` 方法传入 `ConfigResponse`，从其中提取 `content()` 后
/// 调用应用层回调。
struct NacosListenerAdapter {
    /// 应用层配置变更回调。
    callback: ConfigChangeCallback,
}

impl ConfigChangeListener for NacosListenerAdapter {
    /// SDK 推送的变更入口：提取内容后调用应用层回调。
    fn notify(&self, config_resp: ConfigResponse) {
        info!(
            "收到 Nacos 配置变更通知: data_id={}, group={}",
            config_resp.data_id(),
            config_resp.group()
        );
        (self.callback)(config_resp.content());
    }
}

/// Nacos 配置中心实现。
///
/// 内部持有 `nacos-sdk` 的 `ConfigService` 句柄。
pub struct NacosConfigCenter {
    /// nacos-sdk 配置服务客户端。
    config_service: nacos_sdk::api::config::ConfigService,
}

impl NacosConfigCenter {
    /// 创建 Nacos 配置中心实例。
    ///
    /// 构造 `ClientProps` 并构建 `ConfigService` 客户端。
    /// 如配置了用户名和密码则启用认证。
    ///
    /// # Arguments
    ///
    /// * `config` - Nacos 配置中心连接配置。
    ///
    /// # Returns
    ///
    /// * `Ok(NacosConfigCenter)` - 初始化成功。
    /// * `Err(ConfigCenterError::InitFailed)` - nacos-sdk 客户端构建失败。
    pub fn new(config: &NacosConfigCenterConfig) -> Result<Self, ConfigCenterError> {
        let mut client_props = ClientProps::new()
            .server_addr(&config.server_addr)
            .namespace(&config.namespace)
            .app_name(&config.app_name);

        // 同时配置用户名和密码时才启用认证。
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

    /// Nacos 特有：获取远程配置并解析为 `config::Value`。
    ///
    /// 远程配置内容应为 TOML 格式。
    /// 该方法是 [`ConfigCenter::get_config`] 的便利封装，
    /// 适合直接接入 [`ConfigBuilder::add_source`](cmx_utils::ConfigBuilder::add_source)。
    ///
    /// # Arguments
    ///
    /// * `data_id` - 配置标识。
    /// * `group` - 配置分组。
    ///
    /// # Returns
    ///
    /// 成功时返回 `config::Value` 树，可直接被 config-rs 消费。
    ///
    /// # Errors
    ///
    /// * `ConfigCenterError::GetFailed` - 配置获取失败。
    /// * `ConfigCenterError::ParseFailed` - TOML 解析失败。
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

    /// 递归将 `toml::Value` 转换为 `config::Value`。
    ///
    /// `Table` 类型递归处理；其他类型通过 [`Self::toml_primitive_to_value`] 转换。
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

    /// 将 TOML 原始类型转换为 `config::Value`。
    ///
    /// `Table` 类型在此函数中不可达，由 [`Self::toml_to_config_value`] 单独处理。
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
    /// 通过 nacos-sdk 拉取远程配置。
    async fn get_config(&self, data_id: &str, group: &str) -> Result<String, ConfigCenterError> {
        let response = self
            .config_service
            .get_config(data_id.to_string(), group.to_string())
            .await
            .map_err(|e| ConfigCenterError::GetFailed(e.to_string()))?;

        Ok(response.content().to_string())
    }

    /// 注册配置变更监听器。
    ///
    /// 通过 `NacosListenerAdapter` 将应用层回调注入 nacos-sdk。
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

    /// Nacos 实现始终视为已启用。
    fn is_enabled(&self) -> bool {
        true
    }
}
