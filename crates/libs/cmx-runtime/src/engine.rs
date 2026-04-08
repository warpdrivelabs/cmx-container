//! Extism 运行时引擎
//!
//! 提供 WASM 模块的加载、实例化和调用功能。
//! 实现 cmx_traits::RuntimeInvoker trait。
//! 将 HostFunctionProvider 的业务逻辑函数包装成 Extism 宿主函数。
//!
//! # 插件间调用的线程安全设计
//!
//! Extism 的 `Plugin::call()` 是同步阻塞方法。当 WASM-A 通过宿主函数
//! 调用 WASM-B 时，需要在同一个调用栈内嵌套执行 `plugin.call()`。
//!
//! 为支持这种嵌套调用（同时避免死锁），每个 Plugin 实例被包装在
//! `Arc<Mutex<Plugin>>` 中。`invoke` 方法只锁定目标 Plugin 的 Mutex，
//! 不持有全局 HashMap 的锁，因此不会阻止对其他 Plugin 的并发访问。
//! 即使 WASM-A 在执行中通过宿主函数调用 WASM-B，也只会获取 WASM-B
//! 的 Mutex，不会与 WASM-A 的 Mutex 冲突。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing;

use cmx_traits::{CallerData, RuntimeInvoker, TraitError, WasmInvokeResult, HostFunctionProvider};
use extism::{CurrentPlugin, Function, Manifest, Plugin, PluginBuilder, UserData, ValType, Wasm};

use crate::error::ExtismError;

/// Extism 引擎配置
#[derive(Debug, Clone)]
pub struct ExtismEngineConfig {
    /// 是否启用 WASI，默认 true
    pub enable_wasi: bool,
    /// 内存限制（页数），默认 4096 页（256MB）
    pub memory_max: u32,
}

impl Default for ExtismEngineConfig {
    fn default() -> Self {
        Self {
            enable_wasi: true,
            memory_max: 4096,
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

/// 插件入口，包装 Plugin 实例和线程安全访问
///
/// 使用 `std::sync::Mutex` 而非 `tokio::sync::Mutex`，
/// 因为 `Plugin::call()` 是同步阻塞操作。
/// 每个 Plugin 独立加锁，不会影响其他 Plugin 的访问。
struct PluginEntry {
    /// Plugin 实例，用 Mutex 保护可变性
    plugin: Mutex<Plugin>,
}

impl PluginEntry {
    /// 创建新的插件入口
    fn new(plugin: Plugin) -> Self {
        Self {
            plugin: Mutex::new(plugin),
        }
    }

    /// 获取 Plugin 的互斥锁守卫
    fn lock(&self) -> std::sync::MutexGuard<'_, Plugin> {
        self.plugin.lock().unwrap()
    }
}

/// Extism 运行时引擎
///
/// 管理 WASM 插件的生命周期，包括加载、调用和卸载。
/// 预先缓存宿主函数以提高性能。
///
/// # 架构设计
///
/// ```text
/// ExtismEngine
///   ├── plugins: RwLock<HashMap<String, Arc<PluginEntry>>>
///   │     ├── 全局 HashMap 用 RwLock 保护（读写分离）
///   │     ├── load/unload 获取写锁
///   │     └── invoke 获取读锁后立即释放
///   │
///   └── 每个 PluginEntry 内部用 Mutex 保护
///         └── plugin.call() 只锁目标 Plugin 的 Mutex
/// ```
///
/// 这样 WASM-A 执行中通过宿主函数调用 WASM-B 时：
/// 1. 获取 HashMap 读锁 → 找到 WASM-B 的 Arc<PluginEntry> → 释放读锁
/// 2. 锁定 WASM-B 的 Mutex → 执行 plugin.call() → 释放 Mutex
/// 3. 不会与 WASM-A 持有的锁冲突
pub struct ExtismEngine {
    /// 已加载的插件实例映射 (plugin_id -> Arc<PluginEntry>)
    plugins: Arc<RwLock<HashMap<String, Arc<PluginEntry>>>>,
    /// 引擎配置
    config: ExtismEngineConfig,
    /// 预先编译的宿主函数列表
    cached_functions: Arc<RwLock<Vec<Function>>>,
}

impl ExtismEngine {
    /// 创建新的 Extism 引擎
    pub fn new(config: ExtismEngineConfig) -> Result<Self, ExtismError> {
        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            config,
            cached_functions: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// 注册宿主函数提供者
    ///
    /// 将提供者的所有函数预先编译为 `Function` 对象并缓存，
    /// 后续加载插件时直接使用缓存的函数，提高性能。
    pub async fn register_provider(&self, provider: Arc<dyn HostFunctionProvider>) -> Result<(), ExtismError> {
        let namespace = provider.namespace();
        let functions = provider.functions();
        let func_count = functions.len();

        let mut cached = self.cached_functions.write().await;

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
    pub async fn cached_function_count(&self) -> usize {
        self.cached_functions.read().await.len()
    }

    /// 宿主函数包装器
    ///
    /// 通用的宿主函数实现，从 UserData 中获取上下文并调用实际的业务逻辑。
    fn host_function_wrapper(
        plugin: &mut CurrentPlugin,
        inputs: &[extism::Val],
        outputs: &mut [extism::Val],
        user_data: UserData<HostFunctionContext>,
    ) -> Result<(), extism::Error> {
        let ctx = user_data.get()?;
        let guard = ctx.lock().unwrap();

        // 从输入参数中读取字符串
        let input: String = plugin.memory_get_val(&inputs[0]).unwrap_or_default();

        // 调用实际的业务逻辑
        let result = guard.provider.call(&guard.func_name, input);

        // 将结果写回输出
        let output_str = match result {
            Ok(output) => output,
            Err(e) => format!(r#"{{"success":false,"error":"{}"}}"#, e),
        };

        // 将结果编码到输出 Val 中
        plugin.memory_set_val(&mut outputs[0], output_str)?;

        Ok(())
    }

    /// 同步调用 Plugin（内部方法）
    ///
    /// 获取目标 Plugin 的 `Arc<PluginEntry>` 引用后立即释放 HashMap 读锁，
    /// 然后锁定目标 Plugin 的 Mutex 执行 `plugin.call()`。
    /// 由于不持有全局 HashMap 的锁，支持插件间递归调用。
    fn invoke_plugin_sync(
        plugins: &Arc<RwLock<HashMap<String, Arc<PluginEntry>>>>,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
    ) -> Result<WasmInvokeResult, TraitError> {
        let start = std::time::Instant::now();

        // 步骤1：获取 HashMap 读锁，克隆 Arc<PluginEntry>，立即释放读锁
        let entry = {
            let plugins_map = plugins.blocking_read();
            plugins_map
                .get(plugin_id)
                .cloned()
                .ok_or_else(|| TraitError::WasmNotLoaded(plugin_id.to_string()))?
        };

        // 步骤2：锁定目标 Plugin 的 Mutex，执行 plugin.call()
        // 此时不持有 HashMap 的锁，其他 Plugin 可以被并发访问
        let mut plugin = entry.lock();
        let result = plugin
            .call::<&[u8], Vec<u8>>(function_name, input)
            .map_err(|e| TraitError::WasmInvokeFailed(e.to_string()))?;

        let elapsed_us = start.elapsed().as_micros() as u64;

        Ok(WasmInvokeResult {
            output: result,
            elapsed_us,
            fuel_consumed: None,
        })
    }
}

#[async_trait]
impl RuntimeInvoker for ExtismEngine {
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        _caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError> {
        Self::invoke_plugin_sync(&self.plugins, plugin_id, function_name, input)
    }

    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(plugin_id) {
                tracing::warn!("插件 {} 的 WASM 模块已加载，跳过", plugin_id);
                return Ok(());
            }
        }

        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "读取 WASM 文件 {:?} 失败: {}",
                wasm_path, e
            )))?;

        let wasm = Wasm::data(wasm_bytes);
        let manifest = Manifest::new([wasm])
            .with_memory_max(self.config.memory_max);

        let cached_functions = self.cached_functions.read().await;
        let function_count = cached_functions.len();

        let builder = PluginBuilder::new(manifest)
            .with_wasi(self.config.enable_wasi)
            .with_functions(cached_functions.clone());

        let plugin = builder.build()
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "创建 Extism 插件失败: {}",
                e
            )))?;

        tracing::info!(
            "插件 {} WASM 模块加载成功，已注册 {} 个宿主函数",
            plugin_id,
            function_count
        );

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.to_string(), Arc::new(PluginEntry::new(plugin)));

        Ok(())
    }

    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let mut plugins = self.plugins.write().await;
        if plugins.remove(plugin_id).is_some() {
            tracing::info!("插件 {} WASM 模块已卸载", plugin_id);
        } else {
            tracing::warn!("插件 {} WASM 模块未加载，无法卸载", plugin_id);
        }
        Ok(())
    }

    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(plugin_id)
    }
}
