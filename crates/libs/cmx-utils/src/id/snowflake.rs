//! 雪花算法 ID 生成器模块
//!
//! 提供高性能、趋势递增的分布式 ID 生成功能。

use rand::Rng;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 雪花算法 ID 生成器
///
/// 生成的 ID 结构（64位，从高位到低位）：
/// - 第1位（63）：符号位，始终为0
/// - 第2-42位（62-22）：时间戳（41位，毫秒）
/// - 第43-52位（21-12）：节点 ID（10位）
/// - 第53-64位（11-0）：序列号（12位）
///
/// # 特性
/// - 高性能：单线程可达每秒百万级 ID 生成
/// - 趋势递增：基于时间戳生成，保证 ID 趋势递增
/// - 分布式支持：支持多节点
/// - 线程安全：使用原子操作
pub struct SnowflakeGenerator {
    node_id: i64,
    sequence: AtomicI64,
    last_timestamp: AtomicI64,
}

/// 雪花算法生成器的线程安全引用计数版本
pub type SnowflakeGeneratorArc = Arc<SnowflakeGenerator>;

impl SnowflakeGenerator {
    /// 创建新的雪花算法生成器，使用随机节点 ID
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::SnowflakeGenerator;
    /// let generator = SnowflakeGenerator::new_random();
    /// let id = generator.next_id();
    /// println!("Generated ID: {}", id);
    /// ```
    pub fn new_random() -> Self {
        let mut rng = rand::thread_rng();
        let node_id = rng.gen_range(0..1024) as i64;
        Self::new(node_id)
    }

    /// 使用指定节点 ID 创建雪花算法生成器
    ///
    /// # 参数
    /// * `node_id` - 节点 ID (0-1023)
    ///
    /// # 示例
    /// ```rust
    /// use cmx_utils::id::SnowflakeGenerator;
    /// let generator = SnowflakeGenerator::new(1);
    /// let id = generator.next_id();
    /// println!("Generated ID: {}", id);
    /// ```
    pub fn new(node_id: i64) -> Self {
        Self {
            node_id: node_id % 1024,
            sequence: AtomicI64::new(0),
            last_timestamp: AtomicI64::new(0),
        }
    }

    /// 生成下一个雪花 ID
    ///
    /// # 返回值
    /// * `i64` - 生成的 64 位雪花 ID
    pub fn next_id(&self) -> i64 {
        let timestamp = current_timestamp();

        loop {
            let last_ts = self.last_timestamp.load(Ordering::Acquire);
            let current_seq = self.sequence.load(Ordering::Acquire);

            if timestamp < last_ts {
                continue;
            }

            if timestamp == last_ts {
                let new_seq = current_seq + 1;
                if new_seq >= 4096 {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                }

                if self.sequence
                    .compare_exchange(current_seq, new_seq, Ordering::Release, Ordering::Acquire)
                    .is_ok()
                {
                    let id = (timestamp << 22) | (self.node_id << 12) | new_seq;
                    return id;
                }
            } else {
                let _old_seq = self.sequence.swap(0, Ordering::Release);
                self.last_timestamp.store(timestamp, Ordering::Release);
                let id = (timestamp << 22) | (self.node_id << 12) | 0;
                return id;
            }
        }
    }

    /// 生成雪花 ID 并转换为字符串
    ///
    /// # 返回值
    /// * `String` - 雪花 ID 的字符串形式
    pub fn next_id_str(&self) -> String {
        self.next_id().to_string()
    }

    /// 从雪花 ID 中提取时间戳
    ///
    /// # 参数
    /// * `id` - 雪花 ID
    ///
    /// # 返回值
    /// * `i64` - 时间戳（毫秒）
    pub fn extract_timestamp(id: i64) -> i64 {
        id >> 22
    }

    /// 从雪花 ID 中提取节点 ID
    ///
    /// # 参数
    /// * `id` - 雪花 ID
    ///
    /// # 返回值
    /// * `i64` - 节点 ID
    pub fn extract_node_id(id: i64) -> i64 {
        (id >> 12) & 0x3FF
    }

    /// 从雪花 ID 中提取序列号
    ///
    /// # 参数
    /// * `id` - 雪花 ID
    ///
    /// # 返回值
    /// * `i64` - 序列号
    pub fn extract_sequence(id: i64) -> i64 {
        id & 0xFFF
    }

    /// 获取节点 ID
    pub fn node_id(&self) -> i64 {
        self.node_id
    }
}

impl SnowflakeGenerator {
    /// 创建新的线程安全的雪花算法生成器（Arc 包装），使用随机节点 ID
    pub fn new_random_arc() -> SnowflakeGeneratorArc {
        Arc::new(Self::new_random())
    }

    /// 创建新的线程安全的雪花算法生成器（Arc 包装）
    ///
    /// # 参数
    /// * `node_id` - 节点 ID (0-1023)
    pub fn new_arc(node_id: i64) -> SnowflakeGeneratorArc {
        Arc::new(Self::new(node_id))
    }
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_new() {
        let generator = SnowflakeGenerator::new_random();
        let id = generator.next_id();
        assert!(id > 0);
    }

    #[test]
    fn test_snowflake_unique() {
        let generator = SnowflakeGenerator::new(1);
        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let id = generator.next_id();
            assert!(ids.insert(id), "Duplicate ID found: {}", id);
        }
    }

    #[test]
    fn test_snowflake_with_node() {
        let generator = SnowflakeGenerator::new(42);
        let id = generator.next_id();
        let node = SnowflakeGenerator::extract_node_id(id);
        assert_eq!(node, 42);
    }

    #[test]
    fn test_extract_node_id() {
        let generator = SnowflakeGenerator::new(123);
        let id = generator.next_id();
        assert_eq!(SnowflakeGenerator::extract_node_id(id), 123);
    }

    #[test]
    fn test_extract_sequence() {
        let generator = SnowflakeGenerator::new(1);
        let id = generator.next_id();
        let seq = SnowflakeGenerator::extract_sequence(id);
        assert!(seq >= 0);
    }
}
