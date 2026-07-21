//! 配置构建器模块
//!
//! 基于 `config` crate 提供配置的构建、合并和访问功能
//! 保留 `ConfigManager` 全局单例和 `DefaultConfigLoader` 标准加载流程

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use super::error::{ConfigError, ConfigResult};
use super::source::CommandLineSource;
use super::value::ConfigValue;
use serde::de::DeserializeOwned;

/// 配置构建器
///
/// 基于 `config::ConfigBuilder` 的薄封装，支持多种配置源的链式组合。
/// 后添加的 source 优先级更高，会覆盖先添加的同名配置。
///
/// # 配置优先级（从低到高）
/// 1. TOML 配置文件
/// 2. 系统环境变量
/// 3. 命令行参数
pub struct ConfigBuilder {
    /// 底层 config crate 构建器
    inner: config::ConfigBuilder<config::builder::DefaultState>,
}

impl ConfigBuilder {
    /// 创建新的配置构建器
    ///
    /// # 返回值
    /// 返回空的配置构建器实例
    pub fn new() -> Self {
        ConfigBuilder {
            inner: config::Config::builder(),
        }
    }

    /// 添加 TOML 配置文件
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_toml_file(self, path: impl Into<PathBuf>) -> ConfigResult<Self> {
        let path = path.into();
        let builder = self.inner.add_source(
            config::File::new(path.to_str().unwrap_or(""), config::FileFormat::Toml)
                .required(false),
        );
        Ok(ConfigBuilder { inner: builder })
    }

    /// 从环境变量添加 TOML 配置文件（可选）
    ///
    /// 如果指定的环境变量存在，则加载其指向的 TOML 文件
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_toml_file_from_env(self, env_var: &str) -> Self {
        if let Ok(path) = std::env::var(env_var) {
            let path_buf = PathBuf::from(&path);
            let builder = self.inner.add_source(
                config::File::new(path_buf.to_str().unwrap_or(""), config::FileFormat::Toml)
                    .required(true),
            );
            return ConfigBuilder { inner: builder };
        }
        self
    }

    /// 从环境变量添加 TOML 配置文件（必需）
    ///
    /// 如果指定的环境变量不存在则返回错误
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `priority`: 优先级参数（保留以兼容旧 API）
    ///
    /// # 返回值
    /// 成功返回更新后的构建器实例，失败返回错误
    pub fn add_toml_file_from_env_required(
        self,
        env_var: &str,
        _priority: u8,
    ) -> ConfigResult<Self> {
        let path = std::env::var(env_var).map_err(|_| ConfigError::EnvVarError {
            var_name: env_var.to_string(),
        })?;
        let path_buf = PathBuf::from(&path);
        let builder = self.inner.add_source(
            config::File::new(path_buf.to_str().unwrap_or(""), config::FileFormat::Toml)
                .required(true),
        );
        Ok(ConfigBuilder { inner: builder })
    }

    /// 从环境变量添加 TOML 配置文件（带默认值）
    ///
    /// 如果指定的环境变量不存在，则使用默认路径
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `default_path`: 默认配置文件路径
    /// - `priority`: 优先级参数（保留以兼容旧 API）
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_toml_file_from_env_or(
        self,
        env_var: &str,
        default_path: impl Into<PathBuf>,
        _priority: u8,
    ) -> Self {
        let path = std::env::var(env_var)
            .unwrap_or_else(|_| default_path.into().to_string_lossy().to_string());
        let path_buf = PathBuf::from(&path);
        let builder = self.inner.add_source(
            config::File::new(path_buf.to_str().unwrap_or(""), config::FileFormat::Toml)
                .required(false),
        );
        ConfigBuilder { inner: builder }
    }

    /// 添加 .env 文件（通过 dotenvy 加载到环境变量中）
    ///
    /// # 参数
    /// - `path`: .env 文件路径
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env_file(self, path: impl Into<PathBuf>) -> Self {
        let _path = path.into();
        if let Err(e) = dotenvy::from_path(&_path) {
            tracing::debug!("加载 .env 文件失败（可能不存在）: {:?}", e);
        }
        self
    }

    /// 添加系统环境变量
    ///
    /// 加载所有系统环境变量到配置中
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env(self) -> Self {
        let builder = self.inner.add_source(
            config::Environment::default()
                .separator("__")
                .try_parsing(true),
        );
        ConfigBuilder { inner: builder }
    }

    /// 添加带前缀的系统环境变量
    ///
    /// 只加载以指定前缀开头的环境变量，并自动去除前缀
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_env_with_prefix(self, prefix: impl Into<String>) -> Self {
        let builder = self.inner.add_source(
            config::Environment::with_prefix(&prefix.into())
                .separator(".")
                .try_parsing(true),
        );
        ConfigBuilder { inner: builder }
    }

    /// 添加命令行参数
    ///
    /// 支持 `--key=value` 和 `--key value` 两种格式
    ///
    /// # 参数
    /// - `args`: 命令行参数迭代器
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_command_line<I: Iterator<Item = String> + 'static>(self, args: I) -> Self {
        let builder = self.inner.add_source(CommandLineSource::from_args(args));
        ConfigBuilder { inner: builder }
    }

    /// 添加配置源（通用方法）
    ///
    /// # 参数
    /// - `source`: 实现了 `config::Source` trait 的配置源
    ///
    /// # 返回值
    /// 返回更新后的构建器实例
    pub fn add_source<S: config::Source + Send + Sync + 'static>(self, source: S) -> Self {
        let builder = self.inner.add_source(source);
        ConfigBuilder { inner: builder }
    }

    /// 构建配置实例
    ///
    /// 按照添加顺序合并所有配置源，后添加的覆盖先添加的
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn build(self) -> ConfigResult<Config> {
        let inner = self.inner.build()?;
        Ok(Config { inner })
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 配置实例
///
/// 基于 `config::Config` 的薄封装，提供配置的访问和类型转换功能。
/// 支持点分隔的键名访问嵌套配置（如 `database.host`）。
#[derive(Debug, Clone)]
pub struct Config {
    /// 底层 config crate 配置实例
    inner: config::Config,
}

impl Config {
    /// 创建配置构建器
    ///
    /// # 返回值
    /// 返回新的配置构建器实例
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// 从单个 TOML 文件创建配置
    ///
    /// # 参数
    /// - `path`: 配置文件路径
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_file(path: impl AsRef<Path>) -> ConfigResult<Self> {
        let path = path.as_ref();
        let inner = config::Config::builder()
            .add_source(
                config::File::new(path.to_str().unwrap_or(""), config::FileFormat::Toml)
                    .required(true),
            )
            .build()?;
        Ok(Config { inner })
    }

    /// 从环境变量创建配置
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_env() -> ConfigResult<Self> {
        let inner = config::Config::builder()
            .add_source(
                config::Environment::default()
                    .separator(".")
                    .try_parsing(true),
            )
            .build()?;
        Ok(Config { inner })
    }

    /// 从带前缀的环境变量创建配置
    ///
    /// # 参数
    /// - `prefix`: 环境变量前缀
    ///
    /// # 返回值
    /// 成功返回配置实例，失败返回错误
    pub fn from_env_with_prefix(prefix: impl Into<String>) -> ConfigResult<Self> {
        let inner = config::Config::builder()
            .add_source(
                config::Environment::with_prefix(&prefix.into())
                    .separator(".")
                    .try_parsing(true),
            )
            .build()?;
        Ok(Config { inner })
    }

    /// 获取配置值（原始类型）
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 Some，否则返回 None
    pub fn get(&self, key: &str) -> Option<config::Value> {
        self.inner.get(key).ok()
    }

    /// 获取配置值并转换为指定类型
    ///
    /// 通过 serde 反序列化为目标类型，支持所有实现了 `Deserialize` 的类型
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 `Deserialize`
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回转换后的值，失败返回错误
    pub fn get_as<T: DeserializeOwned>(&self, key: &str) -> ConfigResult<T> {
        self.inner.get::<T>(key).map_err(ConfigError::from)
    }

    /// 获取配置值并转换为指定类型，如果不存在则返回默认值
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 `DeserializeOwned`
    ///
    /// # 参数
    /// - `key`: 配置键
    /// - `default`: 默认值
    ///
    /// # 返回值
    /// 如果配置存在返回配置值，否则返回默认值
    pub fn get_as_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
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
        self.inner
            .get_string(key)
            .map_err(|_| ConfigError::KeyNotFound {
                key: key.to_string(),
            })
    }

    /// 获取应用隔离标识(app_id)，统一入口。
    ///
    /// **按 `[deploy] mode` 分支**：
    /// - `Mono`：固定返回 `"default"`，**不读** `[app].module_code`（单体下 [app] 块不生效）
    /// - `Micro`：维持原有查找顺序（[app].module_code → 环境变量 → "default"）
    ///
    /// Micro 模式下的查找顺序：
    /// 1. 配置项 `app.module_code`
    /// 2. 环境变量 `APP_ID`
    /// 3. 环境变量 `SERVICE_REGISTRY_NAME`(nacos 场景)
    /// 4. 环境变量 `NACOS_NAMING_SERVICE_NAME`(nacos 场景)
    /// 5. 兜底 `"default"`
    ///
    /// 全项目应通过此方法获取 app_id，避免散落的 `get_string("app.id")` 调用。
    pub fn get_app_id(&self) -> String {
        // 单体模式：固定返回 "default"，不读 [app].module_code
        if DeployMode::from_config() == DeployMode::Mono {
            return "default".to_string();
        }

        // micro 模式：维持原有查找顺序
        // 1. 配置项
        if let Ok(v) = self.get_string("app.module_code")
            && !v.is_empty()
        {
            return v;
        }
        // 2-4. 环境变量
        for key in [
            "APP_ID",
            "SERVICE_REGISTRY_NAME",
            "NACOS_NAMING_SERVICE_NAME",
        ] {
            if let Ok(v) = std::env::var(key)
                && !v.is_empty()
            {
                return v;
            }
        }
        // 5. 兜底
        "default".to_string()
    }

    /// 获取整数配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回整数值，失败返回错误
    pub fn get_int(&self, key: &str) -> ConfigResult<i64> {
        self.inner
            .get_int(key)
            .map_err(|_| ConfigError::KeyNotFound {
                key: key.to_string(),
            })
    }

    /// 获取浮点数配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回浮点数值，失败返回错误
    pub fn get_float(&self, key: &str) -> ConfigResult<f64> {
        self.inner
            .get_float(key)
            .map_err(|_| ConfigError::KeyNotFound {
                key: key.to_string(),
            })
    }

    /// 获取布尔配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 成功返回布尔值，失败返回错误
    pub fn get_bool(&self, key: &str) -> ConfigResult<bool> {
        self.inner
            .get_bool(key)
            .map_err(|_| ConfigError::KeyNotFound {
                key: key.to_string(),
            })
    }

    /// 获取可选配置值
    ///
    /// 如果配置键不存在则返回 None
    ///
    /// # 类型参数
    /// - `T`: 目标类型
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 Some(值)，否则返回 None
    pub fn get_optional<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.inner.get::<T>(key).ok()
    }

    /// 检查配置键是否存在
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 true，否则返回 false
    pub fn contains(&self, key: &str) -> bool {
        self.inner.get::<String>(key).is_ok()
    }

    /// 获取所有配置键
    ///
    /// 将整个配置反序列化为 HashMap 以获取所有键
    ///
    /// # 返回值
    /// 返回所有配置键的迭代器
    pub fn keys(&self) -> impl Iterator<Item = String> {
        self.inner
            .clone()
            .try_deserialize::<HashMap<String, ConfigValue>>()
            .unwrap_or_default()
            .into_keys()
    }

    /// 获取配置项数量
    ///
    /// # 返回值
    /// 返回配置项的数量
    pub fn len(&self) -> usize {
        self.inner
            .clone()
            .try_deserialize::<HashMap<String, ConfigValue>>()
            .map(|m: HashMap<String, ConfigValue>| m.len())
            .unwrap_or(0)
    }

    /// 检查配置是否为空
    ///
    /// # 返回值
    /// 如果没有配置项返回 true，否则返回 false
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        self.inner
            .clone()
            .try_deserialize()
            .map_err(ConfigError::from)
    }

    /// 创建子配置视图
    ///
    /// 获取指定前缀下的配置子树
    ///
    /// # 参数
    /// - `prefix`: 配置键前缀
    ///
    /// # 返回值
    /// 返回只包含指定前缀配置的新配置实例
    pub fn sub_config(&self, prefix: &str) -> ConfigResult<Config> {
        let table = self
            .inner
            .get_table(prefix)
            .map_err(|_| ConfigError::KeyNotFound {
                key: prefix.to_string(),
            })?;

        let mut builder = config::Config::builder();
        for (key, value) in table {
            builder = builder.set_default(key, value).map_err(ConfigError::from)?;
        }

        let sub_inner = builder.build().map_err(ConfigError::from)?;
        Ok(Config { inner: sub_inner })
    }

    /// 获取底层 config::Config 的引用
    ///
    /// 用于需要直接操作底层配置的场景
    pub fn inner(&self) -> &config::Config {
        &self.inner
    }
}

/// 部署模式 — 启动期契约，决定数据源加载策略、app_id 取值、模块导入守卫。
///
/// - `Mono`：单体模式，加载全部数据源，app_id 固定为 `"default"`
/// - `Micro`：微服务模式，按 `[app]` 三元组精确过滤，app_id 取 `[app].module_code`
///
/// 从 `[deploy] mode` TOML 节读取，支持 `DEPLOY__MODE` 环境变量覆盖。
/// 缺省为 `Mono`（向后兼容）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeployMode {
    /// 单体模式：一个进程服务所有域/应用/模块
    #[default]
    Mono,
    /// 微服务模式：一个进程只服务 `[app]` 三元组指定的模块
    Micro,
}

impl std::str::FromStr for DeployMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mono" | "monolithic" | "single" => Ok(Self::Mono),
            "micro" | "microservice" => Ok(Self::Micro),
            other => Err(format!(
                "未知的 deploy.mode: {}（支持 mono/micro）",
                other
            )),
        }
    }
}

impl DeployMode {
    /// 从全局配置读取部署模式。
    ///
    /// 读取 `[deploy] mode` 配置项，缺省返回 `Mono`。
    pub fn from_config() -> Self {
        ConfigManager::global()
            .get_string("deploy.mode")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }
}

/// 默认配置加载器
///
/// 提供标准的配置加载流程，按照以下优先级加载（从低到高）：
/// 1. default.toml 配置文件
/// 2. 环境变量中指定的 TOML 配置文件
/// 3. .env 文件
/// 4. 系统环境变量
/// 5. 命令行参数（最高优先级）
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

        let default_path = self.config_dir.join("default.toml");
        builder = builder.add_toml_file(default_path)?;

        builder = builder.add_toml_file_from_env("CONFIG_FILE");

        if self.load_env_file {
            let env_path = self.config_dir.join(".env");
            builder = builder.add_env_file(env_path);
        }

        if self.load_system_env {
            builder = if let Some(prefix) = self.env_prefix {
                builder.add_env_with_prefix(prefix)
            } else {
                builder.add_env()
            };
        }

        if self.load_command_line {
            let args: Vec<String> = std::env::args().skip(1).collect();
            builder = builder.add_command_line(args.into_iter());
        }

        builder.build()
    }
}

/// 全局配置管理器
///
/// 提供全局配置单例，支持初始化一次后全局访问
///
/// # 示例
///
/// ```ignore
/// use cmx_utils::config::{ConfigManager, Config};
///
/// // 应用启动时初始化
/// fn init_config() -> Result<(), Box<dyn std::error::Error>> {
///     ConfigManager::initialize(|| {
///         Config::builder()
///             .add_toml_file("config/default.toml", 10)?
///             .add_env()
///             .build()
///     });
///     Ok(())
/// }
///
/// // 任意位置获取配置
/// fn get_db_config() -> Result<String, Box<dyn std::error::Error>> {
///     let config = ConfigManager::global();
///     let host = config.get_string("database.host")?;
///     Ok(host)
/// }
/// ```
pub struct ConfigManager;

static GLOBAL_CONFIG: OnceLock<RwLock<Arc<Config>>> = OnceLock::new();
static INIT_LOCK: Mutex<bool> = Mutex::new(false);

impl ConfigManager {
    /// 初始化全局配置管理器
    ///
    /// 此方法应该在应用启动时调用，只会被调用一次
    ///
    /// # 参数
    /// - `init`: 初始化函数，返回配置实例
    ///
    /// # 返回值
    /// 如果初始化成功返回 Ok(()), 如果已经初始化过返回 Err
    pub fn initialize<F, E>(init: F) -> Result<(), ConfigError>
    where
        F: FnOnce() -> Result<Config, E>,
        E: std::error::Error,
    {
        let mut lock = INIT_LOCK.lock().map_err(|_| ConfigError::BuildError {
            message: "配置管理器锁获取失败".to_string(),
        })?;

        if *lock {
            return Err(ConfigError::BuildError {
                message: "配置管理器已经初始化，不能重复初始化".to_string(),
            });
        }

        let config = init().map_err(|e| ConfigError::BuildError {
            message: format!("配置初始化失败: {}", e),
        })?;

        GLOBAL_CONFIG
            .set(RwLock::new(Arc::new(config)))
            .map_err(|_| ConfigError::BuildError {
                message: "配置管理器已经初始化，不能重复初始化".to_string(),
            })?;

        *lock = true;
        Ok(())
    }

    /// 获取全局配置实例
    ///
    /// # 返回值
    /// 返回全局配置的 Arc 引用（开销极低，仅原子计数器 +1）
    ///
    /// # Panics
    /// 如果配置管理器未初始化则会 panic。
    /// 若内部 RwLock 中毒（持有写锁时发生 panic）也会 panic。
    ///
    // TODO: 后续将 global() 改为返回 Result 以彻底消除 panic 风险。
    // 当前影响 35+ 调用点，且 RwLock 中毒在实际中极难发生（reload() 已用 map_err 处理写锁错误），
    // 故暂用 expect 带上下文替换裸 unwrap 作为过渡方案。
    pub fn global() -> Arc<Config> {
        let guard = GLOBAL_CONFIG
            .get()
            .expect("配置管理器未初始化，请先调用 ConfigManager::initialize()");
        guard
            .read()
            .expect("配置管理器 RwLock 中毒，可能因 reload 过程中 panic 导致")
            .clone()
    }

    /// 尝试获取全局配置实例
    ///
    /// 与 [`global`](Self::global) 不同，此方法在任何情况下都不会 panic：
    /// 未初始化或 RwLock 中毒时均返回 `None`。
    ///
    /// # 返回值
    /// 如果已初始化且锁正常返回 Some(Arc<Config>)，否则返回 None
    pub fn try_global() -> Option<Arc<Config>> {
        GLOBAL_CONFIG
            .get()
            .and_then(|g| g.read().ok().map(|lock| lock.clone()))
    }

    /// 检查是否已初始化
    ///
    /// # 返回值
    /// 如果已初始化返回 true，否则返回 false
    pub fn is_initialized() -> bool {
        GLOBAL_CONFIG.get().is_some()
    }

    /// 原子替换全局配置
    ///
    /// 用于配置热更新场景，将新的配置实例替换全局配置。
    /// 旧的 Arc 引用在所有持有者释放后自动回收。
    ///
    /// # 参数
    /// - `config`: 新的配置实例
    ///
    /// # 返回值
    /// 如果替换成功返回 Ok(())，如果未初始化返回 Err
    pub fn reload(config: Config) -> Result<(), ConfigError> {
        let guard = GLOBAL_CONFIG.get().ok_or(ConfigError::BuildError {
            message: "配置管理器未初始化".to_string(),
        })?;
        let mut lock = guard.write().map_err(|_| ConfigError::BuildError {
            message: "配置管理器锁获取失败".to_string(),
        })?;
        *lock = Arc::new(config);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_config_from_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(b"key = \"value\"\nnumber = 42").unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.get_string("key").unwrap(), "value");
        assert_eq!(config.get_int("number").unwrap(), 42);
    }

    #[test]
    fn test_config_nested_access() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(
            br#"
[database]
host = "localhost"
port = 5432
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.get_string("database.host").unwrap(), "localhost");
        assert_eq!(config.get_int("database.port").unwrap(), 5432);
    }

    #[test]
    fn test_config_deserialize() {
        use serde::Deserialize;

        #[derive(Deserialize, Debug)]
        struct DatabaseConfig {
            host: String,
            port: u16,
        }

        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(
            br#"
[database]
host = "localhost"
port = 5432
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();
        let db: DatabaseConfig = config.get_as("database").unwrap();
        assert_eq!(db.host, "localhost");
        assert_eq!(db.port, 5432);
    }

    #[test]
    fn test_config_get_or() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(b"existing = \"value\"").unwrap();

        let config = Config::from_file(&config_path).unwrap();

        assert_eq!(config.get_as_or("existing", "default".to_string()), "value");
        assert_eq!(
            config.get_as_or("non_existing", "default".to_string()),
            "default"
        );
    }

    #[test]
    fn test_config_merge() {
        let dir = tempdir().unwrap();
        let default_path = dir.path().join("default.toml");
        let mut file = std::fs::File::create(&default_path).unwrap();
        file.write_all(
            br#"
[app]
name = "my-app"
debug = false
"#,
        )
        .unwrap();

        let config = Config::builder()
            .add_toml_file(&default_path)
            .unwrap()
            .add_source(
                config::Environment::default()
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .unwrap();

        assert_eq!(config.get_string("app.name").unwrap(), "my-app");
        assert!(!config.get_bool("app.debug").unwrap());
    }

    #[test]
    fn test_sub_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(
            br#"
[database]
host = "localhost"
port = 5432

[cache]
host = "redis.example.com"
port = 6379
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();
        let db_config = config.sub_config("database").unwrap();
        assert_eq!(db_config.get_string("host").unwrap(), "localhost");
        assert_eq!(db_config.get_int("port").unwrap(), 5432);
    }

    #[test]
    fn test_config_type_conversions() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(
            br#"
string_val = "hello"
int_val = 42
float_val = 3.15
bool_val = true
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();

        assert_eq!(config.get_string("string_val").unwrap(), "hello");
        assert_eq!(config.get_int("int_val").unwrap(), 42);
        assert!((config.get_float("float_val").unwrap() - 3.15).abs() < 0.001);
        assert!(config.get_bool("bool_val").unwrap());
    }
}
