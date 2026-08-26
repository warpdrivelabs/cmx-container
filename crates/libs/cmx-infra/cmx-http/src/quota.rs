//! 每插件每分钟请求配额（简单固定窗口计数）。
//!
//! W4 起步用进程内 dashmap；集群级配额可后续接 Redis。窗口按自然分钟切换。

use dashmap::DashMap;

struct Window {
    minute: i64,
    count: u32,
}

/// 每插件配额计数器。
pub struct QuotaTracker {
    windows: DashMap<String, Window>,
}

impl QuotaTracker {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }

    /// 记一次请求；未超 `max_qpm` 返回 true（放行），否则 false（拒绝）。`max_qpm=0` 视为不限。
    pub fn allow(&self, plugin_id: &str, max_qpm: u32) -> bool {
        if max_qpm == 0 {
            return true;
        }
        let now_min = chrono::Utc::now().timestamp() / 60;
        let mut w = self
            .windows
            .entry(plugin_id.to_string())
            .or_insert(Window {
                minute: now_min,
                count: 0,
            });
        if w.minute != now_min {
            w.minute = now_min;
            w.count = 0;
        }
        if w.count >= max_qpm {
            return false;
        }
        w.count += 1;
        true
    }
}

impl Default for QuotaTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_blocks_over_limit() {
        let q = QuotaTracker::new();
        assert!(q.allow("p", 2));
        assert!(q.allow("p", 2));
        assert!(!q.allow("p", 2)); // 第 3 次同分钟内超限。
    }

    #[test]
    fn quota_zero_means_unlimited() {
        let q = QuotaTracker::new();
        for _ in 0..1000 {
            assert!(q.allow("p", 0));
        }
    }

    #[test]
    fn quota_per_plugin_isolated() {
        let q = QuotaTracker::new();
        assert!(q.allow("a", 1));
        assert!(!q.allow("a", 1));
        assert!(q.allow("b", 1)); // b 独立窗口。
    }
}
