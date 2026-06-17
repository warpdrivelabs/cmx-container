//! Extism 引擎配置
//!
//! 定义引擎运行时参数，并提供从 ConfigManager（dev.toml）读取
//! 各项动态配置的能力。
//!
//! # 配置项（dev.toml `[runtime]` 节）
//!
//! ```toml
//! [runtime]
//! # 内存限制（页数），每页 64KB，默认 4096 页（256MB）
//! memory_max = 4096
//! # 单次调用超时时间（秒），默认 30
//! timeout = 30
//! # 实例池最大实例数，默认使用 CPU 核心数
//! pool_max_instances = 8
//! # Fuel 限制（0 表示不限制，单位：Wasm 指令数）
//! fuel_limit = 0
//! ```

use std::time::Duration;

use cmx_traits::runtime::DEFAULT_TIMEOUT;
use cmx_utils::ConfigManager;

use crate::error::ExtismError;

/// Extism 引擎配置
///
/// 控制 WASM 运行时的核心参数。所有参数均支持从 ConfigManager（dev.toml）覆盖。
#[derive(Debug, Clone)]
pub struct ExtismEngineConfig {
    /// 是否启用 WASI（WebAssembly System Interface），默认 true
    ///
    /// 启用后插件可使用标准 I/O、文件系统等系统接口
    pub enable_wasi: bool,

    /// 内存限制（页数），默认 4096 页（每页 64KB，共 256MB）
    ///
    /// 通过 `Manifest::with_memory_max()` 设置，限制插件线性内存的最大大小
    pub memory_max: u32,

    /// 默认超时时间，默认 30 秒
    ///
    /// 通过 `Manifest::with_timeout()` 设置，单次 plugin.call() 的最大执行时间
    pub timeout: Duration,

    /// 每个 Plugin 池的最大实例数，默认使用 CPU 核心数
    ///
    /// 通过 `PoolBuilder::with_max_instances()` 设置，
    /// 控制每个插件的并发实例数量上限
    pub pool_max_instances: usize,

    /// Fuel 限制（Wasm 指令执行步数），None 表示不限制
    ///
    /// 通过 `PluginBuilder::with_fuel_limit()` 设置，
    /// 限制单次调用中可执行的 WebAssembly 指令数量，
    /// 防止死循环和恶意代码消耗过多 CPU
    pub fuel_limit: Option<u64>,
}

impl Default for ExtismEngineConfig {
    fn default() -> Self {
        Self {
            enable_wasi: true,
            memory_max: 4096,
            timeout: DEFAULT_TIMEOUT,
            pool_max_instances: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(8),
            fuel_limit: None,
        }
    }
}

/// 从 ConfigManager 读取 memory_max 配置
///
/// 读取 `runtime.memory_max` 配置项（页数）。
/// 值必须 > 0，否则使用默认值。
///
/// # 返回值
///
/// - `Some(u32)`: 读取成功，返回配置值
/// - `None`: ConfigManager 未初始化或配置无效，使用默认值
fn read_memory_max() -> Option<u32> {
    let s = ConfigManager::try_global()?.get_string("runtime.memory_max").ok()?;

    match s.parse::<u32>() {
        Ok(n) if n > 0 => Some(n),
        Ok(n) => {
            tracing::warn!("runtime.memory_max={} 无效（必须 > 0），使用默认值", n);
            None
        }
        Err(_) => {
            tracing::warn!("runtime.memory_max='{}' 解析失败，使用默认值", s);
            None
        }
    }
}

/// 从 ConfigManager 读取 timeout 配置
///
/// 读取 `runtime.timeout` 配置项（秒）。
/// 值必须 > 0，否则使用默认值。
///
/// # 返回值
///
/// - `Some(Duration)`: 读取成功，返回 Duration
/// - `None`: ConfigManager 未初始化或配置无效，使用默认值
fn read_timeout() -> Option<Duration> {
    let s = ConfigManager::try_global()?.get_string("runtime.timeout").ok()?;

    match s.parse::<u64>() {
        Ok(n) if n > 0 => {
            let d = Duration::from_secs(n);
            tracing::info!("Timeout 配置: {} 秒", n);
            Some(d)
        }
        Ok(n) => {
            tracing::warn!("runtime.timeout={} 无效（必须 > 0），使用默认值", n);
            None
        }
        Err(_) => {
            tracing::warn!("runtime.timeout='{}' 解析失败，使用默认值", s);
            None
        }
    }
}

/// 从 ConfigManager 读取 pool_max_instances 配置
///
/// 读取 `runtime.pool_max_instances` 配置项。
/// 值必须 > 0，否则使用默认值。
///
/// # 返回值
///
/// - `Some(usize)`: 读取成功，返回配置值
/// - `None`: ConfigManager 未初始化或配置无效，使用默认值
fn read_pool_max_instances() -> Option<usize> {
    let s = ConfigManager::try_global()?
        .get_string("runtime.pool_max_instances")
        .ok()?;

    match s.parse::<usize>() {
        Ok(n) if n > 0 => {
            tracing::info!("Pool 最大实例数配置: {}", n);
            Some(n)
        }
        Ok(n) => {
            tracing::warn!(
                "runtime.pool_max_instances={} 无效（必须 > 0），使用默认值",
                n
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                "runtime.pool_max_instances='{}' 解析失败，使用默认值",
                s
            );
            None
        }
    }
}

/// 从 ConfigManager 读取 Fuel 限制配置
///
/// 读取 `runtime.fuel_limit` 配置项（单位：Wasm 指令数）。
/// 值为 0 表示不限制，正值启用 Fuel 限制。
///
/// # 返回值
///
/// - `Some(u64)`: 启用 Fuel 限制，值为最大指令数
/// - `None`: 不限制（配置为 0 或 ConfigManager 未初始化）
fn read_fuel_limit() -> Option<u64> {
    let fuel_str = ConfigManager::try_global()?
        .get_string("runtime.fuel_limit")
        .ok()?;

    match fuel_str.parse::<u64>() {
        Ok(0) => None,
        Ok(n) => {
            tracing::info!("Fuel 限制已启用: {} 条指令", n);
            Some(n)
        }
        Err(_) => {
            tracing::warn!(
                "Fuel 限制配置无效: '{}', 使用默认值(不限制)",
                fuel_str
            );
            None
        }
    }
}

/// 从 ConfigManager 加载运行时配置并构建 ExtismEngineConfig
///
/// 以传入的基础配置为底，依次从 ConfigManager 覆盖各项动态配置：
/// - `runtime.memory_max`
/// - `runtime.timeout`
/// - `runtime.pool_max_instances`
/// - `runtime.fuel_limit`
///
/// # 参数
///
/// - `base`: 基础配置（用户提供或默认值）
///
/// # 返回值
///
/// 成功返回最终生效的 `ExtismEngineConfig`
pub fn load_runtime_config(mut base: ExtismEngineConfig) -> Result<ExtismEngineConfig, ExtismError> {
    if let Some(v) = read_memory_max() {
        base.memory_max = v;
    }
    if let Some(v) = read_timeout() {
        base.timeout = v;
    }
    if let Some(v) = read_pool_max_instances() {
        base.pool_max_instances = v;
    }
    base.fuel_limit = read_fuel_limit();

    tracing::info!(
        "Extism 引擎配置加载完成: memory_max={} 页, timeout={:?}, pool_max={}, fuel_limit={:?}",
        base.memory_max,
        base.timeout,
        base.pool_max_instances,
        base.fuel_limit,
    );

    Ok(base)
}
