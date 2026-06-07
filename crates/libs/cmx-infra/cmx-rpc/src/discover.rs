//! 注册中心感知的服务发现
//!
//! 桥接 ServiceInstanceCache 与 volo Discover trait，
//! 使 volo 负载均衡器能从注册中心缓存获取服务实例。

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use async_broadcast::{Receiver, Sender, broadcast};
use tracing::instrument;
use volo::context::Endpoint;
use volo::discovery::{Change, Discover, Instance};
use volo::loadbalance::error::LoadBalanceError;
use volo::net::Address;
use volo::FastStr;

use cmx_registry_config::registry::{ServiceInstance, ServiceInstanceCache};

/// 默认 broadcast 通道容量
const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// 注册中心感知的服务发现实现
///
/// 将 ServiceInstanceCache 中的服务实例数据转换为 volo 的 Instance 格式，
/// 并通过 async-broadcast 通道通知 volo 负载均衡器实例变更。
pub struct RegistryAwareDiscover {
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// 实例变更通知发送端
    change_tx: Sender<Change<FastStr>>,
    /// 实例变更通知接收端（watch 时克隆共享）
    change_rx: RwLock<Option<Receiver<Change<FastStr>>>>,
}

impl Clone for RegistryAwareDiscover {
    fn clone(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            change_tx: self.change_tx.clone(),
            change_rx: RwLock::new(self.change_rx.read().expect("change_rx 锁中毒").as_ref().cloned()),
        }
    }
}

impl RegistryAwareDiscover {
    /// 创建新的注册中心感知发现器
    ///
    /// `channel_capacity` 为内部 broadcast 通道容量，默认 1024。
    /// 值越大越能缓冲高频服务变更（如 k8s 滚动更新）。
    pub fn new(cache: Arc<ServiceInstanceCache>, channel_capacity: usize) -> Self {
        let capacity = if channel_capacity == 0 { DEFAULT_CHANNEL_CAPACITY } else { channel_capacity };
        let (tx, rx) = broadcast(capacity);
        Self {
            cache,
            change_tx: tx,
            change_rx: RwLock::new(Some(rx)),
        }
    }

    /// 启动监听某个服务的实例变更
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    pub fn start_watch(&self, service_name: &str) {
        let tx = self.change_tx.clone();
        let service_name = service_name.to_string();
        let cache_for_closure = self.cache.clone();

        // 注册回调到 ServiceInstanceCache
        self.cache.subscribe(
            &service_name,
            Arc::new(move |svc_name, new_instances| {
                let new_volo = instances_to_volo(new_instances);

                // 获取旧实例列表做 diff
                let old_volo = cache_for_closure.get(svc_name)
                    .map(|old| instances_to_volo(&old))
                    .unwrap_or_default();

                // 计算 diff
                let old_addrs: HashSet<_> = old_volo.iter().map(|i| i.address.clone()).collect();
                let new_addrs: HashSet<_> = new_volo.iter().map(|i| i.address.clone()).collect();

                let added: Vec<_> = new_volo.iter()
                    .filter(|i| !old_addrs.contains(&i.address))
                    .cloned()
                    .collect();
                let removed: Vec<_> = old_volo.iter()
                    .filter(|i| !new_addrs.contains(&i.address))
                    .cloned()
                    .collect();
                // updated: 地址相同但 weight/tags 变化的实例
                let updated: Vec<_> = new_volo.iter()
                    .filter(|new_i| {
                        old_addrs.contains(&new_i.address) && old_volo.iter().any(|old_i| {
                            old_i.address == new_i.address &&
                            (old_i.weight != new_i.weight || old_i.tags != new_i.tags)
                        })
                    })
                    .cloned()
                    .collect();

                let change = Change {
                    key: FastStr::new(svc_name),
                    all: new_volo,
                    added,
                    updated,
                    removed,
                };
                match tx.try_broadcast(change) {
                    Ok(_) => {}
                    Err(async_broadcast::TrySendError::Full(_)) => {
                        tracing::error!(
                            target: "cmx_rpc",
                            "实例变更广播失败: 通道已满，事件已丢失（考虑增大 discover_channel_capacity）"
                        );
                    }
                    Err(async_broadcast::TrySendError::Inactive(_)) => {
                        tracing::trace!(
                            target: "cmx_rpc",
                            "实例变更广播跳过: 无活跃接收者（启动期正常）"
                        );
                    }
                    Err(async_broadcast::TrySendError::Closed(_)) => {
                        tracing::warn!(
                            target: "cmx_rpc",
                            "实例变更广播失败: 通道已关闭"
                        );
                    }
                }
            }),
        );
    }
}

/// 将 ServiceInstance 列表转换为 volo Instance 列表
fn instances_to_volo(instances: &[ServiceInstance]) -> Vec<Arc<Instance>> {
    instances
        .iter()
        .filter_map(|i| {
            let addr: std::net::SocketAddr = match format!("{}:{}", i.ip, i.port).parse() {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(
                        target: "cmx_rpc",
                        service_name = %i.service_name,
                        ip = %i.ip,
                        port = i.port,
                        error = %e,
                        "跳过地址解析失败的实例"
                    );
                    return None;
                }
            };
            Some(Arc::new(Instance {
                address: Address::Ip(addr),
                weight: (i.weight * 100.0) as u32,
                tags: i
                    .metadata
                    .iter()
                    .map(|(k, v)| (Cow::Owned(k.clone()), Cow::Owned(v.clone())))
                    .collect(),
            }))
        })
        .collect()
}

impl Discover for RegistryAwareDiscover {
    type Key = FastStr;
    type Error = LoadBalanceError;

    fn discover<'s>(
        &'s self,
        endpoint: &'s Endpoint,
    ) -> impl std::future::Future<Output = Result<Vec<Arc<Instance>>, Self::Error>> + Send {
        let service_name = endpoint.service_name_ref().to_string();
        async move {
            // 从缓存获取，纯内存操作
            match self.cache.get(&service_name) {
                Some(instances) if !instances.is_empty() => Ok(instances_to_volo(&instances)),
                _ => {
                    tracing::debug!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        "服务实例缓存为空或未找到"
                    );
                    Err(LoadBalanceError::Discover(
                        format!("service not found in cache: {}", service_name).into(),
                    ))
                }
            }
        }
    }

    fn key(&self, endpoint: &Endpoint) -> Self::Key {
        endpoint.service_name()
    }

    fn watch(&self, _keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>> {
        self.change_rx
            .read()
            .expect("change_rx 锁中毒")
            .as_ref()
            .map(|rx| rx.clone())
    }
}
