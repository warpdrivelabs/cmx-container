//! Mock 配置中心实现

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::ConfigCenterError;

use super::trait_rs::{ConfigCenter, ConfigChangeCallback};

/// Mock 配置中心
///
/// 内存级实现，用于开发和测试环境。支持注入预设配置和模拟变更通知。
pub struct MockConfigCenter {
    configs: Arc<RwLock<HashMap<String, String>>>,
    listeners: Arc<RwLock<Vec<(String, String, ConfigChangeCallback)>>>,
}

impl MockConfigCenter {
    /// 创建 Mock 配置中心
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注入预设配置（测试用）
    pub async fn set_config(&self, data_id: &str, group: &str, content: &str) {
        let key = format!("{}/{}", group, data_id);
        self.configs.write().await.insert(key, content.to_string());
    }

    /// 模拟配置变更通知（测试用）
    pub async fn simulate_change(&self, data_id: &str, group: &str, new_content: &str) {
        for (did, grp, callback) in self.listeners.read().await.iter() {
            if did == data_id && grp == group {
                callback(new_content);
            }
        }
    }
}

impl Default for MockConfigCenter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigCenter for MockConfigCenter {
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

    fn is_enabled(&self) -> bool {
        true
    }
}
