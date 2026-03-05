/// 配置模块，包含数据库和连接池配置结构

/// 数据库类型枚举
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbType {
    Postgres,
    MySql,
    Sqlite,
}

/// 连接池配置
#[allow(non_snake_case)]
#[derive(Clone,Debug)]
pub struct PoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 最小空闲连接数
    pub min_connections: usize,
    /// 连接超时时间（秒）
    pub connect_timeout: u64,
    /// 空闲连接超时时间（秒）
    pub idle_timeout: u64,
    /// 最大生命周期（秒）
    pub max_lifetime: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: if cfg!(test) { 1 } else { 10 },
            min_connections: 2,
            connect_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        }
    }
}

/// 数据库配置
#[allow(non_snake_case)]
#[derive(Clone)]
pub struct DbConfig {
    /// 数据库类型
    pub db_type: DbType,
    /// 数据库连接 URL
    pub db_url: String,
    /// 连接池配置
    pub pool_config: PoolConfig,
    /// 健康检查间隔（秒）
    pub health_check_interval: u64,
    /// 健康检查超时（秒）
    pub health_check_timeout: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            db_type: DbType::Postgres,
            db_url: "postgresql://localhost/test".to_string(),
            pool_config: PoolConfig::default(),
            health_check_interval: 60,
            health_check_timeout: 5,
        }
    }
}
