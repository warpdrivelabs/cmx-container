//! Extism 运行时引擎
//!
//! 提供 WASM 模块的加载、实例化和调用功能。
//! 实现 cmx_traits::RuntimeInvoker trait。
//! 将 HostFunctionProvider 的业务逻辑函数包装成 Extism 宿主函数。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing;

use cmx_traits::{CallerData, RuntimeInvoker, TraitError, WasmInvokeResult, HostFunctionProvider};
use extism::{Manifest, Plugin, PluginBuilder, Wasm, UserData, ValType, host_fn};

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
/// 包含提供者和函数名，用于在 host_fn! 宏中调用正确的函数
struct HostFunctionContext {
    provider: Arc<dyn HostFunctionProvider>,
    func_name: String,
}

impl HostFunctionContext {
    fn new(provider: Arc<dyn HostFunctionProvider>, func_name: String) -> Self {
        Self { provider, func_name }
    }
}

/// Extism 运行时引擎
pub struct ExtismEngine {
    /// 已加载的插件实例映射 (plugin_id -> Plugin)
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    /// 引擎配置
    config: ExtismEngineConfig,
    /// 宿主函数提供者列表
    providers: Arc<RwLock<Vec<Arc<dyn HostFunctionProvider>>>>,
}

impl ExtismEngine {
    /// 创建新的 Extism 引擎
    pub fn new(config: ExtismEngineConfig) -> Result<Self, ExtismError> {
        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            config,
            providers: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// 注册宿主函数提供者
    pub async fn register_provider(&self, provider: Arc<dyn HostFunctionProvider>) -> Result<(), ExtismError> {
        let mut providers = self.providers.write().await;
        tracing::info!(
            "已注册宿主函数提供者 [{}]，提供 {} 个函数",
            provider.namespace(),
            provider.provided_functions().len()
        );
        providers.push(provider);
        Ok(())
    }

    /// 获取已注册的提供者数量
    pub async fn provider_count(&self) -> usize {
        self.providers.read().await.len()
    }

    /// 将 HostFunctionProvider 的函数注册到 PluginBuilder
    fn register_provider_functions(
        builder: &mut PluginBuilder,
        provider: &Arc<dyn HostFunctionProvider>,
    ) -> Result<(), ExtismError> {
        let functions = provider.functions();
        let namespace = provider.namespace();

        for func_def in functions {
            let ctx = HostFunctionContext::new(provider.clone(), func_def.name.to_string());

            // 定义通用的宿主函数包装器
            host_fn!(host_function_wrapper(user_data: HostFunctionContext; input: String) -> String {
                let ctx = user_data.get()?;
                let guard = ctx.lock().unwrap();
                let result = guard.provider.call(&guard.func_name, input);
                match result {
                    Ok(output) => Ok(output),
                    Err(e) => Ok(format!(r#"{{"success":false,"error":"{}"}}"#, e)),
                }
            });

            // 使用 std::mem::replace 模式
            let temp_manifest = Manifest::new([Wasm::data(vec![])]);
            let temp_builder = PluginBuilder::new(temp_manifest);
            let old_builder = std::mem::replace(builder, temp_builder);

            // 使用 with_function_in_namespace 正确设置命名空间
            // 这样 Extism 会将函数注册为 "namespace:function_name" 格式
            let new_builder = old_builder.with_function_in_namespace(
                namespace,
                func_def.name,
                [ValType::I64],
                [ValType::I64],
                UserData::new(ctx),
                host_function_wrapper,
            );

            *builder = new_builder;
        }

        Ok(())
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
        let start = std::time::Instant::now();

        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| TraitError::WasmNotLoaded(plugin_id.to_string()))?;

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

        let mut builder = PluginBuilder::new(manifest)
            .with_wasi(self.config.enable_wasi);

        let providers = self.providers.read().await;
        let mut total_functions = 0;

        for provider in providers.iter() {
            Self::register_provider_functions(&mut builder, provider)
                .map_err(|e| TraitError::WasmLoadFailed(format!(
                    "注册宿主函数失败 [{}]: {:?}",
                    provider.namespace(),
                    e
                )))?;
            total_functions += provider.provided_functions().len();
        }

        let plugin = builder.build()
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "创建 Extism 插件失败: {}",
                e
            )))?;

        tracing::info!(
            "插件 {} WASM 模块加载成功，已注册 {} 个宿主函数",
            plugin_id,
            total_functions
        );

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.to_string(), plugin);

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
