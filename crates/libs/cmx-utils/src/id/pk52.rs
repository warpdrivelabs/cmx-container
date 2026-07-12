//! 52 位 JS 安全主键 ID 生成器（DCT 数据字典 / DOC 业务单据的 `bigint id` 列专用）
//!
//! 与 [`super::snowflake`]（63 位，值 ≈ 7e18）的关键区别：**总位宽 52**，最大值
//! `≈ 4.5e15 < 2^53-1 (9.007e15)`，即 **JS `Number.MAX_SAFE_INTEGER` 安全区内**。
//!
//! 为什么另起一个而非复用 63 位雪花：本系统读路径把 `BIGINT` 列序列化成 **JSON number**
//! （`cmx_core::model::cell` 的 `serialize_i64` + 列式包 `columnar.rs`），若用 63 位雪花，
//! id 一进浏览器就被 JS `Number` 静默截断精度 → id 变号 → 击穿前端按 `row.id` 记键的
//! ChangeSetCollector / 乐观锁。52 位布局让 id 始终是安全整数，读路径与前端**零改造**。
//!
//! # ID 结构（52 位，从高到低）
//! ```text
//! ┌───────────────────────┬──────────┬───────────┐
//! │ 32 bit 秒级时间戳      │ 8 bit    │ 12 bit    │
//! │ （相对自定义 epoch）   │ node     │ sequence  │
//! └───────────────────────┴──────────┴───────────┘
//! ```
//! - **时间戳（32 位）**：`当前秒 - EPOCH_SECS`，跨度 ≈ 136 年。
//! - **node（8 位）**：256 个部署/权威源。每部署一个**固定** node id（配置注入，见 [`get_pk52_generator`]）——
//!   这是「导出包直插他库不撞、不改号」的根基：不同部署 node 段不同 → 号段天然不重叠。
//! - **sequence（12 位）**：同一秒内自增，4096/秒/node；满则自旋等下一秒。
//!
//! # 唯一性 / 免撞
//! - 单调递增（时间在高位）→ B-tree 写入局部性好。
//! - 跨部署不重叠（node 段隔离）→ A 库导出的行原样 INSERT 进 B 库，主键不可能撞。
//! - 撞号唯一来源 = 两部署复用同一 node id，由「node id 登记 + 配置唯一分配」杜绝。

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 自定义纪元：2025-01-01T00:00:00Z 的 UNIX 秒。
/// 相对此值计时，让 32 位时间戳从 0 起算，最大化可用年限（≈ 136 年，到 ~2161）。
pub const EPOCH_SECS: i64 = 1_735_689_600;

const NODE_BITS: u32 = 8;
const SEQ_BITS: u32 = 12;
const MAX_NODE: i64 = (1 << NODE_BITS) - 1; // 255
const MAX_SEQ: i64 = (1 << SEQ_BITS) - 1; // 4095

/// 52 位 JS 安全主键生成器。线程安全（原子序列 + 时间戳）。
pub struct Pk52Generator {
    node_id: i64,
    /// 高 32 位打包「时间戳(秒,相对 epoch)」、低 32 位打包「当前序列」，一次 CAS 原子推进，
    /// 保证 (ts, seq) 组合的原子性（避免两字段分别原子导致的竞态重号）。
    state: AtomicI64,
}

impl Pk52Generator {
    /// 用指定 node id 创建（0..=255，越界取模收敛）。
    pub fn new(node_id: i64) -> Self {
        Self {
            node_id: node_id & MAX_NODE,
            state: AtomicI64::new(0),
        }
    }

    /// 生成下一个 52 位 id。
    ///
    /// 时钟回拨保护：检测到当前秒 < 上次记录秒时**自旋等待**时钟追上，绝不发一个更小/更大的号
    /// （方案 §4.3：宁可短暂等待也不重号）。序列耗尽（同秒 > 4096 个）同样自旋到下一秒。
    pub fn next_id(&self) -> i64 {
        loop {
            let now = current_secs();
            let cur = self.state.load(Ordering::Acquire);
            let last_ts = cur >> 32;
            let last_seq = cur & 0xFFFF_FFFF;

            let (new_ts, new_seq) = if now > last_ts {
                (now, 0)
            } else if now == last_ts {
                if last_seq >= MAX_SEQ {
                    // 本秒序列耗尽 → 等下一秒。
                    std::hint::spin_loop();
                    continue;
                }
                (last_ts, last_seq + 1)
            } else {
                // 时钟回拨：等待挂钟追上上次时间戳，不发号。
                std::hint::spin_loop();
                continue;
            };

            let next = (new_ts << 32) | new_seq;
            if self
                .state
                .compare_exchange(cur, next, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                return ((new_ts - EPOCH_SECS) << (NODE_BITS + SEQ_BITS))
                    | (self.node_id << SEQ_BITS)
                    | new_seq;
            }
            // CAS 失败（并发争用）→ 重试。
        }
    }

    /// 本生成器的 node id。
    pub fn node_id(&self) -> i64 {
        self.node_id
    }

    /// 从 id 提取 node id（诊断/审计用）。
    pub fn extract_node(id: i64) -> i64 {
        (id >> SEQ_BITS) & MAX_NODE
    }

    /// 从 id 提取时间戳（UNIX 秒，诊断/审计用）。
    pub fn extract_epoch_secs(id: i64) -> i64 {
        (id >> (NODE_BITS + SEQ_BITS)) + EPOCH_SECS
    }
}

/// 当前 UNIX 秒（挂钟）。
fn current_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(EPOCH_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_js_safe() {
        // 核心不变量：任何时刻生成的 id 都 < 2^53-1（JS Number.MAX_SAFE_INTEGER）。
        let g = Pk52Generator::new(255);
        let max_safe: i64 = 9_007_199_254_740_991; // 2^53 - 1
        for _ in 0..1000 {
            let id = g.next_id();
            assert!(id > 0, "id 必须为正（符号位 0）");
            assert!(id <= max_safe, "id {id} 超出 JS 安全整数上限");
        }
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let g = Pk52Generator::new(1);
        let mut seen = std::collections::HashSet::new();
        let mut last = 0i64;
        for _ in 0..5000 {
            let id = g.next_id();
            assert!(seen.insert(id), "重复 id: {id}");
            assert!(id >= last, "id 应单调不减");
            last = id;
        }
    }

    #[test]
    fn node_id_roundtrips() {
        let g = Pk52Generator::new(42);
        let id = g.next_id();
        assert_eq!(Pk52Generator::extract_node(id), 42);
    }

    #[test]
    fn different_nodes_never_collide() {
        // 不同 node 的号段不重叠：同一时刻两 node 各生成，集合无交集。
        let a = Pk52Generator::new(7);
        let b = Pk52Generator::new(12);
        let sa: std::collections::HashSet<i64> = (0..500).map(|_| a.next_id()).collect();
        let sb: std::collections::HashSet<i64> = (0..500).map(|_| b.next_id()).collect();
        assert!(sa.is_disjoint(&sb), "不同 node 的 id 号段不应相交");
    }

    #[test]
    fn node_id_clamped() {
        // 越界 node id 收敛到 8 位，不 panic。
        let g = Pk52Generator::new(300);
        assert_eq!(g.node_id(), 300 & MAX_NODE);
    }
}
