//! CmxService 核心结构
//!
//! 企业级通用服务，作为插件编排的执行引擎，
//! 协调 PluginQuery 和 RuntimeInvoker 完成请求处理。

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{info, warn};

use cmx_traits::{
    LifecycleEvent, PluginLifecycleListener, PluginQuery, RuntimeInvoker, WasmInvokeResult,
};

use crate::error::ServiceError;
use crate::request::{InvokeRequest, InvokeResponse};

/// 服务配置
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// 默认调用超时（毫秒）
    pub invoke_timeout_ms: u64,
    /// 最大重试次数
    pub max_retries: u32,
    /// 是否启用编排缓存
    pub enable_orchestration_cache: bool,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            invoke_timeout_ms: 30000,
            max_retries: 3,
            enable_orchestration_cache: true,
        }
    }
}

/// 企业级通用服务
///
/// 作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker 完成请求处理。
/// 实现 PluginLifecycleListener trait，响应插件生命周期事件。
pub struct CmxService {
    /// 插件查询器（trait 对象，由 web-server 注入）
    plugin_query: Arc<dyn PluginQuery>,
    /// WASM 运行时调用器（trait 对象，由 web-server 注入）
    runtime: Arc<dyn RuntimeInvoker>,
    /// 服务配置
    config: ServiceConfig,
}

impl CmxService {
    /// 创建新的 CmxService 实例
    ///
    /// # 参数
    ///
    /// * `plugin_query` - 插件查询器
    /// * `runtime` - WASM 运行时调用器
    /// * `config` - 服务配置
    pub fn new(
        plugin_query: Arc<dyn PluginQuery>,
        runtime: Arc<dyn RuntimeInvoker>,
        config: ServiceConfig,
    ) -> Self {
        Self {
            plugin_query,
            runtime,
            config,
        }
    }

    /// 使用默认配置创建 CmxService
    pub fn with_defaults(
        plugin_query: Arc<dyn PluginQuery>,
        runtime: Arc<dyn RuntimeInvoker>,
    ) -> Self {
        Self::new(plugin_query, runtime, ServiceConfig::default())
    }

    /// 获取插件查询器引用
    pub fn plugin_query(&self) -> &Arc<dyn PluginQuery> {
        &self.plugin_query
    }

    /// 获取运行时调用器引用
    pub fn runtime(&self) -> &Arc<dyn RuntimeInvoker> {
        &self.runtime
    }

    /// 获取配置引用
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    /// 执行单次插件调用
    ///
    /// # 参数
    ///
    /// * `request` - 调用请求
    ///
    /// # 返回值
    ///
    /// 返回调用响应或错误。
    pub async fn invoke(&self, request: &InvokeRequest) -> Result<InvokeResponse, ServiceError> {
        // 检查插件是否存在且已激活
        let is_active = self.plugin_query.is_active(&request.plugin_id).await?;
        if !is_active {
            return Err(ServiceError::plugin_not_active(&request.plugin_id));
        }

        // 检查 WASM 模块是否已加载
        if !self.runtime.is_loaded(&request.plugin_id).await {
            // 尝试加载
            let wasm_path = self.plugin_query.get_wasm_path(&request.plugin_id).await?;
            self.runtime.load_module(&request.plugin_id, &wasm_path).await?;
        }

        // 序列化输入
        let input_bytes = serde_json::to_vec(&request.input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        // 调用 WASM 函数
        let result: WasmInvokeResult = self.runtime
            .invoke(&request.plugin_id, &request.function_name, &input_bytes)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        // 解析输出
        let output = if result.output.is_empty() {
            None
        } else {
            serde_json::from_slice(&result.output)
                .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?
        };

        Ok(InvokeResponse {
            success: true,
            output,
            elapsed_us: result.elapsed_us,
            fuel_consumed: result.fuel_consumed.unwrap_or(0),
            error: None,
        })
    }
}

/// 实现 PluginLifecycleListener，响应插件生命周期事件
#[async_trait]
impl PluginLifecycleListener for CmxService {
    /// 插件激活时，加载 WASM 模块到运行时
    async fn on_plugin_activated(&self, event: LifecycleEvent) {
        if let Some(wasm_path) = &event.wasm_path {
            match self.runtime.load_module(&event.plugin_id, wasm_path).await {
                Ok(_) => info!("插件 {} WASM 模块加载成功", event.plugin_id),
                Err(e) => warn!("插件 {} WASM 模块加载失败: {}", event.plugin_id, e),
            }
        }
    }

    /// 插件停用时，卸载 WASM 模块
    async fn on_plugin_deactivated(&self, event: LifecycleEvent) {
        match self.runtime.unload_module(&event.plugin_id).await {
            Ok(_) => info!("插件 {} WASM 模块卸载成功", event.plugin_id),
            Err(e) => warn!("插件 {} WASM 模块卸载失败: {}", event.plugin_id, e),
        }
    }

    /// 插件卸载时，清理资源
    async fn on_plugin_uninstalled(&self, event: LifecycleEvent) {
        let _ = self.runtime.unload_module(&event.plugin_id).await;
        info!("插件 {} 资源已清理", event.plugin_id);
    }
}
