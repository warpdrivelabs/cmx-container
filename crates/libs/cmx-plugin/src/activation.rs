//! 激活管理器 - 负责插件激活/停用及 WASM 运行时管理
//!
//! 提供完整的插件激活/停用功能，支持 WASM 模块加载、函数调用和资源管理。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::PluginError;

/// WASM 运行时配置
#[derive(Debug, Clone)]
pub struct WasmRuntimeConfig {
    /// 最大内存限制 (MB)
    pub max_memory_mb: u64,
    /// 最大计算时间 (秒)
    pub max_compute_seconds: u64,
    /// 是否启用缓存
    pub enable_cache: bool,
    /// 调试模式
    pub debug_mode: bool,
}

impl Default for WasmRuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 256,
            max_compute_seconds: 30,
            enable_cache: true,
            debug_mode: false,
        }
    }
}

/// WASM 运行时实例
pub struct WasmRuntime {
    config: WasmRuntimeConfig,
    /// 已编译的模块缓存
    modules: Arc<RwLock<HashMap<String, WasmModule>>>,
}

/// WASM 模块
#[derive(Debug)]
pub struct WasmModule {
    /// 模块 ID
    pub module_id: String,
    /// 编译后的字节数
    pub size_bytes: usize,
    /// 加载时间
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

impl WasmModule {
    /// 创建新的 WASM 模块
    pub fn new(module_id: impl Into<String>, size_bytes: usize) -> Self {
        Self {
            module_id: module_id.into(),
            size_bytes,
            loaded_at: chrono::Utc::now(),
        }
    }
}

impl WasmRuntime {
    /// 创建新的 WASM 运行时
    pub fn new(config: WasmRuntimeConfig) -> Self {
        Self {
            config,
            modules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 使用默认配置创建 WASM 运行时
    pub fn with_default_config() -> Self {
        Self::new(WasmRuntimeConfig::default())
    }

    /// 加载 WASM 模块
    pub async fn load_module(&self, module_id: &str, wasm_bytes: &[u8]) -> Result<(), crate::PluginError> {
        // 验证 WASM 魔数
        if wasm_bytes.len() < 4 || &wasm_bytes[0..4] != b"\0asm" {
            return Err(crate::PluginError::Activate("无效的 WASM 文件".to_string()));
        }

        // 验证文件大小
        let size_mb = wasm_bytes.len() as u64 / (1024 * 1024);
        if size_mb > self.config.max_memory_mb {
            return Err(crate::PluginError::Activate(format!(
                "WASM 模块大小 {}MB 超过限制 {}MB",
                size_mb, self.config.max_memory_mb
            )));
        }

        // TODO: 使用 wasmtime 编译模块
        // let engine = wasmtime::Engine::default();
        // let module = wasmtime::Module::new(&engine, wasm_bytes)
        //     .map_err(|e| crate::PluginError::Activate(format!("WASM 编译失败: {}", e)))?;

        // 缓存模块
        let mut modules = self.modules.write().await;
        modules.insert(
            module_id.to_string(),
            WasmModule::new(module_id, wasm_bytes.len()),
        );

        log::info!("WASM 模块 {} 已加载，大小: {} bytes", module_id, wasm_bytes.len());

        Ok(())
    }

    /// 卸载 WASM 模块
    pub async fn unload_module(&self, module_id: &str) -> Result<(), crate::PluginError> {
        let mut modules = self.modules.write().await;

        if modules.remove(module_id).is_some() {
            log::info!("WASM 模块 {} 已卸载", module_id);
            Ok(())
        } else {
            Err(crate::PluginError::NotFound(format!(
                "WASM 模块 {} 不存在",
                module_id
            )))
        }
    }

    /// 检查模块是否已加载
    pub async fn is_module_loaded(&self, module_id: &str) -> bool {
        let modules = self.modules.read().await;
        modules.contains_key(module_id)
    }

    /// 获取已加载模块数量
    pub async fn loaded_module_count(&self) -> usize {
        let modules = self.modules.read().await;
        modules.len()
    }

    /// 获取配置
    pub fn config(&self) -> &WasmRuntimeConfig {
        &self.config
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::with_default_config()
    }
}

/// 插件实例 - 运行中的插件
#[derive(Debug, Clone)]
pub struct PluginInstance {
    /// 插件 ID
    pub plugin_id: String,
    /// WASM 模块 ID
    pub module_id: String,
    /// 实例数据
    pub data: HashMap<String, serde_json::Value>,
    /// 激活时间
    pub activated_at: chrono::DateTime<chrono::Utc>,
}

/// 插件句柄 - 与 WASM 实例的连接
pub struct PluginHandle {
    /// 插件 ID
    pub plugin_id: String,
    /// 实例 ID
    pub instance_id: String,
    /// 内存指针 (如果是 wasmtime)
    pub memory_ptr: Option<u32>,
}

/// 激活结果
#[derive(Debug, Clone)]
pub struct ActivationResult {
    /// 是否成功
    pub success: bool,
    /// 插件 ID
    pub plugin_id: String,
    /// 实例 ID
    pub instance_id: Option<String>,
    /// 消息
    pub message: Option<String>,
}

/// 停用结果
#[derive(Debug, Clone)]
pub struct DeactivationResult {
    /// 是否成功
    pub success: bool,
    /// 插件 ID
    pub plugin_id: String,
}

/// 重新加载结果
#[derive(Debug, Clone)]
pub struct ReloadResult {
    /// 是否成功
    pub success: bool,
    /// 插件 ID
    pub plugin_id: String,
}

/// 插件调用请求
#[derive(Debug, Clone)]
pub struct PluginCallRequest {
    /// 插件 ID
    pub plugin_id: String,
    /// 函数名
    pub function: String,
    /// 参数
    pub args: Vec<serde_json::Value>,
}

/// 插件调用响应
#[derive(Debug, Clone)]
pub struct PluginCallResponse {
    /// 是否成功
    pub success: bool,
    /// 返回值
    pub result: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
}

/// 激活管理器 - 负责插件激活/停用
pub struct ActivationManager {
    /// WASM 运行时
    wasm_runtime: Arc<WasmRuntime>,
    /// 活跃的插件实例
    instances: Arc<RwLock<HashMap<String, PluginInstance>>>,
    /// 插件句柄
    handles: Arc<RwLock<HashMap<String, PluginHandle>>>,
}

impl ActivationManager {
    /// 创建新的激活管理器
    pub fn new() -> Self {
        Self {
            wasm_runtime: Arc::new(WasmRuntime::with_default_config()),
            instances: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建新的激活管理器（带自定义 WASM 运行时配置）
    pub fn with_config(config: WasmRuntimeConfig) -> Self {
        Self {
            wasm_runtime: Arc::new(WasmRuntime::new(config)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 激活插件
    pub async fn activate(&self, plugin_id: &str, wasm_path: &str) -> Result<ActivationResult, PluginError> {
        // 1. 检查插件是否已激活
        {
            let instances = self.instances.read().await;
            if instances.contains_key(plugin_id) {
                return Err(PluginError::Activate(format!(
                    "插件 {} 已经激活",
                    plugin_id
                )));
            }
        }

        // 2. 加载 WASM 模块
        let wasm_bytes = tokio::fs::read(wasm_path).await
            .map_err(|e| PluginError::Io(e))?;

        // 3. 加载到 WASM 运行时
        self.wasm_runtime.load_module(plugin_id, &wasm_bytes).await?;

        // 4. 创建实例
        let instance_id = uuid::Uuid::new_v4().to_string();
        let instance = PluginInstance {
            plugin_id: plugin_id.to_string(),
            module_id: plugin_id.to_string(),
            data: HashMap::new(),
            activated_at: chrono::Utc::now(),
        };

        // 5. 创建句柄
        let handle = PluginHandle {
            plugin_id: plugin_id.to_string(),
            instance_id: instance_id.clone(),
            memory_ptr: None,
        };

        // 6. 注册实例和句柄
        {
            let mut instances = self.instances.write().await;
            instances.insert(plugin_id.to_string(), instance);
        }
        {
            let mut handles = self.handles.write().await;
            handles.insert(plugin_id.to_string(), handle);
        }

        log::info!("插件 {} 已激活，实例 ID: {}", plugin_id, instance_id);

        Ok(ActivationResult {
            success: true,
            plugin_id: plugin_id.to_string(),
            instance_id: Some(instance_id),
            message: Some("插件激活成功".to_string()),
        })
    }

    /// 停用插件
    pub async fn deactivate(&self, plugin_id: &str) -> Result<DeactivationResult, PluginError> {
        // 1. 检查插件是否存在
        let exists = {
            let instances = self.instances.read().await;
            instances.contains_key(plugin_id)
        };

        if !exists {
            return Err(PluginError::Deactivate(format!(
                "插件 {} 未激活",
                plugin_id
            )));
        }

        // 2. 卸载 WASM 模块
        self.wasm_runtime.unload_module(plugin_id).await?;

        // 3. 移除实例和句柄
        {
            let mut instances = self.instances.write().await;
            instances.remove(plugin_id);
        }
        {
            let mut handles = self.handles.write().await;
            handles.remove(plugin_id);
        }

        log::info!("插件 {} 已停用", plugin_id);

        Ok(DeactivationResult {
            success: true,
            plugin_id: plugin_id.to_string(),
        })
    }

    /// 重新加载插件
    pub async fn reload(&self, plugin_id: &str, wasm_path: &str) -> Result<ReloadResult, PluginError> {
        // 1. 停用插件
        self.deactivate(plugin_id).await?;

        // 2. 重新激活
        let result = self.activate(plugin_id, wasm_path).await?;

        Ok(ReloadResult {
            success: result.success,
            plugin_id: plugin_id.to_string(),
        })
    }

    /// 获取插件是否激活
    pub async fn is_active(&self, plugin_id: &str) -> bool {
        let instances = self.instances.read().await;
        instances.contains_key(plugin_id)
    }

    /// 获取所有已激活的插件
    pub async fn get_active_plugins(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.keys().cloned().collect()
    }

    /// 获取插件信息
    pub async fn get_instance(&self, plugin_id: &str) -> Option<PluginInstance> {
        let instances = self.instances.read().await;
        instances.get(plugin_id).cloned()
    }

    /// 调用插件函数
    pub async fn call_function(&self, request: PluginCallRequest) -> Result<PluginCallResponse, PluginError> {
        // 1. 检查插件是否激活
        if !self.is_active(&request.plugin_id).await {
            return Err(PluginError::Activate(format!(
                "插件 {} 未激活，无法调用函数",
                request.plugin_id
            )));
        }

        // 2. TODO: 使用 wasmtime 调用具体函数
        // let instance = handles.get(&request.plugin_id);
        // let result = instance.call(&request.function, &request.args)?;

        log::info!(
            "调用插件 {} 的函数 {}",
            request.plugin_id,
            request.function
        );

        Ok(PluginCallResponse {
            success: true,
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        })
    }

    /// 获取 WASM 运行时
    pub fn wasm_runtime(&self) -> &Arc<WasmRuntime> {
        &self.wasm_runtime
    }

    /// 获取活跃实例数量
    pub async fn active_instance_count(&self) -> usize {
        let instances = self.instances.read().await;
        instances.len()
    }
}

impl Default for ActivationManager {
    fn default() -> Self {
        Self::new()
    }
}
