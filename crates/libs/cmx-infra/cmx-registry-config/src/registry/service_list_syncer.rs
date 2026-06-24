//! 服务列表定时同步器。
//!
//! 定期轮询注册中心获取服务名列表，发现新服务时自动建立实例订阅，
//! 确保服务实例缓存始终覆盖所有已注册服务。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tracing::{debug, info, warn, instrument};

use super::instance_cache::ServiceInstanceCache;
use super::trait_rs::{InstanceChangeCallback, ServiceInstance, ServiceRegistry};

/// 服务列表定时同步器。
///
/// 定期从注册中心拉取服务名列表，与本地已知服务对比：
/// - 发现新服务时，自动调用 `subscribe_instances()` 建立订阅和缓存
/// - 服务下线时，仅清理缓存（不取消订阅，因为 Nacos 不支持 unsubscribe）
pub struct ServiceListSyncer {
    /// 注册中心实例
    registry: Arc<dyn ServiceRegistry>,
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// 已订阅的服务名集合
    subscribed_services: Arc<std::sync::RwLock<HashSet<String>>>,
    /// 轮询间隔
    interval_secs: u64,
}

impl ServiceListSyncer {
    /// 创建新的服务列表同步器。
    pub fn new(
        registry: Arc<dyn ServiceRegistry>,
        cache: Arc<ServiceInstanceCache>,
        interval_secs: u64,
    ) -> Self {
        Self {
            registry,
            cache,
            subscribed_services: Arc::new(std::sync::RwLock::new(HashSet::new())),
            interval_secs,
        }
    }

    /// 标记指定服务已订阅（避免重复订阅）。
    pub fn mark_subscribed(&self, service_name: &str) {
        self.subscribed_services
            .write()
            .unwrap()
            .insert(service_name.to_string());
    }

    /// 启动定时同步循环。
    ///
    /// 该方法会阻塞当前 task，通常在 `tokio::spawn` 中调用。
    /// 当 `shutdown` 收到 `true` 时优雅退出。
    pub async fn run(&self, shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut ticker = interval(Duration::from_secs(self.interval_secs));
        let mut shutdown = shutdown;
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.sync_once().await {
                        warn!(error = %e, "服务列表同步失败");
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("ServiceListSyncer 正在停止");
                        break;
                    }
                }
            }
        }
    }

    /// 启动定时同步循环（无停止信号，兼容旧调用方）。
    ///
    /// 该方法会阻塞当前 task，通常在 `tokio::spawn` 中调用。
    pub async fn run_forever(&self) {
        let mut ticker = interval(Duration::from_secs(self.interval_secs));
        loop {
            ticker.tick().await;
            if let Err(e) = self.sync_once().await {
                warn!(error = %e, "服务列表同步失败");
            }
        }
    }

    /// 执行一次同步。
    #[instrument(target = "cmx_registry", skip(self))]
    async fn sync_once(&self) -> Result<(), crate::error::RegistryError> {
        let services = self.registry.get_service_list().await?;
        let current: HashSet<String> = services.into_iter().collect();

        let known = self.subscribed_services.read().unwrap().clone();

        // 发现新服务
        let new_services: Vec<&String> = current.difference(&known).collect();
        if !new_services.is_empty() {
            info!(
                new_count = new_services.len(),
                services = ?new_services.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                "发现新服务，开始建立订阅"
            );

            for svc in new_services {
                let callback: InstanceChangeCallback =
                    Arc::new(|_svc: &str, _instances: &[ServiceInstance]| {});
                match self.registry.subscribe_instances(svc, callback).await {
                    Ok(()) => {
                        self.subscribed_services.write().unwrap().insert(svc.clone());
                        debug!(service_name = %svc, "新服务订阅成功");
                    }
                    Err(e) => {
                        warn!(service_name = %svc, error = %e, "新服务订阅失败");
                    }
                }
            }
        }

        // 服务下线：清理缓存
        let removed: Vec<&String> = known.difference(&current).collect();
        if !removed.is_empty() {
            info!(
                removed_count = removed.len(),
                services = ?removed.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                "检测到服务下线，清理缓存"
            );
            for svc in removed {
                // 清理缓存（传入空实例列表）
                self.cache.update(svc, vec![]);
                // 注意：不从 subscribed_services 移除，因为 Nacos 不支持 unsubscribe
                // 保留订阅关系，如果服务重新上线，Nacos 会重新推送
            }
        }

        Ok(())
    }
}
