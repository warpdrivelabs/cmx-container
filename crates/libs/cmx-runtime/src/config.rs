//! Extism 引擎配置
//!
//! 定义引擎运行时参数，并提供从 ConfigManager（dev.toml）读取
//! Fuel 限制等动态配置的能力。
//!
//! # 配置项（dev.toml `[runtime]` 节）
//!
//! ```toml
//! [runtime]
//! # Fuel 限制（0 表示不限制，单位：Wasm 指令数）
//! fuel_limit = 0
//! ```

use std::time::Duration;

use cmx_traits::DEFAULT_TIMEOUT;
use cmx_utils::ConfigManager;

use crate::error::ExtismError;

/// Extism 引擎配置
///
/// 控制 WASM 运行时的核心参数。部分参数（Fuel 限制）
/// 会从 ConfigManager（dev.toml）中读取并覆盖默认值。
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

/// 从 ConfigManager 读取 Fuel 限制配置
///
/// 读取 `runtime.fuel_limit` 配置项（单位：Wasm 指令数）。
/// 值为 0 表示不限制，正值启用 Fuel 限制。
///
/// # 返回值
///
/// - `Some(u64)`: 启用 Fuel 限制，值为最大指令数
/// - `None`: 不限制（配置为 0 或 ConfigManager 未初始化）
pub fn read_fuel_limit() -> Option<u64> {
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
/// 以传入的基础配置为底，覆盖 ConfigManager 中的动态配置（Fuel），
/// 返回最终生效的配置。
///
/// # 参数
///
/// - `base`: 基础配置（用户提供或默认值）
///
/// # 返回值
///
/// 成功返回最终生效的 `ExtismEngineConfig`
pub fn load_runtime_config(mut base: ExtismEngineConfig) -> Result<ExtismEngineConfig, ExtismError> {
    base.fuel_limit = read_fuel_limit();

    tracing::info!(
        "Extism 引擎配置加载完成: fuel_limit={:?}, pool_max={}",
        base.fuel_limit,
        base.pool_max_instances
    );

    Ok(base)
}
