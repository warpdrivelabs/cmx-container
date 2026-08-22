//! 反代目标定位（upstream）——把 `[center_client.services]` 的服务定位配置解析为可动态解析的目标。
//!
//! **定位 per-key**（每个服务键自带定位方式，可混用）：
//! - `services.{key}.url`：静态基址 → [`ProxyUpstream::Static`]
//! - `services.{key}.discovery`：Nacos 服务名 → [`ProxyUpstream::Discovery`]（全局实例缓存
//!   healthy 过滤 + 随机选例；订阅推送 + ServiceListSyncer 30s 同步保证新鲜度）
//! - 键未配置、或 url/discovery 均为空：不挂反代（返回 `None` + warn）
//!
//! `transport` 字段对反代无效（反代恒走 HTTP 透明转发）；配了 `grpc` 会打 warn 提示。
//!
//! 消费方：
//! - 反向代理（cmx-flow-api / cmx-rpt-api / cmx-rule-api 的 ProxyModule）经 [`proxy_upstream`]
//!   取目标，按请求调 [`ProxyUpstream::resolve`] 得当前基址；
//! - 远程导入器（remote_importers）在 discovery 定位 + HTTP 传输时复用 [`pick_instance_base`]
//!   选例，与反代同一套负载均衡语义，避免两份逻辑漂移。

use std::sync::Arc;

use rand::seq::SliceRandom;

use super::config::CenterClientConfig;
use cmx_registry_config::{GlobalServiceInstanceCache, GlobalServiceRegistry, ServiceInstance};

/// 反代目标定位：静态基址或 Nacos 服务发现。
#[derive(Debug, Clone)]
pub enum ProxyUpstream {
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

impl ProxyUpstream {
    /// 解析当前可用的目标基址（如 `http://127.0.0.1:8091`）。
    ///
    /// # Returns
    ///
    /// `Static` 返回固化基址；`Discovery` 每次查全局实例缓存选例。缓存未初始化
    /// （`init_infra` 未执行）或无可用实例时返回 `None`（由调用方决定 503 语义）——
    /// 本方法绝不 panic。
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Static(base) => Some(base.clone()),
            Self::Discovery { service, .. } => resolve_service_base(service),
        }
    }

    /// 生成轻量 resolver 闭包（供反代壳在每请求处调用，捕获启动期解析好的目标）。
    ///
    /// # Returns
    ///
    /// `Arc<dyn Fn() -> Option<String> + Send + Sync>`：`Static` 固化返回基址；
    /// `Discovery` 每次执行 [`Self::resolve`]（仅内存缓存查询，不重新读配置）。
    pub fn resolver_fn(self) -> Arc<dyn Fn() -> Option<String> + Send + Sync> {
        match self {
            Self::Static(base) => Arc::new(move || Some(base.clone())),
            upstream => Arc::new(move || upstream.resolve()),
        }
    }

    /// 生成人读的目标描述（启动日志 / 拓扑面板用）。
    ///
    /// # Returns
    ///
    /// 形如 `static http://127.0.0.1:8091` 或 `nacos cmx-flow-server (DEFAULT_GROUP)` 的字符串。
    pub fn describe(&self) -> String {
        match self {
            Self::Static(base) => format!("static {base}"),
            Self::Discovery { service, group } => format!("nacos {service} ({group})"),
        }
    }
}

/// 从 `[center_client.services]` 解析指定服务键的反代目标（定位 per-key）。
///
/// # Arguments
///
/// * `key` - 服务键（如 "flow" / "report" / "rules"）。
///
/// # Returns
///
/// 配置了该键时返回对应目标（`url` → 静态基址；`discovery` → 服务发现）；
/// 键未配置或 url/discovery 均为空时返回 `None`。
pub fn proxy_upstream(key: &str) -> Option<ProxyUpstream> {
    upstream_from_config(&CenterClientConfig::load(), key)
}

/// 纯函数版分派（单测直接构造 [`CenterClientConfig`] 验证矩阵）。
pub(crate) fn upstream_from_config(cfg: &CenterClientConfig, key: &str) -> Option<ProxyUpstream> {
    let entry = cfg.services.get(key)?;
    // 定位优先级：url 静态基址 → discovery 服务名（均空白视同未配）。
    if let Some(base) = entry
        .url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(ProxyUpstream::Static(
            base.trim_end_matches('/').to_string(),
        ));
    }
    if let Some(service) = entry
        .discovery
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if entry.transport.as_deref().map(str::trim) == Some("grpc") {
            tracing::warn!(
                service.key = key,
                "transport=grpc 对反向代理无效（反代恒走 HTTP 透明转发），按 discovery 定位继续"
            );
        }
        return Some(ProxyUpstream::Discovery {
            service: service.to_string(),
            group: cfg
                .nacos_group
                .clone()
                .unwrap_or_else(|| "DEFAULT_GROUP".to_string()),
        });
    }
    tracing::warn!(
        service.key = key,
        "services.{key} 未配 url/discovery，忽略该键（不挂反代）"
    );
    None
}

/// 经全局实例缓存解析服务发现目标的当前基址。
///
/// # Arguments
///
/// * `service` - Nacos 服务名。
///
/// # Returns
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

/// 从实例列表选一个可用实例并拼 `http://{ip}:{port}` 基址（反代与导入器共用的选例核）。
///
/// 选例规则：优先 healthy 实例、随机负载均衡；无 healthy 实例时回退全量（容忍 Nacos 心跳
/// 滞后）。端口取 `metadata["http_port"]`（可覆盖注册端口），缺省用实例注册端口。
///
/// # Arguments
///
/// * `instances` - 缓存中的实例列表（调用方负责获取）。
///
/// # Returns
///
/// 选中实例的 `http://ip:port` 基址；列表为空时返回 `None`。
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
    let instance = pool.choose(&mut rand::thread_rng())?;
    let port = instance
        .metadata
        .get("http_port")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(instance.port);
    Some(format!("http://{}:{port}", instance.ip))
}

/// 对服务发现定位的目标做订阅预热（首拉实例 + 后续推送更新）。
///
/// 门户 `run_platform` 在 `init_infra` 之后调用：对 `[center_client.services]` 里全部
/// `discovery` 非空的服务名注册 no-op 订阅（触发首拉填充缓存；`registered_listeners`
/// 保证幂等）。缓存或注册中心未初始化、无 discovery 键时为 no-op，不产生网络行为。
pub async fn warm_proxy_upstreams() {
    if !GlobalServiceInstanceCache::is_initialized() || !GlobalServiceRegistry::is_initialized() {
        return;
    }
    let cfg = CenterClientConfig::load();
    let service_names: Vec<String> = cfg
        .services
        .values()
        .filter_map(|e| e.discovery.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if service_names.is_empty() {
        return;
    }
    let registry = GlobalServiceRegistry::get().clone();
    for name in service_names {
        // no-op 回调：订阅动作本身触发首拉 + 注册推送监听，缓存更新由 SDK 驱动。
        if let Err(e) = registry
            .subscribe_instances(&name, Arc::new(|_, _| {}))
            .await
        {
            tracing::warn!(service = %name, error = %e, "center_client 目标订阅预热失败（等 ServiceListSyncer 兜底）");
        }
    }
    tracing::info!("center_client 服务发现目标订阅预热完成");
}

/// 启动时打印服务定位配置快照（补偿 map 键拼写错误静默不挂路由的可见性）。
pub fn log_center_client_snapshot() {
    let cfg = CenterClientConfig::load();
    // 每键一行描述：定位（static url / nacos 服务名）+ 生效传输。
    let entries: Vec<String> = cfg
        .services
        .iter()
        .map(|(key, entry)| {
            let locate = if let Some(url) = entry.url.as_deref().filter(|s| !s.trim().is_empty()) {
                format!("static {url}")
            } else if let Some(svc) = entry.discovery.as_deref().filter(|s| !s.trim().is_empty()) {
                format!("nacos {svc}")
            } else {
                "(未配定位)".to_string()
            };
            format!("{key} = {locate} [transport={}]", cfg.transport_of(key))
        })
        .collect();
    tracing::info!(
        center_client.default_transport = %cfg.default_transport,
        center_client.nacos_group = ?cfg.nacos_group,
        services = ?entries,
        "center_client 服务定位配置快照"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::center_client::config::ServiceEntry;

    fn entry(url: Option<&str>, discovery: Option<&str>, transport: Option<&str>) -> ServiceEntry {
        ServiceEntry {
            url: url.map(str::to_string),
            discovery: discovery.map(str::to_string),
            transport: transport.map(str::to_string),
        }
    }

    fn cfg(entries: &[(&str, ServiceEntry)]) -> CenterClientConfig {
        CenterClientConfig {
            default_transport: "http".to_string(),
            nacos_group: Some("DEFAULT_GROUP".to_string()),
            services: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            timeout_ms: 30000,
        }
    }

    fn instance(ip: &str, port: u16, healthy: bool, http_port: Option<u16>) -> ServiceInstance {
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
            weight: 1.0,
            metadata,
            healthy,
            ephemeral: true,
        }
    }

    /// per-key 分派矩阵：url → Static；discovery → Discovery；键缺失/两字段空 → None；
    /// 同一配置内可混用两种定位。
    #[test]
    fn upstream_dispatch_per_key() {
        // 静态定位命中 → Static。
        let c = cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]);
        match upstream_from_config(&c, "flow") {
            Some(ProxyUpstream::Static(base)) => assert_eq!(base, "http://127.0.0.1:8091"),
            other => panic!("应解析为 Static，实际 {other:?}"),
        }
        // 键拼写错误 → None（靠启动快照日志兜底可见性）。
        assert!(upstream_from_config(&c, "report").is_none());
        // discovery 定位命中 → Discovery。
        let c = cfg(&[("report", entry(None, Some("cmx-rpt-server"), None))]);
        match upstream_from_config(&c, "report") {
            Some(ProxyUpstream::Discovery { service, group }) => {
                assert_eq!(service, "cmx-rpt-server");
                assert_eq!(group, "DEFAULT_GROUP");
            }
            other => panic!("应解析为 Discovery，实际 {other:?}"),
        }
        // 混用：同一配置内 flow 静态 + report 服务发现，互不干扰。
        let c = cfg(&[
            ("flow", entry(Some("http://127.0.0.1:8091"), None, None)),
            ("report", entry(None, Some("cmx-rpt-server"), None)),
        ]);
        assert!(matches!(
            upstream_from_config(&c, "flow"),
            Some(ProxyUpstream::Static(_))
        ));
        assert!(matches!(
            upstream_from_config(&c, "report"),
            Some(ProxyUpstream::Discovery { .. })
        ));
        // url 优先于 discovery（两者都配时）。
        let c = cfg(&[("flow", entry(Some("http://1.2.3.4:80"), Some("cmx-flow-server"), None))]);
        assert!(matches!(
            upstream_from_config(&c, "flow"),
            Some(ProxyUpstream::Static(_))
        ));
        // 空白值视同未配置；两字段均空 → None。
        let c = cfg(&[("flow", entry(Some("  "), None, None))]);
        assert!(upstream_from_config(&c, "flow").is_none());
        let c = cfg(&[("flow", entry(None, None, Some("grpc")))]);
        assert!(upstream_from_config(&c, "flow").is_none());
        // nacos_group 缺省 → DEFAULT_GROUP。
        let mut c = cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]);
        c.nacos_group = None;
        match upstream_from_config(&c, "flow") {
            Some(ProxyUpstream::Discovery { group, .. }) => assert_eq!(group, "DEFAULT_GROUP"),
            other => panic!("应解析为 Discovery，实际 {other:?}"),
        }
    }

    /// resolver_fn：Static 固化返回；Discovery 在缓存未初始化时返回 None 而非 panic。
    #[test]
    fn resolver_fn_static_and_safe_discovery() {
        let c = cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]);
        let resolver = upstream_from_config(&c, "flow").expect("应命中").resolver_fn();
        assert_eq!(resolver().as_deref(), Some("http://127.0.0.1:8091"));

        let c = cfg(&[("flow", entry(None, Some("cmx-flow-server"), Some("grpc")))]);
        let resolver = upstream_from_config(&c, "flow").expect("应命中").resolver_fn();
        // 测试进程未 init_infra：全局缓存未初始化 → None（不 panic）。
        if GlobalServiceInstanceCache::is_initialized() {
            return; // 同进程其他测试已初始化过单例时跳过该断言。
        }
        assert!(resolver().is_none());
    }

    /// 选例核：healthy 优先、http_port 元数据覆盖端口、空列表返回 None、无 healthy 回退全量。
    #[test]
    fn pick_instance_base_rules() {
        assert!(pick_instance_base(&[]).is_none());

        // healthy 过滤：唯一 healthy 实例被选中（随机无歧义）。
        let list = vec![
            instance("10.0.0.1", 8091, false, None),
            instance("10.0.0.2", 8091, true, None),
        ];
        for _ in 0..10 {
            assert_eq!(
                pick_instance_base(&list).as_deref(),
                Some("http://10.0.0.2:8091")
            );
        }

        // http_port 元数据优先于实例注册端口。
        let list = vec![instance("10.0.0.3", 9090, true, Some(8091))];
        assert_eq!(
            pick_instance_base(&list).as_deref(),
            Some("http://10.0.0.3:8091")
        );

        // 无 healthy 实例时回退全量（容忍心跳滞后）。
        let list = vec![instance("10.0.0.4", 8092, false, None)];
        assert_eq!(
            pick_instance_base(&list).as_deref(),
            Some("http://10.0.0.4:8092")
        );
    }

    /// describe 输出人读目标描述（启动日志用）。
    #[test]
    fn describe_upstream_kinds() {
        let c = cfg(&[("flow", entry(Some("http://127.0.0.1:8091"), None, None))]);
        let u = upstream_from_config(&c, "flow").expect("应命中");
        assert_eq!(u.describe(), "static http://127.0.0.1:8091");
        let c = cfg(&[("flow", entry(None, Some("cmx-flow-server"), None))]);
        let u = upstream_from_config(&c, "flow").expect("应命中");
        assert_eq!(u.describe(), "nacos cmx-flow-server (DEFAULT_GROUP)");
    }
}
