//! 配置来源抽象模块
//!
//! 定义配置来源的统一接口和具体实现，支持多种配置来源

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use super::error::{ConfigError, ConfigResult};
use super::parser::parse_file_auto;
use super::value::{ConfigStore, ConfigValue};

/// 配置来源优先级
///
/// 数值越大优先级越高，高优先级的配置会覆盖低优先级的配置
/// 优先级范围：0-100
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub u8);

impl Priority {
    /// 命令行参数优先级（最高）
    pub const COMMAND_LINE: Priority = Priority(100);

    /// 系统环境变量优先级
    pub const SYSTEM_ENV: Priority = Priority(80);

    /// .env 文件优先级
    pub const ENV_FILE: Priority = Priority(60);

    /// 默认的TOML配置文件优先级（用户可以根据需要调整）
    pub const DEFAULT_TOML: Priority = Priority(10);

    /// 创建新的优先级
    ///
    /// # 参数
    /// - `value`: 优先级值（0-100）
    ///
    /// # 返回值
    /// 成功返回优先级实例，失败返回错误
    pub fn new(value: u8) -> ConfigResult<Self> {
        if value > 100 {
            return Err(ConfigError::InvalidPriority { priority: value });
        }
        Ok(Priority(value))
    }
}

/// 配置来源 trait
///
/// 定义配置来源的统一接口
pub trait ConfigSource: Send + Sync {
    /// 加载配置
    ///
    /// # 返回值
    /// 成功返回配置存储，失败返回错误
    fn load(&self) -> ConfigResult<ConfigStore>;

    /// 获取配置来源名称
    ///
    /// # 返回值
    /// 返回配置来源的名称标识
    fn name(&self) -> &str;

    /// 获取配置来源优先级
    ///
    /// # 返回值
    /// 返回配置来源的优先级
    fn priority(&self) -> Priority;
}

/// 文件配置来源
///
/// 从文件系统加载配置文件
pub struct FileSource {
    /// 文件路径
    path: PathBuf,
    /// 配置来源名称
    name: String,
    /// 优先级
    priority: Priority,
}

impl FileSource {
    /// 创建新的文件配置来源（用户指定优先级）
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    /// - `priority`: 配置优先级（0-100）
    ///
    /// # 返回值
    /// 返回文件配置来源实例
    pub fn with_priority(path: impl Into<PathBuf>, priority: u8) -> ConfigResult<Self> {
        let priority = Priority::new(priority)?;
        let path = path.into();
        let name = format!("file:{}", path.display());
        Ok(FileSource {
            path,
            name,
            priority,
        })
    }

    /// 创建新的文件配置来源（使用Priority结构体）
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 返回文件配置来源实例
    pub fn new(path: impl Into<PathBuf>, priority: Priority) -> Self {
        let path = path.into();
        let name = format!("file:{}", path.display());
        FileSource {
            path,
            name,
            priority,
        }
    }

    /// 创建 .env 文件来源
    ///
    /// # 参数
    /// - `path`: .env 文件路径
    ///
    /// # 返回值
    /// 返回优先级为 ENV_FILE 的文件配置来源
    pub fn env_file(path: impl Into<PathBuf>) -> Self {
        Self::new(path, Priority::ENV_FILE)
    }

    /// 从环境变量创建文件配置来源
    ///
    /// 从指定的环境变量中读取配置文件路径，并创建文件配置来源
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 成功返回文件配置来源实例，如果环境变量不存在则返回 None
    ///
    /// # 示例
    /// ```ignore
    /// // 假设环境变量 CONFIG_FILE=/path/to/config.toml
    /// let source = FileSource::from_env_var("CONFIG_FILE", Priority::DEFAULT_TOML);
    /// if let Some(source) = source {
    ///     builder = builder.add_source(source);
    /// }
    /// ```
    pub fn from_env_var(env_var: &str, priority: Priority) -> Option<Self> {
        env::var(env_var).ok().map(|path| {
            let path = PathBuf::from(path);
            let name = format!("file:env_var:{}:{}", env_var, path.display());
            FileSource {
                path,
                name,
                priority,
            }
        })
    }

    /// 从环境变量创建文件配置来源（必需）
    ///
    /// 从指定的环境变量中读取配置文件路径，并创建文件配置来源
    /// 如果环境变量不存在，则返回错误
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 成功返回文件配置来源实例，失败返回错误
    ///
    /// # 示例
    /// ```ignore
    /// // 假设环境变量 CONFIG_FILE=/path/to/config.toml
    /// let source = FileSource::from_env_var_required("CONFIG_FILE", Priority::DEFAULT_TOML)?;
    /// builder = builder.add_source(source);
    /// ```
    pub fn from_env_var_required(env_var: &str, priority: Priority) -> ConfigResult<Self> {
        match env::var(env_var) {
            Ok(path) => {
                let path = PathBuf::from(path);
                let name = format!("file:env_var:{}:{}", env_var, path.display());
                Ok(FileSource {
                    path,
                    name,
                    priority,
                })
            }
            Err(_) => Err(ConfigError::EnvVarError {
                var_name: env_var.to_string(),
            }),
        }
    }

    /// 从环境变量创建文件配置来源（带默认值）
    ///
    /// 从指定的环境变量中读取配置文件路径，如果环境变量不存在则使用默认路径
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `default_path`: 默认配置文件路径
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 返回文件配置来源实例
    ///
    /// # 示例
    /// ```ignore
    /// // 如果环境变量 CONFIG_FILE 存在，使用其值；否则使用 "config/default.toml"
    /// let source = FileSource::from_env_var_or("CONFIG_FILE", "config/default.toml", Priority::DEFAULT_TOML);
    /// builder = builder.add_source(source);
    /// ```
    pub fn from_env_var_or(
        env_var: &str,
        default_path: impl Into<PathBuf>,
        priority: Priority,
    ) -> Self {
        let path = env::var(env_var)
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_path.into());
        let name = format!("file:env_var:{}:{}", env_var, path.display());
        FileSource {
            path,
            name,
            priority,
        }
    }
}

impl ConfigSource for FileSource {
    fn load(&self) -> ConfigResult<ConfigStore> {
        if !self.path.exists() {
            return Err(ConfigError::FileNotFound {
                path: self.path.clone(),
            });
        }
        parse_file_auto(&self.path)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        self.priority
    }
}

/// 系统环境变量配置来源
///
/// 从系统环境变量加载配置
pub struct EnvSource {
    /// 前缀过滤器（可选）
    prefix: Option<String>,
    /// 配置来源名称
    name: String,
}

impl EnvSource {
    /// 创建新的环境变量配置来源
    ///
    /// # 返回值
    /// 返回环境变量配置来源实例
    pub fn new() -> Self {
        EnvSource {
            prefix: None,
            name: "system_env".to_string(),
        }
    }

    /// 创建带前缀过滤的环境变量配置来源
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀，只有以此前缀开头的环境变量才会被加载
    ///
    /// # 返回值
    /// 返回环境变量配置来源实例
    ///
    /// # 示例
    /// ```ignore
    /// let source = EnvSource::with_prefix("APP_");
    /// // 只加载 APP_ 开头的环境变量，如 APP_HOST, APP_PORT 等
    /// ```
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        let prefix = prefix.into();
        EnvSource {
            prefix: Some(prefix.clone()),
            name: format!("env:prefix={}", prefix),
        }
    }

    /// 移除环境变量名的前缀
    ///
    /// # 参数
    /// - `key`: 环境变量名
    ///
    /// # 返回值
    /// 返回移除前缀后的键名
    fn strip_prefix(&self, key: &str) -> String {
        if let Some(ref prefix) = self.prefix {
            if key.starts_with(prefix) {
                return key[prefix.len()..].to_string();
            }
        }
        key.to_string()
    }

    /// 规范化环境变量名
    ///
    /// 将环境变量名转换为配置键名：
    /// 2. 将双下划线 `__` 转换为点号 `.`（用于嵌套配置）
    /// 3. 将单下划线 `_` 保持不变（用于单词分隔）
    ///
    /// # 参数
    /// - `key`: 环境变量名
    ///
    /// # 返回值
    /// 返回规范化后的配置键名
    ///
    /// # 示例
    /// ```ignore
    /// // DB_HOST -> db_host
    /// // DATABASE__HOST -> database.host
    /// // DATABASE__CONNECTION__TIMEOUT -> database.connection.timeout
    /// ```
    fn normalize_env_key(&self, key: &str) -> String {
        //暂时先不转换小写
        let lower = key.to_string();

        // 将双下划线替换为点号（用于嵌套配置）
        // 注意：要先替换双下划线，再处理单下划线
        let normalized = lower.replace("__", ".");

        normalized
    }
}

impl Default for EnvSource {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigSource for EnvSource {
    fn load(&self) -> ConfigResult<ConfigStore> {
        let mut store = ConfigStore::new();

        for (key, value) in env::vars() {
            // 如果设置了前缀，只加载匹配的环境变量
            if let Some(ref prefix) = self.prefix {
                if !key.starts_with(prefix) {
                    continue;
                }
            }

            // 先移除前缀
            let config_key = self.strip_prefix(&key);
            // 再规范化键名（双下划线转点号）
            let normalized_key = self.normalize_env_key(&config_key);

            // 环境变量值统一作为字符串处理，类型推断在后续使用时进行
            store.insert(normalized_key, ConfigValue::String(value));
        }

        Ok(store)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::SYSTEM_ENV
    }
}

/// 命令行参数配置来源
///
/// 从命令行参数加载配置
pub struct CommandLineSource {
    /// 配置键值对
    args: HashMap<String, String>,
    /// 配置来源名称
    name: String,
}

impl CommandLineSource {
    /// 从命令行参数创建配置来源
    ///
    /// # 参数
    /// - `args`: 命令行参数迭代器
    ///
    /// # 返回值
    /// 返回命令行参数配置来源实例
    ///
    /// # 示例
    /// ```ignore
    /// let source = CommandLineSource::from_args(std::env::args().skip(1));
    /// ```
    pub fn from_args<I: Iterator<Item = String>>(args: I) -> Self {
        let mut config_args = HashMap::new();
        let mut iter = args.peekable();

        while let Some(arg) = iter.next() {
            // 支持两种格式：
            // 1. --key=value
            // 2. --key value
            if arg.starts_with("--") {
                let arg_content = &arg[2..];

                if let Some(eq_pos) = arg_content.find('=') {
                    // --key=value 格式
                    let key = arg_content[..eq_pos].to_string();
                    let value = arg_content[eq_pos + 1..].to_string();
                    config_args.insert(key, value);
                } else if let Some(next_arg) = iter.peek() {
                    // --key value 格式
                    if !next_arg.starts_with("--") {
                        let key = arg_content.to_string();
                        let value = next_arg.clone();
                        config_args.insert(key, value);
                        iter.next(); // 消费下一个参数
                    }
                }
            }
        }

        CommandLineSource {
            args: config_args,
            name: "command_line".to_string(),
        }
    }

    /// 从键值对映射创建配置来源
    ///
    /// # 参数
    /// - `args`: 键值对映射
    ///
    /// # 返回值
    /// 返回命令行参数配置来源实例
    pub fn from_map(args: HashMap<String, String>) -> Self {
        CommandLineSource {
            args,
            name: "command_line".to_string(),
        }
    }
}

impl ConfigSource for CommandLineSource {
    fn load(&self) -> ConfigResult<ConfigStore> {
        let mut store = ConfigStore::new();

        for (key, value) in &self.args {
            // 命令行参数值统一作为字符串处理
            store.insert(key.clone(), ConfigValue::String(value.clone()));
        }

        Ok(store)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        Priority::COMMAND_LINE
    }
}

/// 内存配置来源
///
/// 从内存中的键值对创建配置来源，主要用于测试
pub struct MemorySource {
    /// 配置存储
    store: ConfigStore,
    /// 配置来源名称
    name: String,
    /// 优先级
    priority: Priority,
}

impl MemorySource {
    /// 创建新的内存配置来源
    ///
    /// # 返回值
    /// 返回内存配置来源实例
    pub fn new() -> Self {
        MemorySource {
            store: ConfigStore::new(),
            name: "memory".to_string(),
            priority: Priority::DEFAULT_TOML,
        }
    }

    /// 设置配置项
    ///
    /// # 参数
    /// - `key`: 配置键
    /// - `value`: 配置值
    ///
    /// # 返回值
    /// 返回更新后的配置来源实例
    pub fn with(mut self, key: impl Into<String>, value: ConfigValue) -> Self {
        self.store.insert(key, value);
        self
    }

    /// 设置优先级
    ///
    /// # 参数
    /// - `priority`: 优先级
    ///
    /// # 返回值
    /// 返回更新后的配置来源实例
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// 设置名称
    ///
    /// # 参数
    /// - `name`: 配置来源名称
    ///
    /// # 返回值
    /// 返回更新后的配置来源实例
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for MemorySource {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigSource for MemorySource {
    fn load(&self) -> ConfigResult<ConfigStore> {
        Ok(self.store.clone())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> Priority {
        self.priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_source() {
        let source = MemorySource::new()
            .with("key1", ConfigValue::new_string("value1"))
            .with("key2", ConfigValue::new_integer(42))
            .with_priority(Priority::COMMAND_LINE);

        assert_eq!(source.name(), "memory");
        assert_eq!(source.priority(), Priority::COMMAND_LINE);

        let store = source.load().unwrap();
        assert_eq!(store.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(store.get("key2").unwrap().as_integer().unwrap(), 42);
    }

    #[test]
    fn test_command_line_source() {
        let args = vec![
            "--host".to_string(),
            "localhost".to_string(),
            "--port=8080".to_string(),
            "--debug".to_string(),
            "true".to_string(),
        ];

        let source = CommandLineSource::from_args(args.into_iter());
        let store = source.load().unwrap();

        assert_eq!(store.get("host").unwrap().as_str().unwrap(), "localhost");
        assert_eq!(store.get("port").unwrap().as_str().unwrap(), "8080");
        assert_eq!(store.get("debug").unwrap().as_str().unwrap(), "true");
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::COMMAND_LINE > Priority::SYSTEM_ENV);
        assert!(Priority::SYSTEM_ENV > Priority::ENV_FILE);
        assert!(Priority::ENV_FILE > Priority::DEFAULT_TOML);
    }

    #[test]
    fn test_priority_new() {
        // 测试有效优先级
        assert!(Priority::new(50).is_ok());
        assert!(Priority::new(100).is_ok());
        assert!(Priority::new(0).is_ok());

        // 测试无效优先级
        assert!(Priority::new(101).is_err());
    }

    #[test]
    fn test_file_source_with_priority() {
        // 测试创建带优先级的文件源
        let source = FileSource::with_priority("config.toml", 30);
        assert!(source.is_ok());
        let source = source.unwrap();
        assert_eq!(source.priority(), Priority(30));
    }
}
