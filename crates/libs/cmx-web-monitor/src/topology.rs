//! 服务依赖 / 集成拓扑（业务无关，任何服务通用）。
//!
//! 回答「本服务的某能力当前挂的是哪个后端」——是**进程内内嵌**还是**反代到独立微服务**，
//! 目标 URL 是什么，以及（对 proxy 目标）**活体探测**结果：现在真的通吗、延迟多少、对端版本/在线时长。
//!
//! 两层解耦，对标 [`crate::identity`]：
//! - **拓扑来源经 [`set_topology_provider`] 注入**：各服务自报「我有哪些能力、各自 embedded/proxy + 目标 URL」
//!   （平台从 `center_client` 配置派生，flow-server 自身则只有一条 self 记录）。monitor 不猜、不写死。
//! - **活体探测由 [`spawn_topology_prober`] 后台跑**：周期性打每个 proxy 目标的 `/_mon/tech-stats`
//!   （每个独立服务都暴露此端点，故探测**跨服务统一**），把结果缓存进进程级快照，读路径零阻塞。
//!
//! 安全：拓扑暴露内部服务 URL/版本，比纯指标敏感。`/_mon/deps` 是否公开由挂载方决定
//! （平台可加访问控制；见 web-server 接线注释）。

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// 一条能力依赖：门户/平台的某功能由哪个后端提供。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDep {
    /// 能力标识（如 `flow`/`report`/`mdm`）。
    pub key: String,
    /// 展示名（如「流程中心」）。
    pub label: String,
    /// `embedded`（进程内）或 `proxy`（反代到独立微服务）。
    pub mode: String,
    /// proxy 模式下的目标基址（embedded 为 None）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// 该能力是否为「可独立部署/可切换」类型（今天仅 flow 为 true；其余为预留内嵌）。
    pub proxiable: bool,
}

/// 活体探测结果（仅 proxy 能力有意义）。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// 现在真的可达吗。
    pub reachable: bool,
    /// 往返延迟毫秒（探测成功时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// 对端服务名（从远程 /_mon/tech-stats 或页标题解析，尽力而为）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_service: Option<String>,
    /// 对端进程在线秒数（若远程 system 快照可读）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_uptime_secs: Option<u64>,
    /// 探测失败原因（不可达时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 上次探测的进程内单调毫秒（前端算「N 秒前」；0 = 从未探测）。
    pub checked_at_ms: u64,
}

/// 一条依赖 + 其最近探测结果（对外 JSON 形态）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DepView {
    #[serde(flatten)]
    dep: ServiceDep,
    #[serde(skip_serializing_if = "Option::is_none")]
    probe: Option<ProbeResult>,
}

/// 拓扑来源钩子：各服务注入「我有哪些能力依赖」。零捕获自由函数即可。
static TOPOLOGY_PROVIDER: OnceLock<fn() -> Vec<ServiceDep>> = OnceLock::new();

/// 注入本服务的拓扑来源（启动时调一次）。
pub fn set_topology_provider(f: fn() -> Vec<ServiceDep>) {
    let _ = TOPOLOGY_PROVIDER.set(f);
}

/// 当前拓扑来源（未注入 → 空）。
fn current_topology() -> Vec<ServiceDep> {
    TOPOLOGY_PROVIDER.get().map(|f| f()).unwrap_or_default()
}

/// 探测结果缓存：target URL → 最近一次 ProbeResult。
static PROBE_CACHE: OnceLock<Mutex<std::collections::HashMap<String, ProbeResult>>> =
    OnceLock::new();

fn probe_cache() -> &'static Mutex<std::collections::HashMap<String, ProbeResult>> {
    PROBE_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// 进程级单调起点（毫秒时间戳基准；对齐 [`crate::system`] 的做法，避免壁钟）。
static PROBE_CLOCK: OnceLock<Instant> = OnceLock::new();
fn now_ms() -> u64 {
    PROBE_CLOCK
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis() as u64
}

/// 起后台活体探测器（幂等）：周期性打各 proxy 目标的 `/_mon/tech-stats`，缓存 reachable/延迟/对端信息。
///
/// embedded 能力不探（就在本进程，恒可达）。无 proxy 目标时循环空转（极廉价）。
pub fn spawn_topology_prober() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return; // 已起过。
    }
    let _ = PROBE_CLOCK.get_or_init(Instant::now);

    tokio::spawn(async move {
        // 短超时：探测是健康信号，不该拖住。连接失败快速判不可达。
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(2))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "拓扑探测器：reqwest 客户端构建失败，探测禁用");
                return;
            }
        };
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            // 每轮取当前 proxy 目标（provider 可能启动后才注入，故每轮读）。
            let targets: Vec<String> = current_topology()
                .into_iter()
                .filter(|d| d.mode == "proxy")
                .filter_map(|d| d.target)
                .collect();
            for target in targets {
                let result = probe_one(&client, &target).await;
                probe_cache().lock().await.insert(target, result);
            }
        }
    });
}

/// 探一个目标：GET `{target}/_mon/tech-stats`，解析对端服务名/在线时长。
async fn probe_one(client: &reqwest::Client, target: &str) -> ProbeResult {
    let url = format!("{}/_mon/tech-stats", target.trim_end_matches('/'));
    let start = Instant::now();
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let latency = start.elapsed().as_millis() as u64;
            // 尽力解析 {data:{system:{procUptimeSecs,...}}}；失败不致命，只是少几个字段。
            let (svc, uptime) = match resp.json::<Value>().await {
                Ok(v) => {
                    let sys = v.get("data").and_then(|d| d.get("system"));
                    let uptime = sys
                        .and_then(|s| s.get("procUptimeSecs"))
                        .and_then(|u| u.as_u64());
                    let svc = v
                        .get("data")
                        .and_then(|d| d.get("service"))
                        .and_then(|s| s.get("name"))
                        .and_then(|n| n.as_str())
                        .map(String::from);
                    (svc, uptime)
                }
                Err(_) => (None, None),
            };
            ProbeResult {
                reachable: true,
                latency_ms: Some(latency),
                remote_service: svc,
                remote_uptime_secs: uptime,
                error: None,
                checked_at_ms: now_ms(),
            }
        }
        Ok(resp) => ProbeResult {
            reachable: false,
            error: Some(format!("HTTP {}", resp.status().as_u16())),
            checked_at_ms: now_ms(),
            ..Default::default()
        },
        Err(e) => ProbeResult {
            reachable: false,
            // 只留精炼原因（超时/连接拒绝），不泄底层栈。
            error: Some(if e.is_timeout() {
                "timeout".into()
            } else if e.is_connect() {
                "connect refused".into()
            } else {
                "unreachable".into()
            }),
            checked_at_ms: now_ms(),
            ..Default::default()
        },
    }
}

/// 拓扑快照：注入的依赖清单 + 各 proxy 目标最近探测结果，合成对外 JSON。
pub async fn topology_snapshot() -> Value {
    let deps = current_topology();
    let cache = probe_cache().lock().await;
    let views: Vec<DepView> = deps
        .into_iter()
        .map(|dep| {
            let probe = dep
                .target
                .as_ref()
                .filter(|_| dep.mode == "proxy")
                .and_then(|t| cache.get(t).cloned());
            DepView { dep, probe }
        })
        .collect();
    json!({
        "services": views,
        "nowMs": now_ms(),
    })
}

