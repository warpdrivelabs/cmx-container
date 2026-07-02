//! 熔断器。
//!
//! 简化版熔断器（closed/open 两态），通过 `reset_duration` 自动恢复。
//! 当连续失败次数达到阈值时打开熔断器，拒绝后续请求；
//! 经过 `reset_duration` 后自动进入半开状态，允许请求通过。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// 熔断器。
///
/// 通过 `threshold` 与 `reset_duration` 控制熔断与恢复行为，
/// 用于 `IamChecker` 在数据库/缓存故障时保护系统。
pub struct CircuitBreaker {
    /// 连续失败次数。
    failure_count: AtomicU32,
    /// 熔断器是否打开。
    is_open: AtomicBool,
    /// 最后一次失败时间。
    last_failure_time: Mutex<Option<Instant>>,
    /// 熔断阈值。
    threshold: u32,
    /// 熔断恢复时间。
    reset_duration: Duration,
}

impl CircuitBreaker {
    /// 创建新熔断器。
    ///
    /// # Arguments
    ///
    /// * `threshold` - 连续失败次数阈值，达到后打开熔断器。
    /// * `reset_secs` - 熔断恢复时间（秒），经过该时间后进入半开状态。
    ///
    /// # Returns
    ///
    /// 返回处于关闭状态的新熔断器实例。
    pub fn new(threshold: u32, reset_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            is_open: AtomicBool::new(false),
            last_failure_time: Mutex::new(None),
            threshold,
            reset_duration: Duration::from_secs(reset_secs),
        }
    }

    /// 检查是否允许请求通过。
    ///
    /// - 熔断器关闭时：允许。
    /// - 熔断器打开且超过恢复时间：半开，允许（失败会重新打开）。
    /// - 熔断器打开且未超过恢复时间：拒绝。
    ///
    /// # Returns
    ///
    /// 允许通过返回 `true`，拒绝返回 `false`。
    pub fn allow_request(&self) -> bool {
        if !self.is_open.load(Ordering::Relaxed) {
            return true;
        }

        // 熔断器打开，检查是否已过恢复时间
        let should_reset = {
            let last = self.last_failure_time.lock().unwrap();
            match *last {
                Some(time) => Instant::now().duration_since(time) >= self.reset_duration,
                None => true,
            }
        };

        if should_reset {
            // 半开状态：重置并允许请求
            self.is_open.store(false, Ordering::Relaxed);
            self.failure_count.store(0, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 记录成功，重置失败计数并关闭熔断器。
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.is_open.store(false, Ordering::Relaxed);
    }

    /// 记录失败，连续失败达到阈值时打开熔断器。
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
            let mut last = self.last_failure_time.lock().unwrap();
            *last = Some(Instant::now());
        }
    }

    /// 检查熔断器是否打开。
    ///
    /// # Returns
    ///
    /// 熔断器打开返回 `true`，关闭返回 `false`。
    pub fn is_circuit_open(&self) -> bool {
        self.is_open.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_closed() {
        let cb = CircuitBreaker::new(3, 60);
        assert!(cb.allow_request());
        assert!(!cb.is_circuit_open());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.allow_request()); // 未达阈值
        cb.record_failure();
        assert!(cb.is_circuit_open());
        assert!(!cb.allow_request()); // 熔断器打开
    }

    #[test]
    fn test_circuit_breaker_resets_after_duration() {
        let cb = CircuitBreaker::new(1, 0); // 0秒恢复
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // 立即检查，由于 reset_duration=0，应该允许
        std::thread::sleep(Duration::from_millis(10));
        assert!(cb.allow_request());
    }

    #[test]
    fn test_record_success_resets() {
        let cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.failure_count.load(Ordering::Relaxed), 0);
    }

    /// 边界：阈值-1 次失败不应打开熔断器
    #[test]
    fn test_below_threshold_does_not_open() {
        let cb = CircuitBreaker::new(5, 60);
        for _ in 0..4 {
            cb.record_failure();
        }
        assert!(!cb.is_circuit_open());
        assert!(cb.allow_request());
    }

    /// 边界：恰好达到阈值时打开熔断器
    #[test]
    fn test_exact_threshold_opens() {
        let cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_circuit_open());
        cb.record_failure();
        assert!(cb.is_circuit_open());
    }

    /// 熔断状态下连续多次请求均被拒绝
    #[test]
    fn test_open_state_rejects_all_requests() {
        let cb = CircuitBreaker::new(1, 60);
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // 熔断打开期间，连续 5 次请求都应被拒绝
        for _ in 0..5 {
            assert!(!cb.allow_request(), "熔断状态下应拒绝所有请求");
        }
    }

    /// 半开状态：超过恢复时间后允许请求通过
    #[test]
    fn test_half_open_allows_after_reset_duration() {
        let cb = CircuitBreaker::new(1, 0);
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // reset_duration=0，等待 10ms 确保时间已过
        std::thread::sleep(Duration::from_millis(10));
        // 半开 -> 允许请求，并重置为关闭状态
        assert!(cb.allow_request());
        assert!(!cb.is_circuit_open());
    }

    /// 半开成功后熔断器恢复关闭，后续请求正常通过
    #[test]
    fn test_half_open_success_closes_circuit() {
        let cb = CircuitBreaker::new(2, 0);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());
        std::thread::sleep(Duration::from_millis(10));
        // 第一次 allow_request 进入半开 -> 关闭
        assert!(cb.allow_request());
        // 半开后记录成功，熔断器保持关闭
        cb.record_success();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.failure_count.load(Ordering::Relaxed), 0);
        // 后续请求正常通过
        assert!(cb.allow_request());
    }

    /// 半开后再次失败会重新累计，达到阈值后重新熔断
    #[test]
    fn test_half_open_failure_reopens_circuit() {
        // 使用 1 秒恢复时间，确保重新熔断后短期内不立即恢复
        let cb = CircuitBreaker::new(1, 1);
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // 等待超过恢复时间，进入半开
        std::thread::sleep(Duration::from_millis(1100));
        assert!(cb.allow_request());
        assert!(!cb.is_circuit_open());
        // 立即再次失败（threshold=1），重新熔断
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // 刚熔断，未过恢复时间，请求应被拒绝
        assert!(!cb.allow_request());
    }

    /// 熔断打开状态下 record_success 直接关闭熔断器
    #[test]
    fn test_record_success_during_open_state() {
        let cb = CircuitBreaker::new(2, 60);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_circuit_open());
        // 未过恢复时间，但 record_success 仍可关闭
        cb.record_success();
        assert!(!cb.is_circuit_open());
        assert_eq!(cb.failure_count.load(Ordering::Relaxed), 0);
    }

    /// 成功与失败交替时，失败计数被 record_success 重置，不累计熔断
    #[test]
    fn test_success_resets_failure_accumulation() {
        let cb = CircuitBreaker::new(3, 60);
        // 2 次失败 + 1 次成功 + 2 次失败，不应熔断
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_circuit_open());
        assert!(cb.allow_request());
    }
}
