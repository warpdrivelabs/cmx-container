/// 负载均衡模块，提供数据库连接的负载均衡策略

use rand::Rng;

/// 负载均衡策略：轮询
pub struct RoundRobinLoadBalancing {
    current_index: std::sync::atomic::AtomicUsize,
    db_keys: Vec<String>,
}

impl RoundRobinLoadBalancing {
    /// 创建新的轮询负载均衡器
    pub fn new(db_keys: Vec<String>) -> Self {
        Self {
            current_index: std::sync::atomic::AtomicUsize::new(0),
            db_keys,
        }
    }

    /// 获取下一个数据库键
    pub fn next(&self) -> Option<String> {
        if self.db_keys.is_empty() {
            return None;
        }
        
        let current = self.current_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let index = current % self.db_keys.len();
        Some(self.db_keys[index].clone())
    }
}

/// 负载均衡策略：随机
pub struct RandomLoadBalancing {
    db_keys: Vec<String>,
    rng: rand::rngs::ThreadRng,
}

impl RandomLoadBalancing {
    /// 创建新的随机负载均衡器
    pub fn new(db_keys: Vec<String>) -> Self {
        Self {
            db_keys,
            rng: rand::thread_rng(),
        }
    }

    /// 获取随机数据库键
    pub fn next(&mut self) -> Option<String> {
        if self.db_keys.is_empty() {
            return None;
        }
        
        let index = self.rng.gen_range(0..self.db_keys.len());
        Some(self.db_keys[index].clone())
    }
}
