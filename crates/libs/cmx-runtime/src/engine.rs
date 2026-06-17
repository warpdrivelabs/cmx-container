//! Extism 运行时引擎
//!
//! 提供 WASM 模块的加载、实例化和调用功能。
//! 实现 cmx_traits::runtime::RuntimeInvoker trait。
//!
//! # 模块结构
//!
//! ```text
//! engine.rs        ← 本文件：核心引擎结构体 + RuntimeInvoker 实现
//! config.rs        ← 引擎配置 + Fuel 初始化
//! metrics.rs       ← 运行时指标收集（EngineMetrics）
//! host_function.rs ← 宿主函数桥接（HostFunctionContext + 包装器）
//! ```
//!
//! # 高并发架构
//!
//! ```text
//! ExtismEngine
//!   ├── plugin_pools: RwLock<HashMap<String, Pool>>
//!   │     └── Pool (每个 plugin_id 一个)
//!   │           ├── 工厂函数 (PluginBuilder 快速创建实例)
//!   │           └── 内置 Condvar 等待机制
//!   └── metrics: Arc<EngineMetrics>
//! ```
//!
//! # 并发模型：spawn_blocking
//!
//! ```text
//! tokio worker
//!   → spawn_blocking {  (任务迁移到阻塞线程池，worker 被释放)
//!       pool.with_plugin { plugin.call() }
//!     }
//!   → .await JoinHandle (获取结果)
//! ```
//!
//! # 多层防护机制
//!
//! 1. **调用深度限制** — 防止无限递归（默认最大 8 层）
//! 2. **循环检测** — 检测同一插件的递归调用（A→B→A 或 A→A）
//! 3. **Extism 原生超时** — 单次 plugin.call() 超时自动中断
//! 4. **Fuel 限制** — 限制 Wasm 指令执行数，防止死循环和恶意代码

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use cmx_traits::error::TraitError;
use cmx_traits::runtime::{
    HostFunctionProvider, InvokeContext, InvokeOptions, RuntimeInvoker, WasmInvokeResult,
};
use extism::{Manifest, PluginBuilder, Pool, PoolBuilder, Wasm};

use crate::config::{self, ExtismEngineConfig};
use crate::error::ExtismError;
use crate::host_function;
use crate::metrics::EngineMetrics;

/// Extism 运行时引擎
///
/// 管理 WASM 插件的生命周期，包括加载、调用和卸载。
/// 使用 Extism 内置的 Pool 支持高并发调用。
///
/// # 线程安全
///
/// - `plugin_pools` 使用 `std::sync::RwLock` 保护，支持并发读取
/// - `cached_functions` 使用 `std::sync::RwLock` 保护
/// - `metrics` 使用原子操作，无需加锁
pub struct ExtismEngine {
    /// Plugin 池映射 (plugin_id -> Pool)
    ///
    /// 每个插件 ID 对应一个独立的实例池，通过 `Pool::with_plugin()` 自动管理实例的获取和归还
    plugin_pools: RwLock<HashMap<String, Pool>>,

    /// 引擎配置
    config: ExtismEngineConfig,

    /// 预先编译的宿主函数列表
    ///
    /// 通过 `host_function::register_provider_functions()` 注册，
    /// 在 `load_module()` 时通过 `PluginBuilder::with_functions()` 注入到每个插件实例中
    cached_functions: RwLock<Vec<extism::Function>>,

    /// 运行时监控指标（无锁原子操作）
    metrics: Arc<EngineMetrics>,
}

impl ExtismEngine {
    /// 创建新的 Extism 引擎
    ///
    /// 从 ConfigManager（dev.toml）读取运行时配置并初始化：
    /// - `runtime.fuel_limit`: Fuel 限制
    pub fn new(config: ExtismEngineConfig) -> Result<Self, ExtismError> {
        let config = config::load_runtime_config(config)?;

        Ok(Self {
            plugin_pools: RwLock::new(HashMap::new()),
            config,
            cached_functions: RwLock::new(Vec::new()),
            metrics: Arc::new(EngineMetrics::new()),
        })
    }

    /// 注册宿主函数提供者
    ///
    /// 委托给 `host_function::register_provider_functions()` 处理，
    /// 将提供者的业务逻辑函数包装为 Extism 宿主函数。
    pub fn register_provider(
        &self,
        provider: Arc<dyn HostFunctionProvider>,
    ) -> Result<(), ExtismError> {
        let mut cached = self.cached_functions.write().unwrap();
        host_function::register_provider_functions(provider, &mut cached)?;
        Ok(())
    }

    /// 获取已缓存的宿主函数数量
    pub fn cached_function_count(&self) -> usize {
        self.cached_functions.read().unwrap().len()
    }

    /// 获取运行时监控指标的引用
    pub fn get_metrics(&self) -> Arc<EngineMetrics> {
        self.metrics.clone()
    }

    /// 同步调用 Plugin（在 spawn_blocking 线程中执行）
    ///
    /// # 调用流程
    ///
    /// 1. 通过 `InvokeContext::enter()` 进行深度限制和循环检测
    /// 2. 通过 `Pool::with_plugin()` 从实例池获取一个可用实例
    /// 3. 通过 `plugin.call()` 执行 WASM 函数
    /// 4. 记录指标并返回结果
    fn invoke_plugin_sync(
        pool: &Pool,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
        metrics: &EngineMetrics,
    ) -> Result<WasmInvokeResult, TraitError> {
        let start = std::time::Instant::now();

        // 第1层 + 第2层: 深度限制 + 循环检测
        let _guard = InvokeContext::enter(plugin_id, function_name, options.max_depth).map_err(
            |e| {
                tracing::warn!("{}", e);
                metrics.record_failure(start.elapsed().as_micros() as u64);
                TraitError::WasmInvokeFailed(e.to_string())
            },
        )?;

        tracing::debug!(
            "WASM 调用开始: plugin={}, function={}, depth={}",
            plugin_id,
            function_name,
            _guard.depth()
        );

        // Pool::with_plugin: 从实例池中获取一个可用插件实例
        // - 第一个参数是超时时间，如果在指定时间内没有可用实例则返回 Ok(None)
        // - 闭包结束后实例自动归还到池中（RAII）
        let result = pool
            .with_plugin(options.timeout, |plugin| {
                // plugin.call: 调用 WASM 模块中导出的函数
                // - 泛型: Input=&[u8], Output=Vec<u8>
                // - 输入数据写入 Extism 线性内存供插件读取，返回值从内存中读取
                plugin.call::<&[u8], Vec<u8>>(function_name, input)
            })
            .map_err(|e| {
                metrics.record_failure(start.elapsed().as_micros() as u64);
                TraitError::WasmInvokeFailed(e.to_string())
            })?;

        match result {
            Some(output) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                metrics.record_success(elapsed_us);
                tracing::debug!(
                    "WASM 调用完成: plugin={}, function={}, elapsed={}us, depth={}",
                    plugin_id,
                    function_name,
                    elapsed_us,
                    _guard.depth()
                );
                Ok(WasmInvokeResult {
                    output,
                    elapsed_us,
                    fuel_consumed: None,
                })
            }
            // with_plugin 返回 None 表示在超时时间内未能获取到可用实例
            None => {
                metrics.record_timeout(start.elapsed().as_micros() as u64);
                tracing::error!(
                    "插件 {} WASM 调用超时: function={}, timeout={:?}",
                    plugin_id,
                    function_name,
                    options.timeout
                );
                Err(TraitError::WasmInvokeFailed(format!(
                    "获取插件实例超时: {:?}",
                    options.timeout
                )))
            }
        }
    }

    /// 获取指定插件的池实例数
    ///
    /// 返回池中当前存活的实例数（含已借出和可用的实例）。
    pub fn get_pool_count(&self, plugin_id: &str) -> Option<usize> {
        let pools = self.plugin_pools.read().unwrap();
        pools.get(plugin_id).map(|p| p.count())
    }
}

#[async_trait]
impl RuntimeInvoker for ExtismEngine {
    /// 带选项的异步调用
    ///
    /// 通过 `tokio::task::spawn_blocking` 将同步阻塞的 plugin.call()
    /// 迁移到专用阻塞线程池执行，避免占用 tokio worker。
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError> {
        // 获取 Pool 的 Arc 引用后立即释放读锁
        let pool = {
            let pools = self.plugin_pools.read().unwrap();
            pools
                .get(plugin_id)
                .cloned()
                .ok_or_else(|| TraitError::WasmNotLoaded(plugin_id.to_string()))?
        };

        let plugin_id = plugin_id.to_string();
        let function_name = function_name.to_string();
        let input = input.to_vec();
        let options = options.clone();
        let metrics = self.metrics.clone();

        tokio::task::spawn_blocking(move || {
            Self::invoke_plugin_sync(
                &pool, &plugin_id, &function_name, &input, &options, &metrics,
            )
        })
        .await
        .map_err(|e| TraitError::WasmInvokeFailed(format!("spawn_blocking 任务失败: {}", e)))?
    }

    /// 加载 WASM 模块
    ///
    /// 将 WASM 文件编译并创建插件实例池。使用双重检查锁定模式防止并发重复加载。
    ///
    /// # 工厂函数构建
    ///
    /// 工厂函数通过 `PluginBuilder` 链式配置：
    /// 1. `with_wasi()` — 启用 WASI
    /// 2. `with_functions()` — 注入宿主函数
    /// 3. `with_fuel_limit()` — 设置 Fuel 限制（可选）
    /// 4. `build()` — 编译并创建实例
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
        // 第一次检查：快速路径，使用读锁
        {
            let pools = self.plugin_pools.read().unwrap();
            if pools.contains_key(plugin_id) {
                tracing::debug!("插件 {} 的 WASM 模块已加载，跳过", plugin_id);
                return Ok(());
            }
        }

        // 读取 WASM 字节（在锁外执行，避免长时间持锁）
        let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
            TraitError::WasmLoadFailed(format!("读取 WASM 文件 {:?} 失败: {}", wasm_path, e))
        })?;

        // Wasm::data(): 从字节数据创建 WASM 模块描述
        let wasm = Wasm::data(wasm_bytes);

        // Manifest: Extism 插件清单
        let manifest = Manifest::new([wasm])
            .with_timeout(self.config.timeout)
            .with_memory_max(self.config.memory_max);

        let cached_functions = self.cached_functions.read().unwrap();
        let function_count = cached_functions.len();

        // 创建工厂函数
        let functions = cached_functions.clone();
        let enable_wasi = self.config.enable_wasi;
        let fuel_limit = self.config.fuel_limit;

        let factory = move || {
            let mut builder = PluginBuilder::new(manifest.clone())
                .with_wasi(enable_wasi)
                .with_functions(functions.clone());

            // with_fuel_limit(): 设置 Fuel（指令执行数）限制
            if let Some(fuel) = fuel_limit {
                builder = builder.with_fuel_limit(fuel);
            }

            builder.build()
        };

        // PoolBuilder: 创建插件实例池，内置 Condvar 等待机制
        let pool = PoolBuilder::new()
            .with_max_instances(self.config.pool_max_instances)
            .build(factory);

        // 第二次检查 + 插入：使用写锁进行原子操作
        {
            let mut pools = self.plugin_pools.write().unwrap();
            if pools.contains_key(plugin_id) {
                tracing::debug!(
                    "插件 {} 的 WASM 模块已被其他线程加载，跳过",
                    plugin_id
                );
                return Ok(());
            }
            pools.insert(plugin_id.to_string(), pool);
        }

        tracing::info!(
            "插件 {} WASM 模块加载成功，超时={:?}，实例池 max={}，已注册 {} 个宿主函数，fuel={:?}",
            plugin_id,
            self.config.timeout,
            self.config.pool_max_instances,
            function_count,
            self.config.fuel_limit,
        );

        Ok(())
    }

    /// 卸载 WASM 模块
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let mut pools = self.plugin_pools.write().unwrap();
        if pools.remove(plugin_id).is_some() {
            tracing::info!("插件 {} WASM 模块已卸载", plugin_id);
        } else {
            tracing::warn!("插件 {} WASM 模块未加载，无法卸载", plugin_id);
        }
        Ok(())
    }

    /// 检查 WASM 模块是否已加载
    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let pools = self.plugin_pools.read().unwrap();
        pools.contains_key(plugin_id)
    }
}
