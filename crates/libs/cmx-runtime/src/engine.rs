//! WASM 运行时引擎
//!
//! 提供 WASM 模块的加载、实例化和调用功能。
//! 实现 cmx_traits::RuntimeInvoker trait，支持宿主函数注册。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing;

use cmx_traits::{CallerData, HostFunctionProvider, RuntimeInvoker, TraitError, WasmInvokeResult};

use crate::error::RuntimeError;
use crate::instance::{WasmInstance, WasmStoreData};
use crate::linker_adapter::RuntimeLinkerAdapter;

/// WASM 引擎配置
#[derive(Debug, Clone)]
pub struct WasmEngineConfig {
    /// 默认内存上限（字节），默认 256MB
    pub max_memory_bytes: u64,

    /// 是否启用燃料计量，默认 false
    pub enable_fuel: bool,

    /// 最大燃料量，默认 10亿
    pub max_fuel: u64,

    /// 是否启用 WASI（预留），默认 false
    pub enable_wasi: bool,
}

impl Default for WasmEngineConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 256 * 1024 * 1024,
            enable_fuel: false,
            max_fuel: 1_000_000_000,
            enable_wasi: false,
        }
    }
}

/// WASM 运行时引擎
///
/// 核心组件，负责：
/// - 管理宿主函数注册器列表
/// - 创建和配置 wasmtime Linker
/// - 加载/卸载 WASM 模块
/// - 调用 WASM 导出函数
///
/// # 内部可变性
///
/// `WasmEngine` 使用内部可变性模式：
/// - `instances` 和 `host_providers` 都使用 `RwLock` 包装
/// - 所有公共方法都使用 `&self`，不需要外层锁
pub struct WasmEngine {
    /// wasmtime 引擎（编译器和运行时配置）
    engine: wasmtime::Engine,

    /// 已加载的 WASM 实例映射 (plugin_id -> WasmInstance)
    instances: Arc<RwLock<HashMap<String, WasmInstance>>>,

    /// 宿主函数注册器列表（使用 RwLock 支持运行时注册）
    host_providers: RwLock<Vec<Box<dyn HostFunctionProvider>>>,

    /// 引擎配置
    #[allow(dead_code)]
    config: WasmEngineConfig,
}

impl WasmEngine {
    /// 创建新的 WASM 引擎
    ///
    /// # 参数
    ///
    /// * `config` - 引擎配置
    pub fn new(config: WasmEngineConfig) -> Result<Self, RuntimeError> {
        let mut engine_config = wasmtime::Config::new();

        // 启用异步支持
        engine_config.async_support(true);

        // 启用 epoch 中断（支持实例终止）
        engine_config.epoch_interruption(true);

        // 配置燃料计量
        if config.enable_fuel {
            engine_config.consume_fuel(true);
        }

        let engine = wasmtime::Engine::new(&engine_config)
            .map_err(|e| RuntimeError::ConfigError(format!("创建引擎失败: {}", e)))?;

        Ok(Self {
            engine,
            instances: Arc::new(RwLock::new(HashMap::new())),
            host_providers: RwLock::new(Vec::new()),
            config,
        })
    }

    /// 注册宿主函数提供者
    ///
    /// 各模块通过此方法注册自身提供的宿主函数。
    /// 必须在加载任何 WASM 模块之前完成注册。
    ///
    /// # 参数
    ///
    /// * `provider` - 宿主函数注册器（trait 对象）
    pub async fn register_provider(&self, provider: Box<dyn HostFunctionProvider>) {
        tracing::info!(
            "注册宿主函数提供者: {} (提供 {} 个函数)",
            provider.namespace(),
            provider.provided_functions().len()
        );
        let mut providers = self.host_providers.write().await;
        providers.push(provider);
    }

    /// 构建 wasmtime Linker 并注册所有宿主函数
    ///
    /// 遍历所有注册的 HostFunctionProvider，调用其 register_functions 方法。
    async fn build_linker(&self) -> Result<wasmtime::Linker<WasmStoreData>, RuntimeError> {
        let mut linker = wasmtime::Linker::new(&self.engine);

        let providers = self.host_providers.read().await;
        for provider in providers.iter() {
            let mut adapter = RuntimeLinkerAdapter::new(&mut linker);
            provider
                .register_functions(&mut adapter)
                .map_err(|e| RuntimeError::HostFuncRegistrationFailed(e.to_string()))?;

            tracing::debug!(
                "已完成宿主函数注册: {}",
                provider.namespace()
            );
        }

        Ok(linker)
    }

    /// 获取已加载的导出函数列表
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件ID
    pub async fn get_exports(&self, plugin_id: &str) -> Option<Vec<String>> {
        let instances = self.instances.read().await;
        instances
            .get(plugin_id)
            .map(|inst| inst.module_info.exports.clone())
    }
}

#[async_trait]
impl RuntimeInvoker for WasmEngine {
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        _input: &[u8],
        caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError> {
        let start = std::time::Instant::now();

        let mut instances = self.instances.write().await;
        let instance = instances
            .get_mut(plugin_id)
            .ok_or_else(|| TraitError::WasmNotLoaded(plugin_id.to_string()))?;

        // 更新 Store 中的调用者上下文
        instance.store_mut().data_mut().caller_data = caller_data.clone();

        // 获取导出函数
        let func = instance
            .get_export_func(function_name)
            .ok_or_else(|| {
                TraitError::WasmInvokeFailed(format!(
                    "导出函数未找到: {}/{}",
                    plugin_id, function_name
                ))
            })?;

        // 调用 WASM 函数（使用统一的 (i32, i32) -> i32 签名）
        let input_ptr = 0i32;
        let input_len = 0i32;
        let mut results = [wasmtime::Val::I32(0)];

        let mut store = instance.store_mut();
        let call_result = func.call_async(&mut store, &[input_ptr.into(), input_len.into()], &mut results).await;

        match call_result {
            Ok(_) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                Ok(WasmInvokeResult {
                    output: Vec::new(),
                    elapsed_us,
                    fuel_consumed: None,
                })
            }
            Err(e) => Err(TraitError::WasmInvokeFailed(format!(
                "WASM 函数 {}/{} 调用异常: {}",
                plugin_id, function_name, e
            ))),
        }
    }

    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
        // 检查是否已加载
        {
            let instances = self.instances.read().await;
            if instances.contains_key(plugin_id) {
                tracing::warn!("插件 {} 的 WASM 模块已加载，跳过", plugin_id);
                return Ok(());
            }
        }

        // 编译 WASM 模块
        let module = wasmtime::Module::from_file(&self.engine, wasm_path)
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "编译 WASM 文件 {:?} 失败: {}",
                wasm_path, e
            )))?;

        // 创建 Linker 并注册所有宿主函数
        let linker = self
            .build_linker()
            .await
            .map_err(|e| TraitError::WasmLoadFailed(format!("创建 Linker 失败: {}", e)))?;

        // 创建独立的 Store
        let store_data = WasmStoreData::new(CallerData::new(plugin_id, ""));
        let mut store = wasmtime::Store::new(&self.engine, store_data);

        // 实例化模块
        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "实例化 WASM 模块失败: {}",
                e
            )))?;

        // 收集导出函数列表
        let exports: Vec<String> = module
            .exports()
            .filter(|exp| exp.ty().func().is_some())
            .map(|exp| exp.name().to_string())
            .collect();

        tracing::info!(
            "插件 {} WASM 模块加载成功，导出函数: {:?}",
            plugin_id,
            exports
        );

        // 创建 WasmInstance 并保存
        let wasm_instance = WasmInstance::new(
            plugin_id.to_string(),
            instance,
            store,
            exports,
        );

        let mut instances = self.instances.write().await;
        instances.insert(plugin_id.to_string(), wasm_instance);

        Ok(())
    }

    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let mut instances = self.instances.write().await;
        if instances.remove(plugin_id).is_some() {
            tracing::info!("插件 {} WASM 模块已卸载", plugin_id);
            Ok(())
        } else {
            tracing::warn!("插件 {} WASM 模块未加载，无法卸载", plugin_id);
            Ok(())
        }
    }

    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let instances = self.instances.read().await;
        instances.contains_key(plugin_id)
    }
}
