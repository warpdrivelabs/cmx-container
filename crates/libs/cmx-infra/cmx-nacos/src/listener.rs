//! Nacos 配置变更监听器
//!
//! 实现 nacos_sdk 的 ConfigChangeListener trait，
//! 在远程配置变更时通知 ConfigChangeNotifier

use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse};
use tracing::info;

/// 远程配置变更监听器
///
/// 当 Nacos 远程配置发生变更时，将新配置内容
/// 通过 ConfigChangeNotifier 通知给所有注册的处理器
pub struct RemoteConfigChangeListener;

impl ConfigChangeListener for RemoteConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        info!(
            "收到 Nacos 配置变更通知: data_id={}, group={}, md5={}",
            config_resp.data_id(),
            config_resp.group(),
            config_resp.md5()
        );

        let content = config_resp.content();
        crate::notifier::GlobalConfigChangeNotifier::notify(content);
    }
}
