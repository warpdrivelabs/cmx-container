//! 可观测（obs）——调用 span 与 per-key 打点。
//!
//! 每次调用建立 `service_rpc` info span（服务键 + 方法 + 路径），完成时打
//! debug 事件（目标基址 + 耗时 + 结果）；per-key 计数（调用量 / 传输级失败 /
//! 累计耗时）经 [`stats_snapshot`] 暴露，供未来 /metrics 出口接线。
//! 进程内聚合，集群无状态合规。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// per-key 累计指标。
#[derive(Debug, Clone, Default)]
pub struct KeyStats {
    /// 总调用次数（含失败）。
    pub calls: u64,
    /// 传输级失败次数（Unavailable / Timeout）。
    pub transport_failures: u64,
    /// 累计耗时（毫秒）。
    pub total_dur_ms: u64,
}

/// 进程内 per-key 打点聚合器。
///
/// 以服务键为维度累计 [`KeyStats`]，进程内聚合、不落盘——集群无状态合规，
/// 快照经 [`stats_snapshot`] 导出。
#[derive(Debug, Default)]
pub struct Stats {
    inner: Mutex<HashMap<String, KeyStats>>,
}

impl Stats {
    /// 记录一次调用，累加到对应服务键的指标上。
    ///
    /// # Arguments
    ///
    /// * `key` - 服务定位键（如 `flow` / `mdm`），未出现过则先初始化为零值。
    /// * `dur` - 本次调用总耗时（含重试内多次尝试则由调用方聚合后传入）。
    /// * `transport_failure` - 本次调用是否为传输级失败（Unavailable / Timeout），
    ///   为 `true` 时 `transport_failures` 计数加一。
    pub(crate) fn record(&self, key: &str, dur: Duration, transport_failure: bool) {
        let mut inner = self.inner.lock().expect("stats 锁中毒");
        let stats = inner.entry(key.to_string()).or_default();
        stats.calls += 1;
        if transport_failure {
            stats.transport_failures += 1;
        }
        stats.total_dur_ms += dur.as_millis() as u64;
    }
}

/// 建立一次调用的 span。
pub(crate) fn call_span(key: &str, method: &str, path: &str) -> tracing::Span {
    tracing::info_span!(
        "service_rpc",
        rpc.key = %key,
        rpc.method = %method,
        rpc.path = %path,
    )
}

/// 打点快照（键名有序）。
pub fn stats_snapshot(stats: &Stats) -> Vec<(String, KeyStats)> {
    let inner = stats.inner.lock().expect("stats 锁中毒");
    let mut rows: Vec<(String, KeyStats)> = inner
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}
