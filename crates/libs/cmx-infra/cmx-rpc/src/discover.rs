//! 注册中心感知的服务发现
//!
//! 桥接 ServiceInstanceCache 与 volo Discover trait，
//! 使 volo 负载均衡器能从注册中心缓存获取服务实例。

use std::borrow::Cow;
use std::sync::{Arc, RwLock};

use async_broadcast::{Receiver, Sender, broadcast};
use tracing::instrument;
use volo::context::Endpoint;
use volo::discovery::{Change, Discover, Instance};
use volo::loadbalance::error::LoadBalanceError;
use volo::net::Address;
use volo::FastStr;

use cmx_registry_config::registry::{ServiceInstance, ServiceInstanceCache};

/// 注册中心感知的服务发现实现
///
/// 将 ServiceInstanceCache 中的服务实例数据转换为 volo 的 Instance 格式，
/// 并通过 async-broadcast 通道通知 volo 负载均衡器实例变更。
pub struct RegistryAwareDiscover {
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// 实例变更通知发送端
    change_tx: Sender<Change<FastStr>>,
    /// 实例变更通知接收端（watch 时取出）
    change_rx: RwLock<Option<Receiver<Change<FastStr>>>>,
}

impl Clone for RegistryAwareDiscover {
    fn clone(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            change_tx: self.change_tx.clone(),
            // 克隆时不复制接收端，每个克隆实例独立管理
            change_rx: RwLock::new(None),
        }
    }
}

impl RegistryAwareDiscover {
    /// 创建新的注册中心感知发现器
    pub fn new(cache: Arc<ServiceInstanceCache>) -> Self {
        let (tx, rx) = broadcast(256);
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
        let cache = self.cache.clone();

        // 注册回调到 ServiceInstanceCache
        cache.subscribe(
            &service_name,
            Arc::new(move |svc_name, instances| {
                let volo_instances = instances_to_volo(instances);
                let change = Change {
                    key: FastStr::new(svc_name),
                    all: volo_instances.clone(),
                    added: volo_instances,
                    updated: vec![],
                    removed: vec![],
                };
                let _ = tx.try_broadcast(change);
            }),
        );
    }
}

/// 将 ServiceInstance 列表转换为 volo Instance 列表
fn instances_to_volo(instances: &[ServiceInstance]) -> Vec<Arc<Instance>> {
    instances
        .iter()
        .filter_map(|i| {
            let addr: std::net::SocketAddr = format!("{}:{}", i.ip, i.port).parse().ok()?;
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
                    tracing::warn!(
                        target: "cmx_rpc",
                        service_name = %service_name,
                        "服务实例缓存为空或未找到"
                    );
                    Ok(vec![])
                }
            }
        }
    }

    fn key(&self, endpoint: &Endpoint) -> Self::Key {
        endpoint.service_name()
    }

    fn watch(&self, _keys: Option<&[Self::Key]>) -> Option<Receiver<Change<Self::Key>>> {
        self.change_rx.write().unwrap().take()
    }
}
