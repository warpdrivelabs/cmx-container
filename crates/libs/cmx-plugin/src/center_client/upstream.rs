//! 反代目标定位（upstream）——把 `[center_client]` 的服务定位配置解析为可动态解析的目标。
//!
//! mode 驱动（沿用 center_client 既有约定，一张配置服务两类消费者）：
//! - `http_url`：`[center_client.urls].{key}` 手动基址 → [`ProxyUpstream::Static`]
//! - `http_discovery` / `grpc`：`[center_client.discovery.services].{key}` Nacos 服务名 →
//!   [`ProxyUpstream::Discovery`]（全局实例缓存 healthy 过滤 + 随机选例；订阅推送 +
//!   ServiceListSyncer 30s 同步保证新鲜度）
//! - `local` / 未知值（含遗留 `mock`）：不挂反代（返回 `None` + warn）
//!
//! 消费方：
//! - 反向代理（cmx-flow-api / cmx-rpt-api / cmx-rule-api 的 ProxyModule）经 [`proxy_upstream`]
//!   取目标，按请求调 [`ProxyUpstream::resolve`] 得当前基址；
//! - 远程导入器（remote_importers）在 http_discovery 模式复用 [`pick_instance_base`] 选例，
//!   与反代同一套负载均衡语义，避免两份逻辑漂移。

use std::sync::Arc;

use rand::seq::SliceRandom;

use super::config::CenterClientConfig;
use cmx_registry_config::{GlobalServiceInstanceCache, GlobalServiceRegistry, ServiceInstance};

/// 反代目标定位：静态基址或 Nacos 服务发现。
#[derive(Debug, Clone)]
pub enum ProxyUpstream {
    /// 手动基址（`[center_client.urls].{key}`，mode = "http_url"）。值为纯基址（无路径）。
    Static(String),
    /// Nacos 服务发现（`[center_client.discovery.services].{key}`，mode = "http_discovery" / "grpc"）。
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

/// 按 mode 从 `[center_client]` 两张服务定位表解析指定服务键的反代目标。
///
/// # Arguments
///
/// * `key` - 服务键（如 "flow" / "report" / "rules"）。
///
/// # Returns
///
/// 配置了该键时返回对应目标（http_url → 静态基址；http_discovery/grpc → 服务发现）；
/// 未配置、mode 为 `local` 或未知值（含遗留 `mock`）时返回 `None`。
pub fn proxy_upstream(key: &str) -> Option<ProxyUpstream> {
    upstream_from_config(&CenterClientConfig::load(), key)
}

/// 纯函数版分派（单测直接构造 [`CenterClientConfig`] 验证矩阵）。
pub(crate) fn upstream_from_config(cfg: &CenterClientConfig, key: &str) -> Option<ProxyUpstream> {
    match cfg.mode.as_str() {
        "http_url" => cfg
            .urls
            .get(key)
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .map(ProxyUpstream::Static),
        "http_discovery" | "grpc" => {
            let service = cfg
                .discovery
                .services
                .get(key)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())?;
            Some(ProxyUpstream::Discovery {
                service,
                group: cfg
                    .discovery
                    .nacos_group
                    .clone()
                    .unwrap_or_else(|| "DEFAULT_GROUP".to_string()),
            })
        }
        "local" => None,
        other => {
            tracing::warn!(
                center_client.mode = %other,
                upstream.key = %key,
                "center_client.mode 为未知值（含遗留 mock），视同 local：不挂反代"
            );
            None
        }
    }
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

/// 对服务发现模式的目标做订阅预热（首拉实例 + 后续推送更新）。
///
/// 门户 `run_platform` 在 `init_infra` 之后调用：对 `[center_client.discovery.services]`
/// 全部服务名注册 no-op 订阅（触发首拉填充缓存；`registered_listeners` 保证幂等）。
/// local / http_url 模式、缓存或注册中心未初始化时为 no-op，不产生网络行为。
pub async fn warm_proxy_upstreams() {
    if !GlobalServiceInstanceCache::is_initialized() || !GlobalServiceRegistry::is_initialized() {
        return;
    }
    let cfg = CenterClientConfig::load();
    let service_names: Vec<String> = match cfg.mode.as_str() {
        "http_discovery" | "grpc" => cfg
            .discovery
            .services
            .values()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => return,
    };
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
    let urls: Vec<&String> = cfg.urls.keys().collect();
    let services: Vec<&String> = cfg.discovery.services.keys().collect();
    tracing::info!(
        center_client.mode = %cfg.mode,
        urls.keys = ?urls,
        discovery.services.keys = ?services,
        "center_client 服务定位配置快照"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::center_client::config::CenterDiscoveryConfig;

    fn cfg(mode: &str, urls: &[(&str, &str)], services: &[(&str, &str)]) -> CenterClientConfig {
        CenterClientConfig {
            mode: mode.to_string(),
            urls: urls
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            discovery: CenterDiscoveryConfig {
                nacos_group: Some("DEFAULT_GROUP".to_string()),
                services: services
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
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

    /// mode 分派矩阵：http_url 查 urls 表；http_discovery/grpc 查 services 表；local/未知值不挂。
    #[test]
    fn upstream_dispatch_by_mode() {
        // http_url：urls 表命中 → Static。
        let c = cfg("http_url", &[("flow", "http://127.0.0.1:8091")], &[]);
        match upstream_from_config(&c, "flow") {
            Some(ProxyUpstream::Static(base)) => assert_eq!(base, "http://127.0.0.1:8091"),
            other => panic!("应解析为 Static，实际 {other:?}"),
        }
        // http_url：urls 表未配该键 → None（键拼写错误静默不挂，靠启动快照日志兜底）。
        assert!(upstream_from_config(&c, "report").is_none());
        // http_discovery：services 表命中 → Discovery。
        let c = cfg(
            "http_discovery",
            &[("flow", "http://127.0.0.1:8091")],
            &[("flow", "cmx-flow-server")],
        );
        match upstream_from_config(&c, "flow") {
            Some(ProxyUpstream::Discovery { service, group }) => {
                assert_eq!(service, "cmx-flow-server");
                assert_eq!(group, "DEFAULT_GROUP");
            }
            other => panic!("应解析为 Discovery，实际 {other:?}"),
        }
        // grpc 与 http_discovery 同走 services 表。
        let c = cfg("grpc", &[], &[("report", "cmx-rpt-server")]);
        assert!(upstream_from_config(&c, "report").is_some());
        // local 与未知值（含遗留 mock）→ None。
        let c = cfg("local", &[("flow", "http://127.0.0.1:8091")], &[("flow", "x")]);
        assert!(upstream_from_config(&c, "flow").is_none());
        let c = cfg("mock", &[("flow", "http://127.0.0.1:8091")], &[]);
        assert!(upstream_from_config(&c, "flow").is_none());
        // 空白值视同未配置。
        let c = cfg("http_url", &[("flow", "  ")], &[]);
        assert!(upstream_from_config(&c, "flow").is_none());
    }

    /// resolver_fn：Static 固化返回；Discovery 在缓存未初始化时返回 None 而非 panic。
    #[test]
    fn resolver_fn_static_and_safe_discovery() {
        let c = cfg("http_url", &[("flow", "http://127.0.0.1:8091")], &[]);
        let resolver = upstream_from_config(&c, "flow").expect("应命中").resolver_fn();
        assert_eq!(resolver().as_deref(), Some("http://127.0.0.1:8091"));

        let c = cfg("grpc", &[], &[("flow", "cmx-flow-server")]);
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
        let c = cfg("http_url", &[("flow", "http://127.0.0.1:8091")], &[]);
        let u = upstream_from_config(&c, "flow").expect("应命中");
        assert_eq!(u.describe(), "static http://127.0.0.1:8091");
        let c = cfg("grpc", &[], &[("flow", "cmx-flow-server")]);
        let u = upstream_from_config(&c, "flow").expect("应命中");
        assert_eq!(u.describe(), "nacos cmx-flow-server (DEFAULT_GROUP)");
    }
}
