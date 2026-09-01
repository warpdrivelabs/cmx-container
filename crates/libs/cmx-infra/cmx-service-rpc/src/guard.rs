//! per-key 熔断器（circuit breaker）。
//!
//! 连续传输级失败（[`crate::error::ServiceRpcError::is_transport_failure`] 口径：
//! Unavailable / Timeout）达到阈值后**快速失败**（不再发起网络调用），冷却期过半开
//! 放行一次探活——探活成功闭合，失败重新开放。业务级失败（Remote / AuthRejected /
//! Decode）不计数（目标活着，是调用本身被拒）。
//!
//! 状态为进程内基础设施态（类比连接池），非业务缓存，集群无状态合规；
//! `std::sync::Mutex` 临界区极短且不含 await。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 连续失败阈值（达到即开放熔断）。
const FAILURE_THRESHOLD: u32 = 5;

/// 开放后的冷却时长（过此时间进入半开，放行一次探活）。
const COOLDOWN: Duration = Duration::from_secs(10);

#[derive(Debug)]
struct BreakerState {
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    /// 半开中已放行的探活（放行后到结果回来前不再放行）。
    probing: bool,
}

impl BreakerState {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            opened_at: None,
            probing: false,
        }
    }
}

/// per-key 熔断器。
#[derive(Debug, Default)]
pub struct BreakerGuard {
    states: Mutex<HashMap<String, BreakerState>>,
}

/// 熔断检查结果。
pub enum BreakerVerdict {
    /// 放行（闭合态，或冷却期已到的半开探活）。
    Allow,
    /// 拒绝（开放中，含剩余冷却毫秒数）。
    Reject(u64),
}

impl BreakerGuard {
    /// 构造空熔断器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 调用前置检查。
    pub fn check(&self, key: &str) -> BreakerVerdict {
        let mut states = self.states.lock().expect("breaker 锁中毒");
        let Some(state) = states.get_mut(key) else {
            return BreakerVerdict::Allow;
        };
        let Some(opened_at) = state.opened_at else {
            return BreakerVerdict::Allow;
        };
        let elapsed = opened_at.elapsed();
        if elapsed >= COOLDOWN {
            // 半开：放行一次探活。
            if state.probing {
                return BreakerVerdict::Reject(0);
            }
            state.probing = true;
            BreakerVerdict::Allow
        } else {
            BreakerVerdict::Reject((COOLDOWN - elapsed).as_millis() as u64)
        }
    }

    /// 记录一次成功（闭合熔断 / 清零计数 / 结束探活）。
    pub fn record_success(&self, key: &str) {
        let mut states = self.states.lock().expect("breaker 锁中毒");
        states.insert(key.to_string(), BreakerState::new());
    }

    /// 记录一次传输级失败；达到阈值开放熔断。
    pub fn record_failure(&self, key: &str) {
        let mut states = self.states.lock().expect("breaker 锁中毒");
        let state = states.entry(key.to_string()).or_insert_with(BreakerState::new);
        state.probing = false;
        state.consecutive_failures += 1;
        if state.consecutive_failures >= FAILURE_THRESHOLD {
            if state.opened_at.is_none() {
                tracing::warn!(
                    service.key = %key,
                    consecutive_failures = state.consecutive_failures,
                    cooldown_secs = COOLDOWN.as_secs(),
                    "service_rpc 熔断开放：连续传输级失败达阈值，快速失败至冷却期"
                );
            }
            state.opened_at = Some(Instant::now());
        }
    }

    /// 各键熔断快照（观测 / 未来 /metrics 打点用）。
    pub fn snapshot(&self) -> Vec<(String, u32, bool)> {
        let states = self.states.lock().expect("breaker 锁中毒");
        let mut rows: Vec<(String, u32, bool)> = states
            .iter()
            .map(|(k, s)| (k.clone(), s.consecutive_failures, s.opened_at.is_some()))
            .collect();
        rows.sort();
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 连续失败达阈值 → 拒绝；成功闭合；冷却后半开放行探活。
    #[test]
    fn breaker_opens_after_threshold_and_closes_on_success() {
        let guard = BreakerGuard::new();
        for i in 0..FAILURE_THRESHOLD {
            assert!(
                matches!(guard.check("flow"), BreakerVerdict::Allow),
                "第 {i} 次失败前应放行"
            );
            guard.record_failure("flow");
        }
        assert!(matches!(guard.check("flow"), BreakerVerdict::Reject(_)));

        // 成功闭合。
        guard.record_success("flow");
        assert!(matches!(guard.check("flow"), BreakerVerdict::Allow));

        // 快照可见开放状态。
        guard.record_failure("flow");
        assert!(!guard.snapshot().iter().any(|(k, _, open)| k == "flow" && *open));
    }

    /// 阈值以下失败不开放；键间互不影响。
    #[test]
    fn below_threshold_and_key_isolation() {
        let guard = BreakerGuard::new();
        for _ in 0..(FAILURE_THRESHOLD - 1) {
            guard.record_failure("flow");
        }
        assert!(matches!(guard.check("flow"), BreakerVerdict::Allow));
        assert!(matches!(guard.check("report"), BreakerVerdict::Allow));

        for _ in 0..FAILURE_THRESHOLD {
            guard.record_failure("report");
        }
        assert!(matches!(guard.check("report"), BreakerVerdict::Reject(_)));
        assert!(matches!(guard.check("flow"), BreakerVerdict::Allow));
    }
}
