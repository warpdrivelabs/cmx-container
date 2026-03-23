use std::str::FromStr;
use cmx_utils::{ConfigError, ConfigResult, ConfigValue, FromConfigValue};

/// 配置模块，包含数据库和连接池配置结构

/// 数据库类型枚举
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone,Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DbConfig {
    /// 数据库类型
    pub db_type: DbType,
    /// 数据库连接 URL
    pub db_url: String,
    /// db id
    pub db_id: String,
    /// 数据库 schema pg库默认publc
    pub db_schema: Option<String>,
    /// 是否是默认数据库
    pub default: bool,

    /// 连接池配置
    pub pool_config: PoolConfig,
    /// 健康检查间隔（秒）
    pub health_check_interval: u64,
    /// 健康检查超时（秒）
    pub health_check_timeout: u64,
}

// impl Default for DbConfig {
//     fn default() -> Self {
//         Self {
//             db_type: DbType::Postgres,
//             db_id: "default".to_string(),
//             db_url: "postgresql://localhost/test".to_string(),
//             pool_config: PoolConfig::default(),
//             health_check_interval: 60,
//             health_check_timeout: 5,
//             default: false
//         }
//     }
// }


/// 从 ConfigValue 转换为 DbConfig
impl FromConfigValue for DbConfig {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Object(map) => {
                let db_type = get_string_field(map, "db_type")
                    .and_then(|s| DbType::from_str(&s).map_err(|e| ConfigError::TypeConversionError {
                        key: "db_type".to_string(),
                        target_type: e,
                    }))?;

                let db_url = get_string_field(map, "db_url")?;
                let db_id = get_string_field(map, "db_id")?;
                let default = get_bool_field(map, "default").unwrap_or(false);
                let db_schema = get_string_field(map, "db_schema").unwrap_or("public".to_string());

                let pool_config = get_object_field(map, "pool_config")
                    .and_then(|v| v.try_into_type().ok())
                    .unwrap_or_default();

                let health_check_interval = get_int_field(map, "health_check_interval").unwrap_or(60) as u64;
                let health_check_timeout = get_int_field(map, "health_check_timeout").unwrap_or(5) as u64;

                Ok(DbConfig {
                    db_type,
                    db_url,
                    db_id,
                    db_schema: Some(db_schema),
                    default,
                    pool_config,
                    health_check_interval,
                    health_check_timeout,
                })
            }
            _ => Err(ConfigError::TypeConversionError {
                key: "databases".to_string(),
                target_type: "DbConfig".to_string(),
            }),
        }
    }
}



/// 从 ConfigValue 转换为 PoolConfig
impl FromConfigValue for PoolConfig {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Object(map) => {
                let max_connections = get_int_field(map, "max_connections").unwrap_or(10) as usize;
                let min_connections = get_int_field(map, "min_connections").unwrap_or(2) as usize;
                let connect_timeout = get_int_field(map, "connect_timeout").unwrap_or(30) as u64;
                let idle_timeout = get_int_field(map, "idle_timeout").unwrap_or(600) as u64;
                let max_lifetime = get_int_field(map, "max_lifetime").unwrap_or(1800) as u64;

                Ok(PoolConfig {
                    max_connections,
                    min_connections,
                    connect_timeout,
                    idle_timeout,
                    max_lifetime,
                })
            }
            _ => Err(ConfigError::TypeConversionError {
                key: "pool_config".to_string(),
                target_type: "PoolConfig".to_string(),
            }),
        }
    }
}

/// 从对象字段中获取字符串值
fn get_string_field(map: &std::collections::HashMap<String, ConfigValue>, key: &str) -> ConfigResult<String> {
    map.get(key)
        .ok_or_else(|| ConfigError::KeyNotFound { key: key.to_string() })
        .and_then(|v| String::from_config_value(v))
}

/// 从对象字段中获取整数值
fn get_int_field(map: &std::collections::HashMap<String, ConfigValue>, key: &str) -> Option<i64> {
    map.get(key).and_then(|v| {
        if let ConfigValue::Integer(i) = v {
            Some(*i)
        } else {
            None
        }
    })
}

/// 从对象字段中获取布尔值
fn get_bool_field(map: &std::collections::HashMap<String, ConfigValue>, key: &str) -> Option<bool> {
    map.get(key).and_then(|v| {
        if let ConfigValue::Boolean(b) = v {
            Some(*b)
        } else {
            None
        }
    })
}

/// 从对象字段中获取对象值
fn get_object_field(map: &std::collections::HashMap<String, ConfigValue>, key: &str) -> Option<ConfigValue> {
    map.get(key).cloned()
}
