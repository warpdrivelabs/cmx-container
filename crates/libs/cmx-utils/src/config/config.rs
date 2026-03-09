//! 配置构建器模块
//!
//! 提供配置的构建、合并和访问功能

use std::path::{Path, PathBuf};

use super::error::{ConfigError, ConfigResult};
use super::source::{CommandLineSource, ConfigSource, EnvSource, FileSource};
use super::value::{ConfigStore, ConfigValue, FromConfigValue};
use crate::Priority;
use serde::de::DeserializeOwned;

/// 配置构建器
///
/// 用于构建配置实例，支持多个配置源的合并
pub struct ConfigBuilder {
    /// 配置源列表
    sources: Vec<Box<dyn ConfigSource>>,
}

impl ConfigBuilder {
    /// 创建新的配置构建器
    ///
    /// # 返回值
    /// 返回空的配置构建器实例
    pub fn new() -> Self {
        ConfigBuilder {
            sources: Vec::new(),
        }
    }

    /// 添加配置源
    ///
    /// # 参数
    /// - `source`: 配置源
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_source<S: ConfigSource + 'static>(mut self, source: S) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    /// 添加TOML配置文件（用户指定优先级）
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    /// - `priority`: 配置优先级（0-100）
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_toml_file(mut self, path: impl Into<PathBuf>, priority: u8) -> ConfigResult<Self> {
        let source = FileSource::with_priority(path, priority)?;
        Ok(Self { sources: self.sources, }.add_source(source))
    }

    /// 从环境变量添加TOML配置文件（可选）
    ///
    /// 从指定的环境变量中读取配置文件路径，如果环境变量存在则添加配置源
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    ///
    /// # 示例
    /// ```ignore
    /// // 假设环境变量 CONFIG_FILE=/path/to/config.toml
    /// let builder = Config::builder()
    ///     .add_toml_file_from_env("CONFIG_FILE", Priority::DEFAULT_TOML);
    /// ```
    pub fn add_toml_file_from_env(mut self, env_var: &str, priority: super::source::Priority) -> Self {
        if let Some(source) = FileSource::from_env_var(env_var, priority) {
            self.sources.push(Box::new(source));
        }
        self
    }

    /// 从环境变量添加TOML配置文件（必需）
    ///
    /// 从指定的环境变量中读取配置文件路径，如果环境变量不存在则返回错误
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 成功返回更新后的构建器实例，失败返回错误
    ///
    /// # 示例
    /// ```ignore
    /// // 假设环境变量 CONFIG_FILE=/path/to/config.toml
    /// let builder = Config::builder()
    ///     .add_toml_file_from_env_required("CONFIG_FILE", Priority::DEFAULT_TOML)?;
    /// ```
    pub fn add_toml_file_from_env_required(
        mut self,
        env_var: &str,
        priority: super::source::Priority,
    ) -> ConfigResult<Self> {
        let source = FileSource::from_env_var_required(env_var, priority)?;
        self.sources.push(Box::new(source));
        Ok(self)
    }

    /// 从环境变量添加TOML配置文件（带默认值）
    ///
    /// 从指定的环境变量中读取配置文件路径，如果环境变量不存在则使用默认路径
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `default_path`: 默认配置文件路径
    /// - `priority`: 配置优先级
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    ///
    /// # 示例
    /// ```ignore
    /// // 如果环境变量 CONFIG_FILE 存在，使用其值；否则使用 "config/default.toml"
    /// let builder = Config::builder()
    ///     .add_toml_file_from_env_or("CONFIG_FILE", "config/default.toml", Priority::DEFAULT_TOML);
    /// ```
    pub fn add_toml_file_from_env_or(
        mut self,
        env_var: &str,
        default_path: impl Into<PathBuf>,
        priority: super::source::Priority,
    ) -> Self {
        let source = FileSource::from_env_var_or(env_var, default_path, priority);
        self.sources.push(Box::new(source));
        self
    }

    /// 添加 .env 文件
    ///
    /// # 参数
    /// - `path`: .env 文件路径
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Box::new(FileSource::env_file(path)));
        self
    }

    /// 添加系统环境变量
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env(mut self) -> Self {
        self.sources.push(Box::new(EnvSource::new()));
        self
    }

    /// 添加带前缀的系统环境变量
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env_with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.sources
            .push(Box::new(EnvSource::with_prefix(prefix)));
        self
    }

    /// 添加命令行参数
    ///
    /// # 参数
    /// - `args`: 命令行参数迭代器
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_command_line<I: Iterator<Item = String> + 'static>(mut self, args: I) -> Self {
        self.sources
            .push(Box::new(CommandLineSource::from_args(args)));
        self
    }

    /// 构建配置实例
    ///
    /// 按优先级从低到高合并所有配置源
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn build(self) -> ConfigResult<Config> {
        // 按优先级排序（低优先级在前，高优先级在后）
        let mut sources = self.sources;
        sources.sort_by_key(|s| s.priority());

        // 合并配置
        let mut merged_store = ConfigStore::new();
        let mut source_names = Vec::new();

        for source in sources {
            source_names.push(source.name().to_string());
            match source.load() {
                Ok(store) => merged_store.merge(store),
                Err(ConfigError::FileNotFound { .. }) => {
                    // 文件不存在时跳过，不报错
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(Config {
            store: merged_store,
            sources: source_names,
        })
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 配置实例
///
/// 提供配置的访问和类型转换功能
#[derive(Debug, Clone)]
pub struct Config {
    /// 配置存储
    store: ConfigStore,
    /// 配置源名称列表
    sources: Vec<String>,
}

impl Config {
    /// 创建配置构建器
    ///
    /// # 返回值
    /// 返回新的配置构建器实例
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// 从单个文件创建配置
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_file(path: impl AsRef<Path>) -> ConfigResult<Self> {
        ConfigBuilder::new()
            .add_toml_file(path.as_ref(), 10)?
            .build()
    }

    /// 从环境变量创建配置
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_env() -> ConfigResult<Self> {
        ConfigBuilder::new().add_env().build()
    }

    /// 从环境变量（带前缀）创建配置
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_env_with_prefix(prefix: impl Into<String>) -> ConfigResult<Self> {
        ConfigBuilder::new().add_env_with_prefix(prefix).build()
    }

    /// 获取配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 Some，否则返回 None
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.store.get(key)
    }

    /// 获取配置值并转换为指定类型
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 `FromConfigValue` trait
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回转换后的值，失败返回错误
    ///
    /// # 示例
    /// ```ignore
    /// let config = Config::from_file("config.toml")?;
    /// let host: String = config.get_as("database.host")?;
    /// let port: u16 = config.get_as("database.port")?;
    /// ```
    pub fn get_as<T: FromConfigValue>(&self, key: &str) -> ConfigResult<T> {
        let value = self.store.get(key).ok_or_else(|| ConfigError::KeyNotFound {
            key: key.to_string(),
        })?;
        value.try_into_type()
    }

    /// 获取配置值并转换为指定类型，如果不存在则返回默认值
    ///
    /// # 类型参数
    /// - `T`: 目标类型
    ///
    /// # 参数
    /// - `key`: 配置键
    /// - `default`: 默认值
    ///
    /// # 返回值
    /// 如果配置存在返回配置值，否则返回默认值
    pub fn get_as_or<T: FromConfigValue>(&self, key: &str, default: T) -> T {
        self.get_as(key).unwrap_or(default)
    }

    /// 获取字符串配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回字符串值，失败返回错误
    pub fn get_string(&self, key: &str) -> ConfigResult<String> {
        self.get_as(key)
    }

    /// 获取整数配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回整数值，失败返回错误
    pub fn get_int(&self, key: &str) -> ConfigResult<i64> {
        self.get_as(key)
    }

    /// 获取浮点数配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回浮点数值，失败返回错误
    pub fn get_float(&self, key: &str) -> ConfigResult<f64> {
        self.get_as(key)
    }

    /// 获取布尔配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回布尔值，失败返回错误
    pub fn get_bool(&self, key: &str) -> ConfigResult<bool> {
        self.get_as(key)
    }

    /// 检查配置键是否存在
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 true，否则返回 false
    pub fn contains(&self, key: &str) -> bool {
        self.store.contains_key(key)
    }

    /// 获取所有配置键
    ///
    /// # 返回值
    /// 返回所有配置键的迭代器
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.store.keys()
    }

    /// 获取配置项数量
    ///
    /// # 返回值
    /// 返回配置项的数量
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// 检查配置是否为空
    ///
    /// # 返回值
    /// 如果没有配置项返回 true，否则返回 false
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// 将配置反序列化为结构体
    ///
    /// # 类型参数
    /// - `T`: 目标结构体类型，必须实现 `DeserializeOwned`
    ///
    /// # 返回值
    /// 成功返回反序列化后的结构体，失败返回错误
    ///
    /// # 示例
    /// ```ignore
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct DatabaseConfig {
    ///     host: String,
    ///     port: u16,
    /// }
    ///
    /// let config = Config::from_file("config.toml")?;
    /// let db_config: DatabaseConfig = config.deserialize()?;
    /// ```
    pub fn deserialize<T: DeserializeOwned>(&self) -> ConfigResult<T> {
        // 将扁平化的配置转换为嵌套的 JSON 对象
        let nested_json = self.flatten_to_nested();

        serde_json::from_value(nested_json).map_err(|_e| ConfigError::TypeConversionError {
            key: "root".to_string(),
            target_type: std::any::type_name::<T>().to_string(),
        })
    }

    /// 将扁平化的配置转换为嵌套的 JSON 对象
    ///
    /// # 返回值
    /// 返回嵌套的 JSON 对象
    fn flatten_to_nested(&self) -> serde_json::Value {
        let mut result = serde_json::Map::new();

        for (key, value) in self.store.data() {
            let parts: Vec<&str> = key.split('.').collect();
            insert_nested_value(&mut result, &parts, value);
        }

        serde_json::Value::Object(result)
    }

    /// 获取配置源列表
    ///
    /// # 返回值
    /// 返回配置源名称列表
    pub fn sources(&self) -> &[String] {
        &self.sources
    }

    /// 创建子配置视图
    ///
    /// # 参数
    /// - `prefix`: 配置键前缀
    ///
    /// # 返回值
    /// 返回只包含指定前缀配置的新配置实例
    ///
    /// # 示例
    /// ```ignore
    /// let config = Config::from_file("config.toml")?;
    /// let db_config = config.sub_config("database")?;
    /// let host: String = db_config.get_as("host")?;
    /// ```
    pub fn sub_config(&self, prefix: &str) -> ConfigResult<Config> {
        let mut sub_store = ConfigStore::new();
        let prefix_with_dot = format!("{}.", prefix);

        for key in self.store.keys() {
            if key.starts_with(&prefix_with_dot) {
                if let Some(value) = self.store.get(key) {
                    let sub_key = key[prefix_with_dot.len()..].to_string();
                    sub_store.insert(sub_key, value.clone());
                }
            }
        }

        Ok(Config {
            store: sub_store,
            sources: self.sources.clone(),
        })
    }
}

/// 默认配置加载器
///
/// 提供标准的配置加载流程，按照以下优先级加载：
/// 1. 命令行参数（最高优先级）
/// 2. 系统环境变量
/// 3. .env 文件
/// 4. production.toml 配置文件
/// 5. default.toml 配置文件（最低优先级）
pub struct DefaultConfigLoader {
    /// 配置目录
    config_dir: PathBuf,
    /// 环境变量前缀
    env_prefix: Option<String>,
    /// 是否加载 .env 文件
    load_env_file: bool,
    /// 是否加载系统环境变量
    load_system_env: bool,
    /// 是否加载命令行参数
    load_command_line: bool,
}

impl DefaultConfigLoader {
    /// 创建新的默认配置加载器
    ///
    /// # 参数
    /// - `config_dir`: 配置文件目录
    ///
    /// # 返回值
    /// 返回配置加载器实例
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        DefaultConfigLoader {
            config_dir: config_dir.into(),
            env_prefix: None,
            load_env_file: true,
            load_system_env: true,
            load_command_line: true,
        }
    }

    /// 设置环境变量前缀
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀
    ///
    /// # 返回值
    /// 返回更新后的加载器实例
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());
        self
    }

    /// 设置是否加载 .env 文件
    ///
    /// # 参数
    /// - `load`: 是否加载
    ///
    /// # 返回值
    /// 返回更新后的加载器实例
    pub fn with_env_file(mut self, load: bool) -> Self {
        self.load_env_file = load;
        self
    }

    /// 设置是否加载系统环境变量
    ///
    /// # 参数
    /// - `load`: 是否加载
    ///
    /// # 返回值
    /// 返回更新后的加载器实例
    pub fn with_system_env(mut self, load: bool) -> Self {
        self.load_system_env = load;
        self
    }

    /// 设置是否加载命令行参数
    ///
    /// # 参数
    /// - `load`: 是否加载
    ///
    /// # 返回值
    /// 返回更新后的加载器实例
    pub fn with_command_line(mut self, load: bool) -> Self {
        self.load_command_line = load;
        self
    }

    /// 加载配置
    ///
    /// 按照标准优先级加载配置
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn load(self) -> ConfigResult<Config> {
        let mut builder = Config::builder();

        // 1. 加载 default.toml
        let default_path = self.config_dir.join("default.toml");
        builder = builder.add_toml_file(default_path, 10)?;

        // 2. 加载 env中指定的  toml

        builder =builder.add_toml_file_from_env("CONFIG_FILE",Priority(11));


        // 3. 加载 .env 文件
        if self.load_env_file {
            let env_path = self.config_dir.join(".env");
            builder = builder.add_env_file(env_path);
        }

        // 4. 加载系统环境变量
        if self.load_system_env {
            builder = if let Some(prefix) = self.env_prefix {
                builder.add_env_with_prefix(prefix)
            } else {
                builder.add_env()
            };
        }

        // 5. 加载命令行参数
        if self.load_command_line {
            let args: Vec<String> = std::env::args().skip(1).collect();
            builder = builder.add_command_line(args.into_iter());
        }

        builder.build()
    }
}

/// 将扁平化的键值插入到嵌套的 JSON 对象中
///
/// # 参数
/// - `map`: JSON 对象
/// - `parts`: 键的部分列表
/// - `value`: 配置值
fn insert_nested_value(map: &mut serde_json::Map<String, serde_json::Value>, parts: &[&str], value: &ConfigValue) {
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        // 最后一部分，直接插入值
        map.insert(parts[0].to_string(), config_value_to_json(value));
    } else {
        // 中间部分，需要递归处理
        let key = parts[0];
        let remaining = &parts[1..];

        // 获取或创建子对象
        let child = map.entry(key.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        // 确保子对象是一个 Object
        if let serde_json::Value::Object(child_map) = child {
            insert_nested_value(child_map, remaining, value);
        }
    }
}

/// 将 ConfigValue 转换为 serde_json::Value
///
/// # 参数
/// - `value`: 配置值
///
/// # 返回值
/// 返回 JSON 值
fn config_value_to_json(value: &ConfigValue) -> serde_json::Value {
    match value {
        ConfigValue::String(s) => serde_json::Value::String(s.clone()),
        ConfigValue::Integer(i) => serde_json::Value::Number((*i).into()),
        ConfigValue::Float(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        ConfigValue::Boolean(b) => serde_json::Value::Bool(*b),
        ConfigValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(config_value_to_json).collect())
        }
        ConfigValue::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), config_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        ConfigValue::Null => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemorySource, Priority};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_config_builder() {
        let source = MemorySource::new()
            .with("key1", ConfigValue::new_string("value1"))
            .with("key2", ConfigValue::new_integer(42));

        let config = Config::builder()
            .add_source(source)
            .build()
            .unwrap();

        assert_eq!(config.get_string("key1").unwrap(), "value1");
        assert_eq!(config.get_int("key2").unwrap(), 42);
    }

    #[test]
    fn test_config_merge() {
        let source1 = MemorySource::new()
            .with("key1", ConfigValue::new_string("value1"))
            .with_priority(Priority::DEFAULT_TOML);

        let source2 = MemorySource::new()
            .with("key1", ConfigValue::new_string("value2"))
            .with("key2", ConfigValue::new_string("value2"))
            .with_priority(Priority::SYSTEM_ENV);

        let config = Config::builder()
            .add_source(source1)
            .add_source(source2)
            .build()
            .unwrap();

        // 高优先级覆盖低优先级
        assert_eq!(config.get_string("key1").unwrap(), "value2");
        // 低优先级的配置保留
        assert_eq!(config.get_string("key2").unwrap(), "value2");
    }

    #[test]
    fn test_config_type_conversion() {
        let source = MemorySource::new()
            .with("string_val", ConfigValue::new_string("hello"))
            .with("int_val", ConfigValue::new_integer(42))
            .with("float_val", ConfigValue::new_float(3.14))
            .with("bool_val", ConfigValue::new_boolean(true));

        let config = Config::builder()
            .add_source(source)
            .build()
            .unwrap();

        assert_eq!(config.get_string("string_val").unwrap(), "hello");
        assert_eq!(config.get_int("int_val").unwrap(), 42);
        assert!((config.get_float("float_val").unwrap() - 3.14).abs() < 0.001);
        assert_eq!(config.get_bool("bool_val").unwrap(), true);
    }

    #[test]
    fn test_config_get_or() {
        let source = MemorySource::new()
            .with("existing", ConfigValue::new_string("value"));

        let config = Config::builder()
            .add_source(source)
            .build()
            .unwrap();

        assert_eq!(config.get_as_or("existing", "default".to_string()), "value");
        assert_eq!(config.get_as_or("non_existing", "default".to_string()), "default");
    }

    #[test]
    fn test_config_from_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(b"key = \"value\"\nnumber = 42").unwrap();

        let config = Config::builder()
            .add_toml_file(&config_path, 10)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(config.get_string("key").unwrap(), "value");
        assert_eq!(config.get_int("number").unwrap(), 42);
    }
}
