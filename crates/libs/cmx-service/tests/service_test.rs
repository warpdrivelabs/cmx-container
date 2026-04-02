//! CmxService 单元测试
//!
//! 测试服务层的核心功能，使用 Mock 对象模拟依赖。

use cmx_service::{CmxService, ServiceConfig, InvokeRequest, Orchestrator, Orchestration, OrchestrationStep, StepInput};
use cmx_traits::{PluginQuery, PluginSnapshot, RuntimeInvoker, WasmInvokeResult, CallerData, TraitError};
use std::sync::Arc;
use std::path::Path;
use async_trait::async_trait;
use std::collections::HashMap;

/// Mock PluginQuery 实现
struct MockPluginQuery {
    /// 模拟的插件列表
    plugins: HashMap<String, PluginSnapshot>,
    /// 激活状态
    active_plugins: Vec<String>,
}

impl MockPluginQuery {
    /// 创建新的 Mock
    fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            active_plugins: Vec::new(),
        }
    }
    
    /// 添加插件
    fn with_plugin(mut self, plugin: PluginSnapshot) -> Self {
        self.plugins.insert(plugin.plugin_id.clone(), plugin);
        self
    }
    
    /// 设置激活的插件
    fn with_active(mut self, plugin_id: &str) -> Self {
        self.active_plugins.push(plugin_id.to_string());
        self
    }
}

#[async_trait]
impl PluginQuery for MockPluginQuery {
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError> {
        Ok(self.plugins.get(plugin_id).cloned())
    }
    
    async fn is_active(&self, plugin_id: &str) -> Result<bool, TraitError> {
        Ok(self.active_plugins.contains(&plugin_id.to_string()))
    }
    
    async fn get_wasm_path(&self, _plugin_id: &str) -> Result<std::path::PathBuf, TraitError> {
        Ok(std::path::PathBuf::from("test.wasm"))
    }
    
    async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, TraitError> {
        let result: Vec<PluginSnapshot> = self.plugins.values()
            .filter(|p| self.active_plugins.contains(&p.plugin_id))
            .cloned()
            .collect();
        Ok(result)
    }
    
    async fn list_plugins(&self, _filter: &cmx_traits::PluginFilter) -> Result<Vec<PluginSnapshot>, TraitError> {
        Ok(self.plugins.values().cloned().collect())
    }
}

/// Mock RuntimeInvoker 实现
struct MockRuntimeInvoker {
    /// 已加载的模块
    loaded_modules: std::collections::HashSet<String>,
    /// 调用计数
    invoke_count: std::sync::atomic::AtomicU64,
}

impl MockRuntimeInvoker {
    /// 创建新的 Mock
    fn new() -> Self {
        Self {
            loaded_modules: std::collections::HashSet::new(),
            invoke_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl RuntimeInvoker for MockRuntimeInvoker {
    async fn load_module(&self, plugin_id: &str, _wasm_path: &Path) -> Result<(), TraitError> {
        // Mock 实现 - 标记为已加载
        let _ = plugin_id;
        Ok(())
    }
    
    async fn unload_module(&self, _plugin_id: &str) -> Result<(), TraitError> {
        Ok(())
    }
    
    async fn is_loaded(&self, plugin_id: &str) -> bool {
        self.loaded_modules.contains(plugin_id)
    }
    
    async fn invoke(
        &self,
        _plugin_id: &str,
        _function_name: &str,
        _input: &[u8],
        _caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError> {
        // 增加调用计数
        self.invoke_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // 返回模拟结果
        Ok(WasmInvokeResult {
            output: br#"{"result": "success"}"#.to_vec(),
            elapsed_us: 1000,
            fuel_consumed: Some(5000),
        })
    }
}

/// 创建测试用的插件快照
fn create_test_plugin(id: &str) -> PluginSnapshot {
    PluginSnapshot {
        plugin_id: id.to_string(),
        name: format!("Test Plugin {}", id),
        version: "1.0.0".to_string(),
        status: "installed".to_string(),
        install_path: "/plugins".to_string(),
        wasm_path: Some("test.wasm".to_string()),
        plugin_type: "wasm".to_string(),
        domain_code: "default".to_string(),
        application_code: "test".to_string(),
        module_code: "test".to_string(),
    }
}

/// 测试 CmxService 创建
#[tokio::test]
async fn test_service_creation() {
    let plugin_query = Arc::new(MockPluginQuery::new());
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let service = CmxService::new(plugin_query, runtime, ServiceConfig::default());
    
    // 验证配置
    assert!(service.config().invoke_timeout_ms > 0);
    assert!(service.config().max_retries > 0);
}

/// 测试服务调用 - 插件未激活
#[tokio::test]
async fn test_service_invoke_plugin_not_active() {
    let plugin = create_test_plugin("test-plugin");
    
    let plugin_query = Arc::new(
        MockPluginQuery::new()
            .with_plugin(plugin)
    );
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let service = CmxService::new(plugin_query, runtime, ServiceConfig::default());
    
    let request = InvokeRequest {
        plugin_id: "test-plugin".to_string(),
        function_name: "test_function".to_string(),
        input: serde_json::json!({"data": "test"}),
        db_id: Some("default".to_string()),
        request_id: Some("req-001".to_string()),
        tenant_id: None,
    };
    
    let result = service.invoke(&request).await;
    
    // 插件未激活，应该返回错误
    assert!(result.is_err());
}

/// 测试服务调用 - 插件已激活
#[tokio::test]
async fn test_service_invoke_plugin_active() {
    let plugin = create_test_plugin("active-plugin");
    
    let plugin_query = Arc::new(
        MockPluginQuery::new()
            .with_plugin(plugin)
            .with_active("active-plugin")
    );
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let service = CmxService::new(plugin_query, runtime, ServiceConfig::default());
    
    let request = InvokeRequest {
        plugin_id: "active-plugin".to_string(),
        function_name: "test_function".to_string(),
        input: serde_json::json!({"data": "test"}),
        db_id: Some("default".to_string()),
        request_id: Some("req-001".to_string()),
        tenant_id: None,
    };
    
    let result = service.invoke(&request).await;
    
    // 插件已激活，应该成功
    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.success);
    assert!(response.output.is_some());
}

/// 测试编排执行器创建
#[tokio::test]
async fn test_orchestrator_creation() {
    let plugin_query = Arc::new(MockPluginQuery::new());
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let orchestrator = Orchestrator::new(runtime, plugin_query);
    
    // 验证创建成功
    let _ = orchestrator;
}

/// 测试空编排执行
#[tokio::test]
async fn test_orchestration_empty_steps() {
    let plugin_query = Arc::new(MockPluginQuery::new());
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let orchestrator = Orchestrator::new(runtime, plugin_query);
    
    let orchestration = Orchestration {
        id: "test-flow".to_string(),
        name: "测试流程".to_string(),
        description: None,
        steps: vec![],
    };
    
    let caller_data = CallerData::new("__test__", "default");
    let result = orchestrator.execute(&orchestration, &serde_json::json!({}), &caller_data).await;
    
    // 空步骤应该成功
    assert!(result.is_ok());
    let orchestration_result = result.unwrap();
    assert!(orchestration_result.success);
    assert!(orchestration_result.step_results.is_empty());
}

/// 测试单步骤编排
#[tokio::test]
async fn test_orchestration_single_step() {
    let plugin = create_test_plugin("step-plugin");
    
    let plugin_query = Arc::new(
        MockPluginQuery::new()
            .with_plugin(plugin)
            .with_active("step-plugin")
    );
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let orchestrator = Orchestrator::new(runtime, plugin_query);
    
    let orchestration = Orchestration {
        id: "single-step-flow".to_string(),
        name: "单步骤流程".to_string(),
        description: None,
        steps: vec![
            OrchestrationStep {
                step_id: "step1".to_string(),
                plugin_id: "step-plugin".to_string(),
                function_name: "process".to_string(),
                input: StepInput::Static { 
                    value: serde_json::json!({"data": "input"}) 
                },
                parallel: false,
                condition: None,
            }
        ],
    };
    
    let caller_data = CallerData::new("__test__", "default");
    let result = orchestrator.execute(&orchestration, &serde_json::json!({}), &caller_data).await;
    
    assert!(result.is_ok());
    let orchestration_result = result.unwrap();
    assert!(orchestration_result.success);
    assert_eq!(orchestration_result.step_results.len(), 1);
    assert!(orchestration_result.step_results[0].success);
}

/// 测试服务配置默认值
#[test]
fn test_service_config_defaults() {
    let config = ServiceConfig::default();
    
    assert_eq!(config.invoke_timeout_ms, 30000);
    assert_eq!(config.max_retries, 3);
    assert!(config.enable_orchestration_cache);
}
