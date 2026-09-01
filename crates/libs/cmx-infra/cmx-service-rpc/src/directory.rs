//! 服务目录（directory）——把 `[service_rpc.services]` 配置解析为可执行的目标定位与调用参数。
//!
//! **定位 per-key**（每个服务键自带定位方式，可混用）：
//! - `services.{key}.url`：静态基址 → [`Locator::Static`]（★无注册中心的回滚形态）；
//! - `services.{key}.discovery`：Nacos 服务名 → [`Locator::Discovery`]（全局实例缓存
//!   healthy 过滤 + 按 Nacos weight 加权随机选例 + `http_port` 元数据优先；
//!   订阅推送 + ServiceListSyncer 30s 同步保证新鲜度）；
//! - url 与 discovery 并存时 **url 优先**（灰度切换语义：url 不可达即失败，不兜底切换；
//!   删 url 即切服务发现选例）。
//!
//! 消费方：
//! - 服务间调用（[`crate::invoke`] 的 RPC 执行）经 [`ServiceDirectory::resolve_base`] 得目标基址；
//! - 反向代理（cmx-flow-api / cmx-rpt-api / cmx-rule-api 等 ProxyModule）经
//!   [`Locator::resolver_fn`] 构造每请求 resolver——反代只取定位字段，不取 transport；
//! - gRPC 皮肤客户端经 [`ServiceDirectory::grpc_service_name`] 取服务发现名。

use std::sync::Arc;
use std::time::Duration;

use rand::distributions::WeightedIndex;
use rand::prelude::{Distribution, SliceRandom};

use cmx_registry_config::{GlobalServiceInstanceCache, GlobalServiceRegistry, ServiceInstance};

use crate::config::{ServiceEntry, ServiceRpcConfig, TransportKind};
use crate::error::ServiceRpcError;

/// 服务目标定位：静态基址或 Nacos 服务发现。
///
/// 供反代壳按请求解析当前基址（`resolve`），也可固化 / 动态两种方式生成 resolver 闭包。
#[derive(Debug, Clone)]
pub enum Locator {
    /// 手动基址（`services.{key}.url`）。值为纯基址（无路径）。
    Static(String),
    /// Nacos 服务发现（`services.{key}.discovery`）。
    Discovery {
        /// Nacos 服务名。
        service: String,
        /// Nacos 分组（配置缺省时为 DEFAULT_GROUP）。
        group: String,
    },
}

impl Locator {
    /// 解析当前可用的目标基址（如 `http://127.0.0.1:8091`）。
    ///
    /// `Static` 返回固化基址；`Discovery` 每次查全局实例缓存选例。缓存未初始化
    /// （注册中心未启用 / `init_infra` 未执行）或无可用实例时返回 `None`
    /// （由调用方决定 503 / Unavailable 语义）——本方法绝不 panic。
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Static(base) => Some(base.clone()),
            Self::Discovery { service, .. } => resolve_service_base(service),
        }
    }

    /// 生成轻量 resolver 闭包（供反代壳在每请求处调用，捕获启动期解析好的目标）。
    ///
    /// `Static` 固化返回基址；`Discovery` 每次执行 [`Self::resolve`]
    /// （仅内存缓存查询，不重新读配置）。
    pub fn resolver_fn(self) -> Arc<dyn Fn() -> Option<String> + Send + Sync> {
        match self {
            Self::Static(base) => Arc::new(move || Some(base.clone())),
            locator => Arc::new(move || locator.resolve()),
        }
    }

    /// 生成人读的目标描述（启动日志 / 拓扑面板用）。
    ///
    /// 形如 `static http://127.0.0.1:8091` 或 `nacos cmx-flow-server (DEFAULT_GROUP)`。
    pub fn describe(&self) -> String {
        match self {
            Self::Static(base) => format!("static {base}"),
            Self::Discovery { service, group } => format!("nacos {service} ({group})"),
        }
    }
}

/// 服务目录：服务键 → 定位 / 传输 / 超时 / 重试的只读快照（初始化期捕获）。
#[derive(Debug, Clone)]
pub struct ServiceDirectory {
    config: ServiceRpcConfig,
}

impl ServiceDirectory {
    /// 从配置构造目录（初始化期调用一次，进程内共享只读快照）。
    pub fn new(config: ServiceRpcConfig) -> Self {
        Self { config }
    }

    /// 底层配置（gRPC 服务端参数等读取用）。
    pub fn config(&self) -> &ServiceRpcConfig {
        &self.config
    }

    /// 服务键是否已配置（键存在即视为目录命中，无论定位字段形态）。
    pub fn contains(&self, key: &str) -> bool {
        self.config.services.contains_key(key)
    }

    /// 目录中配置了给定键集合中的任一键。
    ///
    /// 供装配层（如门户 iam.rs）决定本地 / 远程实现切换。
    pub fn has_any_key(&self, keys: &[&str]) -> bool {
        keys.iter().any(|k| self.contains(k))
    }

    /// 解析指定服务键的目标定位。
    ///
    /// `url`（trim 非空）优先 → [`Locator::Static`]（尾 `/` 归一）；
    /// 否则 `discovery`（trim 非空）→ [`Locator::Discovery`]；
    /// 键未配置或两字段均空 → `None`。
    pub fn locator(&self, key: &str) -> Option<Locator> {
        let entry = self.config.services.get(key)?;
        locator_from_entry(&self.config, entry)
    }

    /// 解析指定服务键当前可用的 HTTP 基址（静态直连或服务发现选例）。
    ///
    /// 无该键或当前无可用实例时返回 `None`。
    pub fn resolve_base(&self, key: &str) -> Option<String> {
        self.locator(key)?.resolve()
    }

    /// 指定服务键的生效传输（键级覆盖全局缺省）。
    pub fn transport_of(&self, key: &str) -> TransportKind {
        self.config.transport_of(key)
    }

    /// 指定服务键的生效请求总超时（键级 `timeout_ms` ?? 全局）。
    pub fn timeout_of(&self, key: &str) -> Duration {
        Duration::from_millis(self.timeout_ms_of(key))
    }

    /// 指定服务键的生效超时毫秒数。
    pub fn timeout_ms_of(&self, key: &str) -> u64 {
        self.config
            .services
            .get(key)
            .and_then(|e| e.timeout_ms)
            .filter(|ms| *ms > 0)
            .unwrap_or(self.config.timeout_ms)
    }

    /// 指定服务键的生效幂等重试上限（键级 `retry_max` ?? 全局）。
    pub fn retry_max_of(&self, key: &str) -> u32 {
        self.config
            .services
            .get(key)
            .and_then(|e| e.retry_max)
            .unwrap_or(self.config.retry_max)
    }

    /// 指定服务键的 gRPC 服务发现名（`transport = "grpc"` 的调用目标）。
    ///
    /// 仅 discovery 定位键可服务 gRPC（按服务名路由）；返回 `(service, group)`。
    pub fn grpc_service_name(&self, key: &str) -> Option<(String, String)> {
        match self.locator(key)? {
            Locator::Discovery { service, group } => Some((service, group)),
            Locator::Static(_) => None,
        }
    }

    /// 全部服务发现定位的目标服务名（订阅预热用；去重）。
    pub fn discovery_service_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .config
            .services
            .values()
            .filter_map(|e| e.discovery.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// 目录快照（人读，启动日志补偿键拼写错误静默不挂的可见性）。
    pub fn snapshot_lines(&self) -> Vec<String> {
        let mut keys: Vec<&String> = self.config.services.keys().collect();
        keys.sort();
        keys.iter()
            .map(|key| {
                let entry = &self.config.services[*key];
                let locate = match locator_from_entry(&self.config, entry) {
                    Some(l) => l.describe(),
                    None => "(未配定位)".to_string(),
                };
                let mut line =
                    format!("{key} = {locate} [transport={}]", self.config.transport_of(key).as_str());
                if let Some(ms) = entry.timeout_ms {
                    line.push_str(&format!(" timeout={ms}ms"));
                }
                if let Some(r) = entry.retry_max {
                    line.push_str(&format!(" retry_max={r}"));
                }
                line
            })
            .collect()
    }

    /// 启动期 fail-fast 校验（判定矩阵）。
    ///
    /// - 键配 `transport = "grpc"` 但进程未编译 `grpc-client` feature → `NoBinding` 错误
    ///   （错配显性化，不静默）；
    /// - 键仅配 `discovery`（无 url）但注册中心未启用 → `Unavailable` 错误并给出键清单
    ///   （静默 503 不可接受，显性提示"补 url 或开注册中心"）。
    ///
    /// `registry_enabled`：注册中心是否真实启用（Mock / 未初始化 = false）。
    pub fn validate(&self, registry_enabled: bool) -> Result<(), ServiceRpcError> {
        for key in self.config.services.keys() {
            if self.config.transport_of(key) == TransportKind::Grpc {
                #[cfg(not(feature = "grpc-client"))]
                {
                    return Err(ServiceRpcError::NoBinding {
                        key: key.clone(),
                        transport: "grpc".to_string(),
                        cause: "键配置 transport=grpc，但进程未启用 grpc-client feature"
                            .to_string(),
                    });
                }
            }
        }
        if !registry_enabled {
            let orphan: Vec<String> = self
                .config
                .services
                .iter()
                .filter(|(_, e)| {
                    let has_url = e.url.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_some();
                    let has_discovery = e
                        .discovery
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .is_some();
                    has_discovery && !has_url
                })
                .map(|(k, _)| k.clone())
                .collect();
            if !orphan.is_empty() {
                return Err(ServiceRpcError::Unavailable {
                    key: orphan.join(", "),
                    cause: "以下键仅配置 discovery 定位，但注册中心未启用且无 url 直连——"
                        .to_string()
                        + "请为这些键补 url（回滚直连形态）或启用注册中心（NACOS_ENABLED）",
                });
            }
        }
        Ok(())
    }
}

/// 从目录条目解析定位（url 优先，纯函数，单测直接构造验证矩阵）。
fn locator_from_entry(cfg: &ServiceRpcConfig, entry: &ServiceEntry) -> Option<Locator> {
    // 定位优先级：url 静态基址 → discovery 服务名（均空白视同未配）。
    // 并存非主备：discovery 完全不生效（url 不可达即失败，不会兜底切换）。
    if let Some(base) = entry
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(Locator::Static(base.trim_end_matches('/').to_string()));
    }
    if let Some(service) = entry
        .discovery
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(Locator::Discovery {
            service: service.to_string(),
            group: cfg
                .nacos_group
                .clone()
                .unwrap_or_else(|| "DEFAULT_GROUP".to_string()),
        });
    }
    None
}

/// 经全局实例缓存解析服务发现目标的当前基址。
///
/// 缓存未初始化（注册中心未启用 / `init_infra` 未执行）或无可用实例时返回 `None`，绝不 panic。
pub fn resolve_service_base(service: &str) -> Option<String> {
    if !GlobalServiceInstanceCache::is_initialized() {
        return None;
    }
    let instances = GlobalServiceInstanceCache::get()
        .get(service)
        .unwrap_or_default();
    pick_instance_base(&instances)
}

/// 注册中心是否真实启用（Nacos 等真实实现，非 Mock）。
///
/// 判据用 `RegistryConfig::from_env().enabled`（环境开关真源）——`MockRegistry::is_enabled`
/// 恒返回 true（历史语义，供其他消费方），不能作为启停依据；未初始化（未跑 `init_infra`）
/// 视同未启用。
pub fn registry_enabled() -> bool {
    GlobalServiceRegistry::is_initialized()
        && cmx_registry_config::RegistryConfig::from_env().enabled
}

/// 从实例列表选一个可用实例并拼 `http://{ip}:{port}` 基址（HTTP 调用与反代共用的选例核）。
///
/// 选例规则：优先 healthy 实例、按 Nacos `weight` 加权随机（与 gRPC 路径 volo 加权负载
/// 均衡语义对齐；权重 clamp 到 `>= 0`，全 0/NaN 时回退均匀随机）；无 healthy 实例时回退
/// 全量（容忍 Nacos 心跳滞后）。端口取 `metadata["http_port"]`（可覆盖注册端口），缺省用
/// 实例注册端口。
pub fn pick_instance_base(instances: &[ServiceInstance]) -> Option<String> {
    if instances.is_empty() {
        return None;
    }
    let healthy: Vec<&ServiceInstance> = instances.iter().filter(|i| i.healthy).collect();
    let pool: Vec<&ServiceInstance> = if healthy.is_empty() {
        instances.iter().collect()
    } else {
        healthy
    };
    let instance = pick_weighted(&pool)?;
    let port = instance
        .metadata
        .get("http_port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(instance.port);
    Some(format!("http://{}:{port}", instance.ip))
}

/// 池内加权随机选一个实例；全部权重 `<= 0`（或 NaN，经 clamp 归零）时回退均匀随机。
fn pick_weighted<'a>(pool: &[&'a ServiceInstance]) -> Option<&'a ServiceInstance> {
    let weights: Vec<f64> = pool.iter().map(|i| i.weight.max(0.0)).collect();
    match WeightedIndex::new(&weights) {
        Ok(dist) => pool.get(dist.sample(&mut rand::thread_rng())).copied(),
        Err(_) => pool.choose(&mut rand::thread_rng()).copied(),
    }
}

/// 对服务发现定位的目标做订阅预热（首拉实例 + 后续推送更新）。
///
/// 注册中心 / 实例缓存未初始化、无 discovery 键时为 no-op，不产生网络行为；
/// 单个订阅失败仅 warn（等 ServiceListSyncer 30s 兜底）。
pub async fn warm_discovery_targets(names: &[String]) {
    if names.is_empty() {
        return;
    }
    if !GlobalServiceInstanceCache::is_initialized() || !GlobalServiceRegistry::is_initialized() {
        return;
    }
    let registry = GlobalServiceRegistry::get().clone();
    for name in names {
        // no-op 回调：订阅动作本身触发首拉 + 注册推送监听，缓存更新由 SDK 驱动。
        if let Err(e) = registry.subscribe_instances(name, Arc::new(|_, _| {})).await {
            tracing::warn!(service = %name, error = %e, "service_rpc 目标订阅预热失败（等 ServiceListSyncer 兜底）");
        }
    }
    tracing::info!(count = names.len(), "service_rpc 服务发现目标订阅预热完成");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: Option<&str>, discovery: Option<&str>, transport: Option<&str>) -> ServiceEntry {
        ServiceEntry {
            url: url.map(str::to_string),
            discovery: discovery.map(str::to_string),
            transport: transport.map(str::to_string),
            timeout_ms: None,
            retry_max: None,
        }
    }

    fn cfg(entries: &[(&str, ServiceEntry)]) -> ServiceRpcConfig {
        ServiceRpcConfig {
            default_transport: "http".to_string(),
            nacos_group: Some("DEFAULT_GROUP".to_string()),
            timeout_ms: 30_000,
            retry_max: 1,
            services: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            server: Default::default(),
        }
    }

    fn instance(
        ip: &str,
        port: u16,
        healthy: bool,
        http_port: Option<u16>,
        weight: f64,
    ) -> ServiceInstance {
        let mut metadata = std::collections::HashMap::new();
        if let Some(p) = http_port {
            metadata.insert("http_port".to_string(), p.to_string());
        }
        ServiceInstance {
            ip: ip.to_string(),
            port,
            service_name: "cmx-flow-server".to_string(),
            group_name: None,
            cluster_name: None,
            weight,
            metadata,
            healthy,
            ephemeral: true,
        }
    }

    /// per-key 定位矩阵：url → Static；discovery → Discovery；键缺失/两字段空 → None；
    /// 同一目录内可混用；url 优先于 discovery；nacos_group 缺省 DEFAULT_GROUP。
    #[test]
    fn locator_dispatch_per_key() {
        let c = ServiceDirectory::new(cfg(&[("flow", entry(Some("http://127.0.0.1:8091/"), None, None))]));
        match c.locator("flow") {
            Some(Locator::Static(base)) => assert_eq!(base, "http://127.0.0.1:8091"),
            other => panic!("应解析为 Static，实际 {other:?}"),
        }
        // 键拼写错误 → None。
        assert!(c.locator("report").is_none());

        match ServiceDirectory::new(cfg(&[("report", entry(None, Some("cmx-rpt-server"), None))]))
            .locator("report")
        {
            Some(Locator::Discovery { service, group }) => {
                assert_eq!(service, "cmx-rpt-server");
                assert_eq!(group, "DEFAULT_GROUP");
            }
            other => panic!("应解析为 Discovery，实际 {other:?}"),
        }

        // 混用 + url 优先。
        let c = ServiceDirectory::new(cfg(&[
            ("flow", entry(Some("http://1.2.3.4:80"), Some("cmx-flow-server"), None)),
            ("report", entry(None, Some("cmx-rpt-server"), None)),
        ]));
        assert!(matches!(c.locator("flow"), Some(Locator::Static(_))));
        assert!(matches!(c.locator("report"), Some(Locator::Discovery { .. })));

        // 空白值视同未配置。
        let c = ServiceDirectory::new(cfg(&[("flow", entry(Some("  "), None, None))]));
        assert!(c.locator("flow").is_none());
        let c = ServiceDirectory::new(cfg(&[("flow", entry(None, None, Some("grpc")))]));
        assert!(c.locator("flow").is_none());
        // 键存在但无定位 → contains 仍为 true（目录命中）、locator None。
        assert!(c.contains("flow"));
        assert!(!c.has_any_key(&["report", "rules"]));
        assert!(c.has_any_key(&["report", "flow"]));

        // nacos_group 缺省 → DEFAULT_GROUP。
        let mut raw = cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]);
        raw.nacos_group = None;
        match ServiceDirectory::new(raw).locator("flow") {
            Some(Locator::Discovery { group, .. }) => assert_eq!(group, "DEFAULT_GROUP"),
            other => panic!("应解析为 Discovery，实际 {other:?}"),
        }
    }

    /// resolver_fn：Static 固化返回；Discovery 在缓存未初始化时返回 None 而非 panic。
    #[test]
    fn resolver_fn_static_and_safe_discovery() {
        let c = ServiceDirectory::new(cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]));
        let resolver = c.locator("flow").expect("应命中").resolver_fn();
        assert_eq!(resolver().as_deref(), Some("http://127.0.0.1:8091"));

        let c = ServiceDirectory::new(cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]));
        let resolver = c.locator("flow").expect("应命中").resolver_fn();
        if GlobalServiceInstanceCache::is_initialized() {
            return; // 同进程其他测试已初始化过单例时跳过该断言。
        }
        assert!(resolver().is_none());
    }

    /// 超时 / 重试覆盖链：键级覆盖全局；键级 0/缺失回退全局。
    #[test]
    fn timeout_and_retry_precedence() {
        let mut raw = cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]);
        raw.timeout_ms = 30_000;
        raw.retry_max = 1;
        raw.services.get_mut("flow").unwrap().timeout_ms = Some(10_000);
        raw.services.get_mut("flow").unwrap().retry_max = Some(0);
        let c = ServiceDirectory::new(raw);
        assert_eq!(c.timeout_ms_of("flow"), 10_000);
        assert_eq!(c.timeout_of("flow"), Duration::from_millis(10_000));
        assert_eq!(c.retry_max_of("flow"), 0);
        assert_eq!(c.timeout_ms_of("report"), 30_000, "未知键回退全局");
    }

    /// grpc 服务名解析：仅 discovery 定位键可服务 gRPC。
    #[test]
    fn grpc_service_name_resolution() {
        let c = ServiceDirectory::new(cfg(&[(
            "perm",
            entry(None, Some("cmx-portal-local"), Some("grpc")),
        )]));
        assert_eq!(
            c.grpc_service_name("perm"),
            Some(("cmx-portal-local".to_string(), "DEFAULT_GROUP".to_string()))
        );
        let c = ServiceDirectory::new(cfg(&[("flow", entry(Some("http://1.2.3.4:8091"), None, None))]));
        assert!(c.grpc_service_name("flow").is_none());
    }

    /// discovery 目标收集：去重 + trim + 排除空白。
    #[test]
    fn discovery_names_collect() {
        let c = ServiceDirectory::new(cfg(&[
            ("flow", entry(Some("http://127.0.0.1:8091"), Some("cmx-flow-server"), None)),
            ("report", entry(None, Some(" cmx-rpt-server "), None)),
            ("rules", entry(None, Some("cmx-flow-server"), None)),
            ("mdm", entry(Some("http://127.0.0.1:8095"), None, None)),
        ]));
        assert_eq!(
            c.discovery_service_names(),
            vec!["cmx-flow-server".to_string(), "cmx-rpt-server".to_string()]
        );
    }

    /// fail-fast 校验矩阵：discovery-only + 注册中心未启用 → 报错并列键清单；
    /// 有 url 的 discovery 键不报错；空目录不报错；注册中心启用时不报错。
    #[test]
    fn validate_matrix() {
        // 空目录：合法。
        assert!(ServiceDirectory::new(ServiceRpcConfig::default())
            .validate(false)
            .is_ok());

        // discovery-only + 未启用注册中心 → Err，错误信息含键名。
        let c = ServiceDirectory::new(cfg(&[
            ("flow", entry(None, Some("cmx-flow-server"), None)),
            ("model", entry(None, Some("cmx-model-server"), None)),
        ]));
        let err = c.validate(false).expect_err("应 fail-fast");
        assert!(err.to_string().contains("flow"));
        assert!(err.to_string().contains("model"));

        // url + discovery 并存（url 优先）→ 不报错。
        let c = ServiceDirectory::new(cfg(&[(
            "flow",
            entry(Some("http://127.0.0.1:8091"), Some("cmx-flow-server"), None),
        )]));
        assert!(c.validate(false).is_ok());

        // 注册中心启用 → discovery-only 合法。
        let c = ServiceDirectory::new(cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]));
        assert!(c.validate(true).is_ok());
    }

    /// 选例核：healthy 优先、http_port 元数据覆盖端口、空列表 None、无 healthy 回退全量。
    #[test]
    fn pick_instance_base_rules() {
        assert!(pick_instance_base(&[]).is_none());

        let list = vec![
            instance("10.0.0.1", 8091, false, None, 1.0),
            instance("10.0.0.2", 8091, true, None, 1.0),
        ];
        for _ in 0..10 {
            assert_eq!(
                pick_instance_base(&list).as_deref(),
                Some("http://10.0.0.2:8091")
            );
        }

        let list = vec![instance("10.0.0.3", 9090, true, Some(8091), 1.0)];
        assert_eq!(
            pick_instance_base(&list).as_deref(),
            Some("http://10.0.0.3:8091")
        );

        let list = vec![instance("10.0.0.4", 8092, false, None, 1.0)];
        assert_eq!(
            pick_instance_base(&list).as_deref(),
            Some("http://10.0.0.4:8092")
        );
    }

    /// 加权随机：高权重显著吃流量（100:1 时低权重命中期望约 1%）。
    #[test]
    fn pick_instance_base_weighted() {
        let list = vec![
            instance("10.0.1.1", 8091, true, None, 5.0),
            instance("10.0.1.2", 8091, true, None, 0.05),
        ];
        let low_hits = (0..200)
            .filter(|_| pick_instance_base(&list).as_deref() == Some("http://10.0.1.2:8091"))
            .count();
        assert!(
            low_hits < 40,
            "低权重实例命中 {low_hits}/200，加权随机未生效"
        );
    }

    /// 全 0 权重回退均匀随机：每个实例都有机会被选中。
    #[test]
    fn pick_instance_base_zero_weight_falls_back_uniform() {
        let list = vec![
            instance("10.0.2.1", 8091, true, None, 0.0),
            instance("10.0.2.2", 8091, true, None, 0.0),
        ];
        let mut hit_first = false;
        let mut hit_second = false;
        for _ in 0..100 {
            match pick_instance_base(&list).as_deref() {
                Some("http://10.0.2.1:8091") => hit_first = true,
                Some("http://10.0.2.2:8091") => hit_second = true,
                other => panic!("应选中池内实例，实际 {other:?}"),
            }
        }
        assert!(hit_first && hit_second, "全 0 权重回退均匀随机应覆盖全部实例");
    }

    /// describe 输出人读目标描述。
    #[test]
    fn describe_locator_kinds() {
        let c = ServiceDirectory::new(cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]));
        assert_eq!(
            c.locator("flow").expect("应命中").describe(),
            "static http://127.0.0.1:8091"
        );
        let c = ServiceDirectory::new(cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]));
        assert_eq!(
            c.locator("flow").expect("应命中").describe(),
            "nacos cmx-flow-server (DEFAULT_GROUP)"
        );
    }
}
