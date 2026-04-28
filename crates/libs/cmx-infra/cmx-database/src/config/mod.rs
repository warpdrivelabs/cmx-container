use std::str::FromStr;

use cmx_utils::ConfigResult;

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

    /// 从 db_url 解析数据库名称
    ///
    /// 对于 PostgreSQL/MySQL：从 URL 的 path 部分提取数据库名（去掉前导 `/`）
    /// 对于 SQLite：返回 None
    ///
    /// # 返回值
    /// 解析成功返回数据库名称，解析失败或为 SQLite 类型时返回 None
    pub fn parse_db_name(&self) -> Option<String> {
        // SQLite 不支持从 URL 解析数据库名
        if self.db_type == DbType::Sqlite {
            return None;
        }
        let parsed = url::Url::parse(&self.db_url).ok()?;
        // 从 URL path 中提取数据库名，去掉前导 '/'
        let path = parsed.path();
        let db_name = path.trim_start_matches('/');
        if db_name.is_empty() {
            return None;
        }
        Some(db_name.to_string())
    }

    /// 从 db_url 解析数据库主机地址
    ///
    /// # 返回值
    /// 解析成功返回主机地址字符串，解析失败返回 None
    pub fn parse_db_host(&self) -> Option<String> {
        let parsed = url::Url::parse(&self.db_url).ok()?;
        parsed.host_str().map(|s| s.to_string())
    }

    /// 从 db_url 解析数据库端口号
    ///
    /// # 返回值
    /// 解析成功返回端口号，URL 中未指定端口或解析失败返回 None
    pub fn parse_db_port(&self) -> Option<u16> {
        let parsed = url::Url::parse(&self.db_url).ok()?;
        parsed.port()
    }

    /// 从 db_url 解析数据库用户名
    ///
    /// # 返回值
    /// 解析成功返回用户名字符串，URL 中未包含用户名或解析失败返回 None
    pub fn parse_db_user(&self) -> Option<String> {
        let parsed = url::Url::parse(&self.db_url).ok()?;
        let username = parsed.username();
        if username.is_empty() {
            return None;
        }
        Some(username.to_string())
    }
}
