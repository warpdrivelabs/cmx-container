use std::str::FromStr;

use cmx_utils::ConfigResult;

/// 配置模块，包含数据库和连接池配置结构

/// 数据库类型枚举
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbType {
    Postgres,
    MySql,
    Sqlite,
}

impl FromStr for DbType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgres" | "postgresql" | "pgsql" => Ok(DbType::Postgres),
            "mysql" | "mariadb" => Ok(DbType::MySql),
            "sqlite" | "sqlite3" => Ok(DbType::Sqlite),
            _ => Err(format!("'{}' 不是支持的数据库类型", s)),
        }
    }
}

/// 连接池配置
#[allow(non_snake_case)]
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PoolConfig {
    /// 最大连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// 最小空闲连接数
    #[serde(default = "default_min_connections")]
    pub min_connections: usize,
    /// 连接超时时间（秒）
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,
    /// 空闲连接超时时间（秒）
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,
    /// 最大生命周期（秒）
    #[serde(default = "default_max_lifetime")]
    pub max_lifetime: u64,
}

fn default_max_connections() -> usize {
    if cfg!(test) { 1 } else { 10 }
}

fn default_min_connections() -> usize {
    2
}

fn default_connect_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    600
}

fn default_max_lifetime() -> u64 {
    1800
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout: default_connect_timeout(),
            idle_timeout: default_idle_timeout(),
            max_lifetime: default_max_lifetime(),
        }
    }
}

/// 数据库配置
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DbConfig {
    /// 数据库类型
    pub db_type: DbType,
    /// 数据库连接 URL
    pub db_url: String,
    /// db id
    pub db_id: String,
    /// 数据库 schema pg库默认public
    pub db_schema: Option<String>,
    /// 是否是默认数据库
    #[serde(default)]
    pub default: bool,
    /// 连接池配置
    #[serde(default)]
    pub pool_config: PoolConfig,
    /// 健康检查间隔（秒）
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: u64,
    /// 健康检查超时（秒）
    #[serde(default = "default_health_check_timeout")]
    pub health_check_timeout: u64,
}

fn default_health_check_interval() -> u64 {
    60
}

fn default_health_check_timeout() -> u64 {
    5
}

impl DbConfig {
    /// 从配置中获取数据库配置
    ///
    /// # 参数
    /// - `config`: 配置实例
    ///
    /// # 返回值
    /// 成功返回数据库配置，失败返回错误
    pub fn from_config(config: &cmx_utils::Config) -> ConfigResult<Self> {
        config.get_as("databases")
    }

    /// 从配置中获取数据库配置数组
    ///
    /// # 参数
    /// - `config`: 配置实例
    ///
    /// # 返回值
    /// 成功返回数据库配置数组，失败返回错误
    pub fn list_from_config(config: &cmx_utils::Config) -> ConfigResult<Vec<Self>> {
        config.get_as("databases")
    }
}
