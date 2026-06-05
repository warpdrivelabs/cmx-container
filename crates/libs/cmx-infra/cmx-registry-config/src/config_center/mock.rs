//! Mock 配置中心实现。
//!
//! 该模块提供 [`ConfigCenter`] 的内存级实现，仅用于本地开发与单元测试环境。
//! 支持注入预设配置和模拟变更通知，方便在测试环境中验证热更新逻辑。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::ConfigCenterError;

use super::trait_rs::{ConfigCenter, ConfigChangeCallback};

/// Mock 配置中心。
///
/// 内存级实现，使用 `tokio::sync::RwLock<HashMap<>>` 维护配置内容，
/// 使用 `Vec<(data_id, group, callback)>` 维护监听器。
/// 适用于本地开发和单元测试。
pub struct MockConfigCenter {
    /// 配置内容，键为 `"group/data_id"` 格式。
    configs: Arc<RwLock<HashMap<String, String>>>,

    /// 已注册的监听器列表。
    listeners: Arc<RwLock<Vec<(String, String, ConfigChangeCallback)>>>,
}

impl MockConfigCenter {
    /// 创建 Mock 配置中心。
    ///
    /// # Returns
    ///
    /// 返回初始为空的 `MockConfigCenter` 实例。
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注入预设配置（测试用）。
    ///
    /// 在测试代码中预先填充配置内容，使 `get_config` 能够返回预期值。
    ///
    /// # Arguments
    ///
    /// * `data_id` - 配置标识。
    /// * `group` - 配置分组。
    /// * `content` - 配置内容字符串。
    pub async fn set_config(&self, data_id: &str, group: &str, content: &str) {
        let key = format!("{}/{}", group, data_id);
        self.configs.write().await.insert(key, content.to_string());
    }

    /// 模拟配置变更通知（测试用）。
    ///
    /// 手动触发所有匹配 `data_id` 和 `group` 的已注册监听器回调。
    /// 用于验证业务模块的配置热更新逻辑。
    ///
    /// # Arguments
    ///
    /// * `data_id` - 配置标识。
    /// * `group` - 配置分组。
    /// * `new_content` - 新的配置内容，将作为参数传入回调。
    pub async fn simulate_change(&self, data_id: &str, group: &str, new_content: &str) {
        for (did, grp, callback) in self.listeners.read().await.iter() {
            if did == data_id && grp == group {
                callback(new_content);
            }
        }
    }
}

impl Default for MockConfigCenter {
    /// 返回默认（空）Mock 配置中心。
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigCenter for MockConfigCenter {
    /// 从内存中读取指定配置。
    ///
    /// # Errors
    ///
    /// * `ConfigCenterError::GetFailed` - 配置不存在（未通过 [`Self::set_config`] 注入）。
    async fn get_config(&self, data_id: &str, group: &str) -> Result<String, ConfigCenterError> {
        let key = format!("{}/{}", group, data_id);
        self.configs
            .read()
            .await
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                ConfigCenterError::GetFailed(format!("配置不存在: {}", key))
            })
    }

    /// 注册配置变更监听器。
    ///
    /// 监听器不会自动触发，需要测试代码通过 [`Self::simulate_change`] 手动模拟。
    async fn listen(
        &self,
        data_id: &str,
        group: &str,
        callback: ConfigChangeCallback,
    ) -> Result<(), ConfigCenterError> {
        self.listeners
            .write()
            .await
            .push((data_id.to_string(), group.to_string(), callback));
        info!("[MockConfigCenter] 已添加配置监听: {}/{}", group, data_id);
        Ok(())
    }

    /// Mock 实现始终视为已启用。
    fn is_enabled(&self) -> bool {
        true
    }
}
