//! 构建配额（W1 收尾）—— 并发上限 + 每 key（工作区/租户）每分钟提交上限 + 磁盘配额位。
//!
//! 挂在 [`crate::executor::BuildExecutor::submit`] 前：并发满 → 拒绝（作业置 Failed 或返错）；
//! 频控超限 → 拒绝。避免大量并发 cargo 拖垮机器（方案 W1 风险项）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 配额配置。
#[derive(Debug, Clone)]
pub struct QuotaConfig {
    /// 最大并发构建数（0=不限）。
    pub max_concurrent: usize,
    /// 每 key 每分钟最大提交数（0=不限）。
    pub max_per_min: u32,
    /// 单构建产物/工作区磁盘上限字节（0=不限；由 Build Service 落盘时校验，本模块只存位）。
    pub max_disk_bytes: u64,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            max_per_min: 10,
            max_disk_bytes: 0,
        }
    }
}

/// 拒绝原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaDenied {
    Concurrency { running: usize, max: usize },
    RateLimited { key: String, max: u32 },
}

impl std::fmt::Display for QuotaDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuotaDenied::Concurrency { running, max } => {
                write!(f, "并发构建已满（{running}/{max}），稍后重试")
            }
            QuotaDenied::RateLimited { key, max } => {
                write!(f, "工作区 {key} 提交过于频繁（每分钟上限 {max}）")
            }
        }
    }
}

struct Window {
    minute: i64,
    count: u32,
}

/// 配额守卫：并发计数（放行时返回一个 RAII permit，drop 自动释放）+ 每 key 频控。
pub struct QuotaGuard {
    cfg: QuotaConfig,
    running: Arc<AtomicUsize>,
    windows: Arc<Mutex<HashMap<String, Window>>>,
}

impl QuotaGuard {
    pub fn new(cfg: QuotaConfig) -> Self {
        Self {
            cfg,
            running: Arc::new(AtomicUsize::new(0)),
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 当前在跑数（诊断）。
    pub fn running(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    /// 尝试取一个构建许可。放行返回 [`BuildPermit`]（drop 时释放并发计数）；拒绝返回原因。
    ///
    /// `key` 用于频控（工作区 id 或租户）；传时钟分钟以保确定性单测。
    pub fn try_acquire(&self, key: &str, now_minute: i64) -> Result<BuildPermit, QuotaDenied> {
        // ① 频控（先查，不占并发名额）。
        if self.cfg.max_per_min > 0 {
            let mut w = self.windows.lock().unwrap();
            let win = w.entry(key.to_string()).or_insert(Window { minute: now_minute, count: 0 });
            if win.minute != now_minute {
                win.minute = now_minute;
                win.count = 0;
            }
            if win.count >= self.cfg.max_per_min {
                return Err(QuotaDenied::RateLimited { key: key.into(), max: self.cfg.max_per_min });
            }
            win.count += 1;
        }
        // ② 并发（CAS 递增，满则回退频控计数不必——频控按提交计，超并发也算一次提交尝试）。
        if self.cfg.max_concurrent > 0 {
            let cur = self.running.load(Ordering::SeqCst);
            if cur >= self.cfg.max_concurrent {
                return Err(QuotaDenied::Concurrency { running: cur, max: self.cfg.max_concurrent });
            }
        }
        self.running.fetch_add(1, Ordering::SeqCst);
        Ok(BuildPermit { running: self.running.clone() })
    }
}

/// 构建许可（RAII）：drop 时释放并发计数。
pub struct BuildPermit {
    running: Arc<AtomicUsize>,
}

impl Drop for BuildPermit {
    fn drop(&mut self) {
        self.running.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_caps_and_releases() {
        let g = QuotaGuard::new(QuotaConfig { max_concurrent: 2, max_per_min: 0, max_disk_bytes: 0 });
        let p1 = g.try_acquire("w", 0).unwrap();
        let _p2 = g.try_acquire("w", 0).unwrap();
        assert_eq!(g.running(), 2);
        // 第三个超并发。
        assert!(matches!(g.try_acquire("w", 0), Err(QuotaDenied::Concurrency { .. })));
        drop(p1);
        assert_eq!(g.running(), 1);
        // 释放后可再取。
        assert!(g.try_acquire("w", 0).is_ok());
    }

    #[test]
    fn rate_limit_per_key_per_minute() {
        let g = QuotaGuard::new(QuotaConfig { max_concurrent: 0, max_per_min: 2, max_disk_bytes: 0 });
        assert!(g.try_acquire("a", 100).is_ok());
        assert!(g.try_acquire("a", 100).is_ok());
        assert!(matches!(g.try_acquire("a", 100), Err(QuotaDenied::RateLimited { .. })));
        // 下一分钟窗口重置。
        assert!(g.try_acquire("a", 101).is_ok());
        // 另一 key 独立。
        assert!(g.try_acquire("b", 100).is_ok());
    }

    #[test]
    fn zero_means_unlimited() {
        let g = QuotaGuard::new(QuotaConfig { max_concurrent: 0, max_per_min: 0, max_disk_bytes: 0 });
        for _ in 0..100 {
            let _p = g.try_acquire("w", 0).unwrap();
        }
    }
}
