//! 编排执行器集成测试公共辅助模块
//!
//! 提供 mock 实现和流程构造工具，用于在不依赖真实 WASM 运行时和数据库的情况下
//! 测试 Orchestrator 的执行逻辑。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use cmx_core::model::service::{
    FunctionOutput, NodeData, NodeMeta, NodeNodeMeta, NodePosition, NodeSize,
    ServiceDefinition, ServiceEdge, ServiceFlow, ServiceNode, ServiceOrchestration, SVRContext,
};
use cmx_service::Orchestrator;
use cmx_traits::error::TraitError;
use cmx_traits::plugin::{PluginFilter, PluginQuery, PluginSnapshot};
use cmx_traits::runtime::{InvokeOptions, RuntimeInvoker, WasmInvokeResult};
use cmx_traits::service::{
    ServicePageFilter, ServicePageResult, ServiceQuery,
};
use serde_json::json;

// ============================================================================
// Mock RuntimeInvoker
// ============================================================================

/// 可配置的 WASM 运行时 mock
///
/// 通过函数名映射到预配置的返回结果，支持：
/// - 成功返回：返回指定的 FunctionOutput
/// - 失败返回：返回指定的 TraitError
pub struct MockRuntimeInvoker {
    /// 函数名 -> 成功输出的映射
    /// key 格式: "{plugin_id}:{function_name}"
    outputs: Mutex<HashMap<String, FunctionOutput>>,
    /// 函数名 -> 失败错误消息的映射（优先于 outputs）
    /// key 格式: "{plugin_id}:{function_name}"
    /// 存储 String 而非 TraitError，因为 TraitError 未实现 Clone
    errors: Mutex<HashMap<String, String>>,
    /// 已加载的插件集合
    loaded: Mutex<std::collections::HashSet<String>>,
}

impl MockRuntimeInvoker {
    pub fn new() -> Self {
        Self {
            outputs: Mutex::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
            loaded: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// 配置某函数的成功返回值
    pub fn with_success(mut self, plugin_id: &str, function_name: &str, result: serde_json::Value) -> Self {
        let key = format!("{}:{}", plugin_id, function_name);
        self.outputs.get_mut().unwrap().insert(key, FunctionOutput::new(result));
        self
    }

    /// 配置某函数的失败错误
    #[allow(dead_code)]
    pub fn with_error(mut self, plugin_id: &str, function_name: &str, error_message: impl Into<String>) -> Self {
        let key = format!("{}:{}", plugin_id, function_name);
        self.errors.get_mut().unwrap().insert(key, error_message.into());
        self
    }

    /// 标记某插件已加载
    pub fn with_loaded(mut self, plugin_id: &str) -> Self {
        self.loaded.get_mut().unwrap().insert(plugin_id.to_string());
        self
    }
}

impl Default for MockRuntimeInvoker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeInvoker for MockRuntimeInvoker {
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        _input: &[u8],
        _options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError> {
        let key = format!("{}:{}", plugin_id, function_name);

        // 优先检查错误配置
        if let Some(msg) = self.errors.lock().unwrap().get(&key) {
            return Err(TraitError::WasmInvokeFailed(msg.clone()));
        }

        // 查找成功输出
        let output = self.outputs.lock().unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_else(|| FunctionOutput::new(json!(null)));

        // 序列化为 MsgPack（与生产代码保持一致）
        let output_bytes = rmp_serde::to_vec(&output)
            .map_err(|e| TraitError::WasmInvokeFailed(format!("序列化失败: {}", e)))?;

        Ok(WasmInvokeResult {
            output: output_bytes,
            elapsed_us: 100,
            fuel_consumed: None,
        })
    }

    async fn load_module(&self, plugin_id: &str, _wasm_path: &Path) -> Result<(), TraitError> {
        self.loaded.lock().unwrap().insert(plugin_id.to_string());
        Ok(())
    }

    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        self.loaded.lock().unwrap().remove(plugin_id);
        Ok(())
    }

    async fn is_loaded(&self, plugin_id: &str) -> bool {
        self.loaded.lock().unwrap().contains(plugin_id)
    }
}

// ============================================================================
// Mock PluginQuery
// ============================================================================

/// 简单的 PluginQuery mock
///
/// 由于 MockRuntimeInvoker 已预加载插件，PluginQuery 的方法通常不会被调用。
/// 这里提供最小化实现以满足 trait 要求。
pub struct MockPluginQuery {
    install_path: PathBuf,
}

impl MockPluginQuery {
    pub fn new() -> Self {
        Self {
            install_path: PathBuf::from("/tmp/mock_plugins"),
        }
    }
}

impl Default for MockPluginQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PluginQuery for MockPluginQuery {
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError> {
        Ok(Some(PluginSnapshot {
            plugin_id: plugin_id.to_string(),
            name: plugin_id.to_string(),
            version: "1.0.0".to_string(),
            status: "activated".to_string(),
            install_path: self.install_path.to_string_lossy().to_string(),
            wasm_path: Some("mock.wasm".to_string()),
            plugin_type: "wasm".to_string(),
            domain_code: "test".to_string(),
            application_code: "test".to_string(),
            module_code: "test".to_string(),
            source_path: None,
        }))
    }

    async fn is_installed(&self, _plugin_id: &str) -> Result<bool, TraitError> {
        Ok(true)
    }

    async fn is_active(&self, _plugin_id: &str) -> Result<bool, TraitError> {
        Ok(true)
    }

    async fn get_wasm_path(&self, plugin_id: &str) -> Result<PathBuf, TraitError> {
        Ok(self.install_path.join(plugin_id).join("mock.wasm"))
    }

    async fn list_plugins(&self, _filter: &PluginFilter) -> Result<Vec<PluginSnapshot>, TraitError> {
        Ok(vec![])
    }
}

// ============================================================================
// Mock ServiceQuery
// ============================================================================

/// 可配置的 ServiceQuery mock
///
/// 预配置服务编排定义，按 service_key 返回。
pub struct MockServiceQuery {
    /// service_key -> ServiceOrchestration
    orchestrations: Mutex<HashMap<String, ServiceOrchestration>>,
}

impl MockServiceQuery {
    pub fn new() -> Self {
        Self {
            orchestrations: Mutex::new(HashMap::new()),
        }
    }

    /// 注册一个服务编排
    pub fn with_orchestration(mut self, orchestration: ServiceOrchestration) -> Self {
        let key = orchestration.code.clone();
        self.orchestrations.get_mut().unwrap().insert(key, orchestration);
        self
    }
}

impl Default for MockServiceQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceQuery for MockServiceQuery {
    async fn get_service(&self, service_key: &str) -> Result<Option<ServiceDefinition>, TraitError> {
        // 返回一个最小化的 ServiceDefinition
        Ok(Some(ServiceDefinition {
            id: format!("id_{}", service_key),
            app_id: "test_app".to_string(),
            service_key: service_key.to_string(),
            service_name: format!("测试服务_{}", service_key),
            description: "测试服务".to_string(),
            plugin_id: "test_plugin".to_string(),
            status: 1,
            version: "1.0.0".to_string(),
            config: None,
            domain_code: "test".to_string(),
            application_code: "test".to_string(),
            module_code: "test".to_string(),
            domain_name: "测试域".to_string(),
            application_name: "测试应用".to_string(),
            module_name: "测试模块".to_string(),
            plugin_name: "测试插件".to_string(),
            api_doc: None,
        }))
    }

    async fn get_services_by_plugin(&self, _plugin_id: &str) -> Result<Vec<ServiceDefinition>, TraitError> {
        Ok(vec![])
    }

    async fn list_active_services(&self) -> Result<Vec<ServiceDefinition>, TraitError> {
        Ok(vec![])
    }

    async fn get_orchestration(&self, service_key: &str) -> Result<Option<ServiceOrchestration>, TraitError> {
        Ok(self.orchestrations.lock().unwrap().get(service_key).cloned())
    }

    async fn page_services(
        &self,
        _filter: ServicePageFilter,
        _page: u64,
        _size: u64,
    ) -> Result<ServicePageResult, TraitError> {
        Ok(ServicePageResult { items: vec![], total: 0 })
    }
}

// ============================================================================
// 流程构造工具
// ============================================================================

/// 构造测试用节点元数据
pub fn make_meta() -> NodeMeta {
    NodeMeta {
        z_index: 1,
        size: NodeSize { width: 100, height: 50 },
        position: NodePosition { x: 0.0, y: 0.0 },
    }
}

/// 构造开始节点
pub fn make_start_node(id: &str) -> ServiceNode {
    ServiceNode {
        id: id.to_string(),
        node_type: "skylake-start".to_string(),
        parent: None,
        meta: make_meta(),
        data: Some(NodeData {
            name: "开始".to_string(),
            node_meta: None,
            inputs: serde_json::Value::Array(vec![]),
            outputs: serde_json::Value::Array(vec![]),
            options: None,
        }),
    }
}

/// 构造结束节点
pub fn make_end_node(id: &str) -> ServiceNode {
    ServiceNode {
        id: id.to_string(),
        node_type: "skylake-end".to_string(),
        parent: None,
        meta: make_meta(),
        data: Some(NodeData {
            name: "结束".to_string(),
            node_meta: None,
            inputs: serde_json::Value::Array(vec![]),
            outputs: serde_json::Value::Array(vec![]),
            options: None,
        }),
    }
}

/// 构造函数节点
pub fn make_func_node(id: &str, name: &str, plugin_id: &str, function_name: &str) -> ServiceNode {
    ServiceNode {
        id: id.to_string(),
        node_type: "skylake-func".to_string(),
        parent: None,
        meta: make_meta(),
        data: Some(NodeData {
            name: name.to_string(),
            node_meta: Some(NodeNodeMeta {
                plugin_id: plugin_id.to_string(),
                plugin_name: plugin_id.to_string(),
                plugin_version: "1.0.0".to_string(),
                function_name: function_name.to_string(),
                database_id: None,
            }),
            inputs: serde_json::Value::Array(vec![]),
            outputs: serde_json::Value::Array(vec![]),
            options: None,
        }),
    }
}

/// 构造 switch 节点
#[allow(dead_code)]
pub fn make_switch_node(id: &str, name: &str, plugin_id: &str, function_name: &str) -> ServiceNode {
    ServiceNode {
        id: id.to_string(),
        node_type: "skylake-switch".to_string(),
        parent: None,
        meta: make_meta(),
        data: Some(NodeData {
            name: name.to_string(),
            node_meta: Some(NodeNodeMeta {
                plugin_id: plugin_id.to_string(),
                plugin_name: plugin_id.to_string(),
                plugin_version: "1.0.0".to_string(),
                function_name: function_name.to_string(),
                database_id: None,
            }),
            inputs: serde_json::Value::Array(vec![]),
            outputs: serde_json::Value::Array(vec![]),
            options: Some(vec!["1".to_string(), "2".to_string()]),
        }),
    }
}

/// 构造事务框内节点（有 parent）
#[allow(dead_code)]
pub fn make_func_node_with_parent(
    id: &str,
    name: &str,
    plugin_id: &str,
    function_name: &str,
    parent_id: &str,
) -> ServiceNode {
    let mut node = make_func_node(id, name, plugin_id, function_name);
    node.parent = Some(parent_id.to_string());
    node
}

/// 构造一条边
pub fn make_edge(source: &str, source_port: &str, target: &str) -> ServiceEdge {
    ServiceEdge {
        source_node_id: source.to_string(),
        source_port_id: source_port.to_string(),
        target_node_id: target.to_string(),
        target_port_id: "in".to_string(),
    }
}

/// 构造服务编排定义
pub fn make_orchestration(code: &str, flow: ServiceFlow) -> ServiceOrchestration {
    ServiceOrchestration {
        name: format!("测试编排_{}", code),
        code: code.to_string(),
        description: "测试用编排".to_string(),
        flow,
        source_str: String::new(),
    }
}

/// 构造测试用 SVRContext
pub fn make_svr_context(initial_input: serde_json::Value) -> SVRContext {
    SVRContext::new(
        initial_input,
        HashMap::new(),
        Utc::now(),
        "test_request_id".to_string(),
    )
}

/// 创建编排器实例（注入 mock 依赖）
pub fn create_orchestrator(
    runtime: MockRuntimeInvoker,
    service_query: MockServiceQuery,
) -> Orchestrator {
    let runtime: Arc<dyn RuntimeInvoker> = Arc::new(runtime);
    let plugin_query: Arc<dyn PluginQuery> = Arc::new(MockPluginQuery::new());
    let service_query: Arc<dyn ServiceQuery> = Arc::new(service_query);
    Orchestrator::new(runtime, plugin_query, service_query, "db_default".to_string())
}
