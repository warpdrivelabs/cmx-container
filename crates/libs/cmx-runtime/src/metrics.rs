//! 运行时监控指标
//!
//! 使用原子类型实现无锁并发安全的指标收集。
//! 通过 `ExtismEngine::get_metrics()` 获取引用进行读取。
//!
//! # 指标项
//!
//! - **total_calls** — 总调用次数（含成功、失败和超时）
//! - **failed_calls** — 失败调用次数（不含超时）
//! - **timeout_calls** — 超时调用次数
//! - **total_elapsed_us** — 累计执行耗时（微秒）
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use std::sync::atomic::Ordering;
//! use cmx_runtime::EngineMetrics;
//!
//! let metrics = EngineMetrics::new();
//! let total = metrics.total_calls.load(Ordering::Relaxed);
//! let failed = metrics.failed_calls.load(Ordering::Relaxed);
//! let avg_latency = metrics.total_elapsed_us.load(Ordering::Relaxed) / total.max(1);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};

/// 运行时监控指标
///
/// 所有字段均为原子类型，支持无锁并发安全的读写。
/// 通过 `Ordering::Relaxed` 保证最终一致性，适合监控场景。
#[derive(Debug, Default)]
pub struct EngineMetrics {
    /// 总调用次数（含成功和失败）
    pub total_calls: AtomicU64,
    /// 失败调用次数（不含超时）
    pub failed_calls: AtomicU64,
    /// 超时调用次数
    pub timeout_calls: AtomicU64,
    /// 累计执行耗时（微秒），可用于计算平均延迟
    pub total_elapsed_us: AtomicU64,
}

impl EngineMetrics {
    /// 创建初始化为零的指标实例
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次成功调用
    ///
    /// 递增总调用计数和累计耗时
    pub fn record_success(&self, elapsed_us: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_elapsed_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// 记录一次失败调用
    ///
    /// 递增总调用计数、失败计数和累计耗时
    pub fn record_failure(&self, elapsed_us: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.failed_calls.fetch_add(1, Ordering::Relaxed);
        self.total_elapsed_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
    }

    /// 记录一次超时调用
    ///
    /// 递增总调用计数、超时计数和累计耗时
    pub fn record_timeout(&self, elapsed_us: u64) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.timeout_calls.fetch_add(1, Ordering::Relaxed);
        self.total_elapsed_us
            .fetch_add(elapsed_us, Ordering::Relaxed);
    }
}
