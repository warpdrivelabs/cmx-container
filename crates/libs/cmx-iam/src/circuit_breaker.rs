//! 熔断器
//!
//! 简化版熔断器（closed/open 两态），通过 reset_duration 自动恢复。
//! 当连续失败次数达到阈值时打开熔断器，拒绝后续请求；
//! 经过 reset_duration 后自动进入半开状态，允许请求通过。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 熔断器
pub struct CircuitBreaker {
    /// 连续失败次数
    failure_count: AtomicU32,
    /// 熔断器是否打开
    is_open: AtomicBool,
    /// 最后一次失败时间
    last_failure_time: Mutex<Option<Instant>>,
    /// 熔断阈值
    threshold: u32,
    /// 熔断恢复时间
    reset_duration: Duration,
}

impl CircuitBreaker {
    /// 创建新熔断器
    pub fn new(threshold: u32, reset_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            is_open: AtomicBool::new(false),
            last_failure_time: Mutex::new(None),
            threshold,
            reset_duration: Duration::from_secs(reset_secs),
        }
    }

    /// 检查是否允许请求通过
    ///
    /// - 熔断器关闭时：允许
    /// - 熔断器打开且超过恢复时间：半开，允许（失败会重新打开）
    /// - 熔断器打开且未超过恢复时间：拒绝
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

    /// 记录成功
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.is_open.store(false, Ordering::Relaxed);
    }

    /// 记录失败
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.threshold {
            self.is_open.store(true, Ordering::Relaxed);
            let mut last = self.last_failure_time.lock().unwrap();
            *last = Some(Instant::now());
        }
    }

    /// 检查熔断器是否打开
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
}
