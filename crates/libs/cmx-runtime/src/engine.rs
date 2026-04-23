//! Extism 运行时引擎
//!
//! 提供 WASM 模块的加载、实例化和调用功能。
//! 实现 cmx_traits::RuntimeInvoker trait。
//! 将 HostFunctionProvider 的业务逻辑函数包装成 Extism 宿主函数。
//!
//! # 高并发架构设计
//!
//! 使用 Extism 内置的 `CompiledPlugin` + `Pool` 实现高性能实例池：
//!
//! ```text
//! ExtismEngine
//!   └── plugin_pools: RwLock<HashMap<String, Pool>>
//!         │
//!         └── Pool (每个 plugin_id 一个)
//!               ├── CompiledPlugin (预编译 WASM，避免重复编译)
//!               ├── 工厂函数 (从 CompiledPlugin 快速创建实例)
//!               └── 内置 Condvar 等待机制
//! ```
//!
//! # 并发模型：spawn_blocking
//!
//! 调用链使用 `tokio::task::spawn_blocking` 将同步阻塞的 plugin.call()
//! 迁移到专用阻塞线程池执行，避免占用 tokio worker：
//!
//! ```text
//! tokio worker
//!   → spawn_blocking {  (任务迁移到阻塞线程池，worker 被释放)
//!       pool.with_plugin { plugin.call() }
//!         → 宿主函数回调
//!           → Handle::current().block_on(async { ... })
//!             → tokio worker 已空闲 → 正常驱动 future
//!     }
//!   → .await JoinHandle (获取结果)
//! ```
//!
//! # 性能优化要点
//!
//! 1. **CompiledPlugin** - 预编译 WASM 模块，避免每次创建实例时重新编译
//! 2. **extism::Pool** - 内置实例池，使用 Condvar 等待而非轮询
//! 3. **Pool::with_plugin** - 自动获取和归还实例，RAII 模式
//!
//! # 多层防护机制
//!
//! 1. **调用深度限制** — 防止无限递归（默认最大 8 层）
//! 2. **循环检测** — 检测同一插件的递归调用（A→B→A 或 A→A）
//! 3. **Extism 原生超时** — 单次 plugin.call() 超时自动中断

use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use async_trait::async_trait;
use tracing;

use cmx_traits::{
    HostFunctionProvider, InvokeContext, InvokeOptions,
    RuntimeInvoker, TraitError, WasmInvokeResult, DEFAULT_TIMEOUT,
};
use extism::{CurrentPlugin, Function, Manifest, PluginBuilder, Pool, PoolBuilder, UserData, ValType, Wasm};

use crate::error::ExtismError;

/// Extism 引擎配置
#[derive(Debug, Clone)]
pub struct ExtismEngineConfig {
    /// 是否启用 WASI，默认 true
    pub enable_wasi: bool,
    /// 内存限制（页数），默认 4096 页（256MB）
    pub memory_max: u32,
    /// 默认超时时间，默认 30 秒
    pub timeout: Duration,
    /// 每个 Plugin 池的最大实例数，默认使用 CPU 核心数
    pub pool_max_instances: usize,
}

impl Default for ExtismEngineConfig {
    fn default() -> Self {
        Self {
            enable_wasi: true,
            memory_max: 4096,
            timeout: DEFAULT_TIMEOUT,
            pool_max_instances: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(4),
            // pool_max_instances: 100,
        }
    }
}

/// 宿主函数调用上下文
///
/// 包含提供者和函数名，用于在宿主函数中调用正确的业务逻辑
struct HostFunctionContext {
    /// 宿主函数提供者
    provider: Arc<dyn HostFunctionProvider>,
    /// 函数名称
    func_name: String,
}

impl HostFunctionContext {
    /// 创建新的宿主函数上下文
    fn new(provider: Arc<dyn HostFunctionProvider>, func_name: String) -> Self {
        Self { provider, func_name }
    }
}

/// Extism 运行时引擎
///
/// 管理 WASM 插件的生命周期，包括加载、调用和卸载。
/// 使用 Extism 内置的 Pool 和 CompiledPlugin 支持高并发调用。
pub struct ExtismEngine {
    /// Plugin 池映射 (plugin_id -> Pool)
    plugin_pools: RwLock<HashMap<String, Pool>>,
    /// 引擎配置
    config: ExtismEngineConfig,
    /// 预先编译的宿主函数列表
    cached_functions: RwLock<Vec<Function>>,
}

impl ExtismEngine {
    /// 创建新的 Extism 引擎
    pub fn new(config: ExtismEngineConfig) -> Result<Self, ExtismError> {
        Ok(Self {
            plugin_pools: RwLock::new(HashMap::new()),
            config,
            cached_functions: RwLock::new(Vec::new()),
        })
    }

    /// 注册宿主函数提供者
    pub fn register_provider(&self, provider: Arc<dyn HostFunctionProvider>) -> Result<(), ExtismError> {
        let namespace = provider.namespace();
        let functions = provider.functions();
        let func_count = functions.len();

        let mut cached = self.cached_functions.write().unwrap();

        for func_def in functions {
            let ctx = HostFunctionContext::new(provider.clone(), func_def.name.to_string());

            let func = Function::new(
                func_def.name,
                [ValType::I64],
                [ValType::I64],
                UserData::new(ctx),
                Self::host_function_wrapper,
            )
            .with_namespace(namespace);

            cached.push(func);
        }

        tracing::info!(
            "已注册宿主函数提供者 [{}]，提供 {} 个函数，当前缓存 {} 个函数",
            namespace,
            func_count,
            cached.len()
        );

        Ok(())
    }

    /// 获取已缓存的函数数量
    pub fn cached_function_count(&self) -> usize {
        self.cached_functions.read().unwrap().len()
    }

    /// 宿主函数包装器
    fn host_function_wrapper(
        plugin: &mut CurrentPlugin,
        inputs: &[extism::Val],
        outputs: &mut [extism::Val],
        user_data: UserData<HostFunctionContext>,
    ) -> Result<(), extism::Error> {
        let ctx = user_data.get()?;
        let guard = ctx.lock().unwrap();

        let input: String = plugin.memory_get_val(&inputs[0]).unwrap_or_default();

        let result = guard.provider.call(&guard.func_name, input);

        let output_str = match result {
            Ok(output) => output,
            Err(e) => format!(r#"{{"success":false,"error":"{}"}}"#, e),
        };

        plugin.memory_set_val(&mut outputs[0], output_str)?;

        Ok(())
    }

    /// 同步调用 Plugin（在 spawn_blocking 线程中执行）
    ///
    /// 由 `invoke_with_options` 通过 `spawn_blocking` 调用，
    /// 此时已脱离 tokio worker，可安全执行阻塞操作。
    fn invoke_plugin_sync(
        pool: &Pool,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError> {
        let start = std::time::Instant::now();

        // 第1层 + 第2层: 深度限制 + 循环检测
        let _guard = InvokeContext::enter(plugin_id, function_name, options.max_depth)
            .map_err(|e| {
                tracing::warn!("{}", e);
                TraitError::WasmInvokeFailed(e.to_string())
            })?;

        tracing::debug!(
            "WASM 调用开始: plugin={}, function={}, depth={}",
            plugin_id,
            function_name,
            _guard.depth()
        );
        unsafe {
            env::set_var("EXTISM_DEBUG", "1");
        }
        // 使用 Pool::with_plugin 自动获取和归还实例
        let result = pool
            .with_plugin(options.timeout, |plugin| {

                plugin.call::<&[u8], Vec<u8>>(function_name, input)
            })
            .map_err(|e| TraitError::WasmInvokeFailed(e.to_string()))?;
        unsafe {
            env::remove_var("EXTISM_DEBUG");
        }

        match result {
            Some(output) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
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
            None => {
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
    pub fn get_pool_count(&self, plugin_id: &str) -> Option<usize> {
        let pools = self.plugin_pools.read().unwrap();
        pools.get(plugin_id).map(|p| p.count())
    }
}

#[async_trait]
impl RuntimeInvoker for ExtismEngine {
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError> {
        // 获取 Pool 引用后立即释放锁
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

        // 使用 spawn_blocking 将同步阻塞的 plugin.call() 迁移到专用阻塞线程池，
        // tokio worker 通过 .await 等待结果时会被释放，可处理其他任务。
        // 宿主函数中的 block_on 可正常使用空闲的 tokio worker 驱动 async future。
        tokio::task::spawn_blocking(move || {
            Self::invoke_plugin_sync(&pool, &plugin_id, &function_name, &input, &options)
        })
        .await
        .map_err(|e| TraitError::WasmInvokeFailed(format!("spawn_blocking 任务失败: {}", e)))?
    }

    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
        // 第一次检查：快速路径，使用读锁
        {
            let pools = self.plugin_pools.read().unwrap();
            if pools.contains_key(plugin_id) {
                tracing::debug!("插件 {} 的 WASM 模块已加载，跳过", plugin_id);
                return Ok(());
            }
        }

        // 读取并编译 WASM（在锁外执行，避免长时间持锁）
        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "读取 WASM 文件 {:?} 失败: {}",
                wasm_path, e
            )))?;

        let wasm = Wasm::data(wasm_bytes);
        let manifest = Manifest::new([wasm])
            .with_timeout(self.config.timeout)
            .with_memory_max(self.config.memory_max);

        let cached_functions = self.cached_functions.read().unwrap();
        let function_count = cached_functions.len();

        // 创建工厂函数
        let functions = cached_functions.clone();
        let enable_wasi = self.config.enable_wasi;
        let factory = move || {
            PluginBuilder::new(manifest.clone())
                .with_wasi(enable_wasi)
                .with_functions(functions.clone())
                .build()
        };

        // 创建 Pool（在锁外执行）
        let pool = PoolBuilder::new()
            .with_max_instances(self.config.pool_max_instances)
            .build(factory);

        // 第二次检查 + 插入：使用写锁进行原子操作
        {
            let mut pools = self.plugin_pools.write().unwrap();
            // 双重检查：防止竞态条件下重复加载
            if pools.contains_key(plugin_id) {
                tracing::debug!("插件 {} 的 WASM 模块已被其他线程加载，跳过", plugin_id);
                return Ok(());
            }
            pools.insert(plugin_id.to_string(), pool);
        }

        tracing::info!(
            "插件 {} WASM 模块加载成功，超时={:?}，实例池 max={}，已注册 {} 个宿主函数",
            plugin_id,
            self.config.timeout,
            self.config.pool_max_instances,
            function_count
        );

        Ok(())
    }

    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let mut pools = self.plugin_pools.write().unwrap();
        if pools.remove(plugin_id).is_some() {
            tracing::info!("插件 {} WASM 模块已卸载", plugin_id);
        } else {
            tracing::warn!("插件 {} WASM 模块未加载，无法卸载", plugin_id);
        }
        Ok(())
    }

    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let pools = self.plugin_pools.read().unwrap();
        pools.contains_key(plugin_id)
    }
}
