//! 配置中心 trait 定义。
//!
//! 该模块定义配置中心的抽象接口。
//! 所有具体实现（Nacos、Mock、未来的 Apollo）都必须实现 [`ConfigCenter`]。

use async_trait::async_trait;
use std::sync::Arc;

use crate::error::ConfigCenterError;

/// 配置变更监听回调（单参数：配置内容）。
///
/// `listen` 方法本身已接收 `data_id` 和 `group`，回调只需关心内容变化。
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 配置中心 trait。
///
/// 抽象远程配置的获取和变更监听能力。
/// 仅定义通用方法；实现者特有的功能作为具体类型的独立方法提供
/// （如 `NacosConfigCenter::get_config_as_source`）。
///
/// # 与变更通知的集成
///
/// 收到 SDK 推送后通常应调用 [`GlobalChangeNotifier::notify`](crate::GlobalChangeNotifier::notify)，
/// 由全局通知器完成 handlers 的多路分发。
#[async_trait]
pub trait ConfigCenter: Send + Sync {
    /// 获取配置内容。
    ///
    /// # Arguments
    ///
    /// * `data_id` - 配置标识。
    /// * `group` - 配置分组。
    ///
    /// # Returns
    ///
    /// 成功时返回配置内容字符串（约定为 TOML 格式）。
    ///
    /// # Errors
    ///
    /// * `ConfigCenterError::GetFailed` - 配置不存在或网络错误。
    async fn get_config(&self, data_id: &str, group: &str) -> Result<String, ConfigCenterError>;

    /// 添加配置变更监听器。
    ///
    /// 注册后，配置中心 SDK 会在指定配置发生变更时调用 `callback`。
    ///
    /// # Arguments
    ///
    /// * `data_id` - 配置标识。
    /// * `group` - 配置分组。
    /// * `callback` - 变更回调函数，接收新的配置内容。
    ///
    /// # Errors
    ///
    /// * `ConfigCenterError::ListenFailed` - SDK 注册监听失败。
    async fn listen(
        &self,
        data_id: &str,
        group: &str,
        callback: ConfigChangeCallback,
    ) -> Result<(), ConfigCenterError>;

    /// 检查配置中心是否已启用。
    ///
    /// # Returns
    ///
    /// * `true` - 配置中心已启用。
    /// * `false` - 配置中心被禁用。
    fn is_enabled(&self) -> bool;
}
