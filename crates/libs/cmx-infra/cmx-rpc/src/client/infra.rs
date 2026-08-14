//! gRPC 共享基础设施。
//!
//! 管理服务实例缓存、注册中心订阅、Discover 生命周期。
//! 多个领域客户端通过 `Arc<GrpcInfrastructure>` 共享此实例，避免重复订阅和资源浪费。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::instrument;

use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::RpcError;

use crate::config::GrpcConfig;
use crate::discover::RegistryAwareDiscover;

/// gRPC 共享基础设施。
///
/// 被各领域客户端（`cmx-rpcs/*` 皮肤 crate，如 `cmx-orchestrator-rpc` /
/// `cmx-resource-rpc` 的 `*GrpcClient`）通过 `Arc` 共同持有。
///
/// 核心职责：按 `service_name` 缓存 [`RegistryAwareDiscover`]，避免重复订阅注册中心。
///
/// 共享后多个领域客户端复用同一个 broadcast channel，默认
/// `discover_channel_capacity = 1024` 对多客户端订阅仍充足（实例变更事件频率低）。
pub struct GrpcInfrastructure {
    /// 服务实例缓存
    cache: Arc<ServiceInstanceCache>,
    /// gRPC 配置
    config: GrpcConfig,
    /// 注册中心实例（用于缓存穿透时主动订阅）
    registry: Arc<dyn ServiceRegistry>,
    /// Discover 缓存（service_name → RegistryAwareDiscover）
    discovers: RwLock<HashMap<String, RegistryAwareDiscover>>,
    /// 本服务对外服务级凭证（cmx_sk_xxx），由客户端出站时注入到 gRPC metadata。
    /// `None` 表示未配置服务身份（仅 loopback/单体无跨服务调用场景）。
    outbound_service_key: Option<String>,
}

impl GrpcInfrastructure {
    /// 创建新的 gRPC 共享基础设施。
    pub fn new(
        cache: Arc<ServiceInstanceCache>,
        config: GrpcConfig,
        registry: Arc<dyn ServiceRegistry>,
    ) -> Self {
        Self {
            cache,
            config,
            registry,
            discovers: RwLock::new(HashMap::new()),
            outbound_service_key: None,
        }
    }

    /// 设置本服务对外服务级凭证（`cmx_sk_xxx`）。
    ///
    /// 由组装层（`web-server`）在 `init_rpc_clients` 后调用，来源为
    /// `[service_auth].outgoing_api_key` 配置项。
    pub fn with_outbound_service_key(mut self, key: impl Into<Option<String>>) -> Self {
        self.outbound_service_key = key.into();
        self
    }

    /// 获取本服务对外服务级凭证（出站 header 注入用）。
    pub fn outbound_service_key(&self) -> Option<&str> {
        self.outbound_service_key.as_deref()
    }

    /// 单次 gRPC `rpc_timeout`（供 volo `rpc_timeout` 设置）。
    pub fn rpc_timeout(&self) -> Duration {
        Duration::from_millis(self.config.timeout_ms)
    }

    /// 连接超时（供 volo `connect_timeout` 设置）。
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.config.connect_timeout_ms)
    }

    /// RPC 总超时预算（毫秒），供 [`super::retry::with_retry`] 计算 deadline。
    ///
    /// 注意：当前与 [`rpc_timeout`](Self::rpc_timeout) 同源（共用 `config.timeout_ms`），
    /// 属于已知设计债——见方案文档第十一章。未来计划拆分为
    /// `rpc_timeout_ms`（单次）与 `retry_total_budget_ms`（总预算）。
    pub fn timeout_ms(&self) -> u64 {
        self.config.timeout_ms
    }

    /// 重试次数（仅对可重试错误生效：UNAVAILABLE/DEADLINE_EXCEEDED/RESOURCE_EXHAUSTED/ABORTED）。
    pub fn retry_count(&self) -> usize {
        self.config.retry_count
    }

    /// 获取或创建指定服务的 Discover（double-check locking + 注册中心懒订阅）。
    ///
    /// 网络 IO（`subscribe_instances`）在写锁外完成，写锁只保护 HashMap insert，
    /// 避免因网络请求阻塞其他服务的 Discover 创建。
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    pub async fn get_or_create_discover(
        &self,
        service_name: &str,
    ) -> Result<RegistryAwareDiscover, RpcError> {
        // 快查：读锁检查缓存
        if let Some(d) = self.discovers.read().await.get(service_name) {
            return Ok(d.clone());
        }

        // 慢路径：在获取写锁之前，先完成网络 IO（订阅 + 缓存填充）。
        if self.cache.get(service_name).is_none_or(|v| v.is_empty()) {
            self.registry
                .subscribe_instances(service_name, Arc::new(|_, _| {}))
                .await
                .map_err(|e| {
                    RpcError::NoAvailableInstance(format!(
                        "服务 '{}' 订阅失败: {}",
                        service_name, e
                    ))
                })?;

            if self.cache.get(service_name).is_none_or(|v| v.is_empty()) {
                return Err(RpcError::NoAvailableInstance(service_name.to_string()));
            }
        }

        // 创建 Discover 并启动监听（不涉及网络 IO，可在锁外完成）
        let discover =
            RegistryAwareDiscover::new(self.cache.clone(), self.config.discover_channel_capacity);
        discover.start_watch(service_name);

        // 写锁：仅保护 HashMap insert，double-check 防止并发重复创建
        let mut discovers = self.discovers.write().await;
        if let Some(d) = discovers.get(service_name) {
            return Ok(d.clone());
        }
        discovers.insert(service_name.to_string(), discover.clone());

        Ok(discover)
    }
}
