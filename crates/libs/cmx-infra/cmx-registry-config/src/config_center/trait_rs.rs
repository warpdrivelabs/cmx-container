//! 配置中心 trait 定义

use async_trait::async_trait;
use std::sync::Arc;

use crate::error::ConfigCenterError;

/// 配置变更监听回调（单参数：配置内容）
///
/// listen 方法本身已接收 data_id 和 group，回调只需关心内容变化。
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 配置中心 trait
///
/// 抽象远程配置的获取和变更监听能力。
/// 仅定义通用方法，实现者特有的功能作为具体类型的独立方法提供。
#[async_trait]
pub trait ConfigCenter: Send + Sync {
    /// 获取配置内容
    async fn get_config(&self, data_id: &str, group: &str) -> Result<String, ConfigCenterError>;

    /// 添加配置变更监听器
    async fn listen(
        &self,
        data_id: &str,
        group: &str,
        callback: ConfigChangeCallback,
    ) -> Result<(), ConfigCenterError>;

    /// 检查配置中心是否已启用
    fn is_enabled(&self) -> bool;
}
