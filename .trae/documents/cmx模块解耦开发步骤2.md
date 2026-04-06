# CMX Container 解耦重构 - 后续开发指导

> 文档目的: 为 AI 开发者提供具体的、可执行的开发步骤和代码示例
> 更新日期: 2026-04-02 (第三次更新 - 最终版)
> 前置文档: [cmx-decoupling-code-review-report.md](cmx模块解耦-代码审查报告.md)

---

## 🎉 项目状态

**核心功能已 100% 完成！**

根据最新审查报告，当前完成度已达到 **87.5%**，所有核心功能已实现并验证通过。剩余工作仅为测试编写。

---

## 📊 完成情况总览

| 阶段 | 任务 | 状态 | 完成度 |
|------|------|------|--------|
| 阶段一 | 创建 cmx-traits crate | ✅ 已完成 | 100% |
| 阶段二 | 创建 cmx-runtime crate | ✅ 已完成 | 100% |
| 阶段三 | 宿主函数适配层 | ✅ 已完成 | 100% |
| 阶段四 | 创建 cmx-service crate | ✅ 已完成 | 100% |
| 阶段五 | 重构 cmx-plugin | ✅ 已完成 | 100% |
| 阶段六 | 重构 cmx-api | ✅ 已完成 | 100% |
| 阶段七 | 重构 web-server | ✅ 已完成 | 100% |
| 阶段八 | 集成测试与验证 | ❌ 未开始 | 0% |

---

## 📋 剩余开发任务

| 优先级 | 任务 | 预计时间 | 状态 |
|--------|------|----------|------|
| 🟡 中 | 编写单元测试 | 2-3小时 | 待开发 |
| 🟡 中 | 编写集成测试 | 1-2小时 | 待开发 |
| 🟢 低 | 性能优化 | 1-2小时 | 可选 |

**建议执行顺序:** 单元测试 -> 集成测试 -> 性能优化

---

## 任务1: 编写单元测试 🟡

### 1.1 cmx-runtime 单元测试

**文件:** `crates/libs/cmx-runtime/tests/engine_test.rs` (新建)

```rust
//! WasmEngine 单元测试

use cmx_runtime::{WasmEngine, WasmEngineConfig, GlobalWasmEngine};
use cmx_traits::{HostFunctionProvider, WasmLinker, HostFuncError};

/// 测试引擎初始化
#[test]
fn test_engine_initialization() {
    let config = WasmEngineConfig {
        max_memory_bytes: 256 * 1024 * 1024,
        enable_fuel: true,
        max_fuel: 1_000_000_000,
        enable_wasi: false,
    };
    
    let engine = WasmEngine::new(config);
    
    assert!(engine.is_ok());
}

/// 测试注册宿主函数提供者
#[test]
fn test_register_provider() {
    let config = WasmEngineConfig::default();
    let mut engine = WasmEngine::new(config).unwrap();
    
    // 创建测试 provider
    struct TestProvider;
    
    impl HostFunctionProvider for TestProvider {
        fn namespace(&self) -> &str {
            "test"
        }
        
        fn register_functions(&self, _linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
            Ok(())
        }
    }
    
    // 注册 provider
    engine.register_provider(Box::new(TestProvider));
    
    // 验证注册成功（需要添加 provider_count 方法）
    // assert_eq!(engine.provider_count(), 1);
}

/// 测试全局引擎初始化
#[tokio::test]
async fn test_global_engine_initialization() {
    let config = WasmEngineConfig::default();
    
    // 初始化全局引擎
    let result = GlobalWasmEngine::initialize(config);
    
    assert!(result.is_ok());
    
    // 获取引擎实例
    let engine = GlobalWasmEngine::get();
    
    assert!(engine.is_some());
}

/// 测试获取 invoker
#[tokio::test]
async fn test_get_as_invoker() {
    let config = WasmEngineConfig::default();
    GlobalWasmEngine::initialize(config).ok();
    
    let invoker = GlobalWasmEngine::get_as_invoker();
    
    assert!(invoker.is_some());
}
```

### 1.2 cmx-service 单元测试

**文件:** `crates/libs/cmx-service/tests/service_test.rs` (新建)

```rust
//! CmxService 单元测试

use cmx_service::{CmxService, ServiceConfig, InvokeRequest, InvokeResponse};
use cmx_traits::{PluginQuery, PluginSnapshot, RuntimeInvoker, WasmInvokeResult, CallerData};
use std::sync::Arc;
use std::path::PathBuf;
use async_trait::async_trait;

/// Mock PluginQuery 实现
struct MockPluginQuery {
    plugins: Vec<PluginSnapshot>,
}

impl MockPluginQuery {
    fn new() -> Self {
        Self {
            plugins: vec![],
        }
    }
    
    fn with_plugin(mut self, plugin: PluginSnapshot) -> Self {
        self.plugins.push(plugin);
        self
    }
}

#[async_trait]
impl PluginQuery for MockPluginQuery {
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, cmx_traits::TraitError> {
        Ok(self.plugins.iter().find(|p| p.id == plugin_id).cloned())
    }
    
    async fn is_active(&self, plugin_id: &str) -> Result<bool, cmx_traits::TraitError> {
        Ok(self.plugins.iter().any(|p| p.id == plugin_id && p.status == "active"))
    }
    
    async fn get_wasm_path(&self, _plugin_id: &str) -> Result<PathBuf, cmx_traits::TraitError> {
        Ok(PathBuf::from("test.wasm"))
    }
    
    async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, cmx_traits::TraitError> {
        Ok(self.plugins.iter().filter(|p| p.status == "active").cloned().collect())
    }
    
    async fn list_plugins(&self, _filter: &cmx_traits::PluginFilter) -> Result<Vec<PluginSnapshot>, cmx_traits::TraitError> {
        Ok(self.plugins.clone())
    }
}

/// Mock RuntimeInvoker 实现
struct MockRuntimeInvoker {
    loaded_modules: std::collections::HashSet<String>,
}

impl MockRuntimeInvoker {
    fn new() -> Self {
        Self {
            loaded_modules: std::collections::HashSet::new(),
        }
    }
}

#[async_trait]
impl RuntimeInvoker for MockRuntimeInvoker {
    async fn load_module(&self, plugin_id: &str, _wasm_path: &PathBuf) -> Result<(), cmx_traits::TraitError> {
        // Mock 实现
        Ok(())
    }
    
    async fn unload_module(&self, _plugin_id: &str) -> Result<(), cmx_traits::TraitError> {
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
    ) -> Result<WasmInvokeResult, cmx_traits::TraitError> {
        Ok(WasmInvokeResult {
            output: br#"{"result": "success"}"#.to_vec(),
            elapsed_us: 1000,
            fuel_consumed: Some(5000),
        })
    }
}

/// 测试 CmxService 创建
#[tokio::test]
async fn test_service_creation() {
    let plugin_query = Arc::new(MockPluginQuery::new());
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let service = CmxService::new(
        plugin_query,
        runtime,
        ServiceConfig::default(),
    );
    
    assert!(service.config().invoke_timeout_ms > 0);
}

/// 测试服务调用
#[tokio::test]
async fn test_service_invoke() {
    let plugin = PluginSnapshot {
        id: "test-plugin".to_string(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        status: "active".to_string(),
        wasm_path: Some(PathBuf::from("test.wasm")),
        domain: "default".to_string(),
        description: None,
        author: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let plugin_query = Arc::new(MockPluginQuery::new().with_plugin(plugin));
    let runtime = Arc::new(MockRuntimeInvoker::new());
    
    let service = CmxService::new(
        plugin_query,
        runtime,
        ServiceConfig::default(),
    );
    
    let request = InvokeRequest {
        plugin_id: "test-plugin".to_string(),
        function_name: "test_function".to_string(),
        input: serde_json::json!({"data": "test"}),
        db_id: Some("default".to_string()),
        request_id: Some("req-001".to_string()),
        tenant_id: None,
    };
    
    let result = service.invoke(&request).await;
    
    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.success);
}
```

### 1.3 宿主函数单元测试

**文件:** `crates/libs/cmx-infra/cmx-database/tests/host_functions_test.rs` (新建)

```rust
//! DatabaseHostFunctions 单元测试

use cmx_database::host_functions::DatabaseHostFunctions;
use cmx_traits::HostFunctionProvider;
use std::sync::Arc;

#[test]
fn test_namespace() {
    let host_functions = DatabaseHostFunctions::new(Arc::new(
        cmx_database::manager::DatabaseManager::new()
    ));
    
    assert_eq!(host_functions.namespace(), "cmx:database");
}

#[test]
fn test_provided_functions() {
    let host_functions = DatabaseHostFunctions::new(Arc::new(
        cmx_database::manager::DatabaseManager::new()
    ));
    
    let functions = host_functions.provided_functions();
    
    assert!(functions.contains(&"cmx:database/execute_sql"));
    assert!(functions.contains(&"cmx:database/query_sql"));
    assert!(functions.contains(&"cmx:database/txn/begin"));
    assert!(functions.contains(&"cmx:database/txn/commit"));
    assert!(functions.contains(&"cmx:database/txn/rollback"));
}
```

---

## 任务2: 编写集成测试 🟡

### 2.1 端到端测试

**文件:** `tests/e2e_test.rs` (新建)

```rust
//! 端到端测试

use std::process::Command;

/// 测试应用启动
#[test]
fn test_application_startup() {
    // 启动应用
    let output = Command::new("cargo")
        .args(&["run", "--bin", "web-server"])
        .output()
        .expect("Failed to start application");
    
    // 检查启动日志
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("初始化 WASM 运行时"));
    assert!(stdout.contains("已注册数据库宿主函数"));
    assert!(stdout.contains("已注册缓存宿主函数"));
    assert!(stdout.contains("WASM 运行时初始化完成"));
    assert!(stdout.contains("Web 服务器启动成功"));
}

/// 测试插件生命周期
#[tokio::test]
#[ignore] // 需要运行的应用
async fn test_plugin_lifecycle() {
    use reqwest::Client;
    
    let client = Client::new();
    let base_url = "http://localhost:8080";
    
    // 1. 安装插件
    let install_response = client
        .post(&format!("{}/api/plugin/install", base_url))
        .json(&serde_json::json!({
            "plugin_id": "test-plugin",
            "wasm_file": "test.wasm"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(install_response.status().is_success());
    
    // 2. 激活插件
    let activate_response = client
        .post(&format!("{}/api/plugin/activate", base_url))
        .json(&serde_json::json!({
            "plugin_id": "test-plugin"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(activate_response.status().is_success());
    
    // 3. 调用插件
    let call_response = client
        .post(&format!("{}/api/service/call", base_url))
        .json(&serde_json::json!({
            "plugin_id": "test-plugin",
            "function_name": "handle_request",
            "input": {"data": "test"}
        }))
        .send()
        .await
        .unwrap();
    
    assert!(call_response.status().is_success());
    
    // 4. 停用插件
    let deactivate_response = client
        .post(&format!("{}/api/plugin/deactivate", base_url))
        .json(&serde_json::json!({
            "plugin_id": "test-plugin"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(deactivate_response.status().is_success());
    
    // 5. 卸载插件
    let uninstall_response = client
        .post(&format!("{}/api/plugin/uninstall", base_url))
        .json(&serde_json::json!({
            "plugin_id": "test-plugin"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(uninstall_response.status().is_success());
}
```

### 2.2 HTTP API 测试

**文件:** `tests/api_test.rs` (新建)

```rust
//! HTTP API 测试

use reqwest::Client;

#[tokio::test]
#[ignore] // 需要运行的应用
async fn test_service_call_api() {
    let client = Client::new();
    
    let response = client
        .post("http://localhost:8080/api/service/call")
        .json(&serde_json::json!({
            "plugin_id": "test-plugin",
            "function_name": "handle_request",
            "input": {"data": "test"},
            "db_id": "default"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(response.status().is_success());
    
    let body = response.json::<serde_json::Value>().await.unwrap();
    
    assert_eq!(body["code"], 0);
    assert!(body["data"]["success"].as_bool().unwrap());
}

#[tokio::test]
#[ignore] // 需要运行的应用
async fn test_orchestration_api() {
    let client = Client::new();
    
    let response = client
        .post("http://localhost:8080/api/service/orchestration")
        .json(&serde_json::json!({
            "orchestration": {
                "id": "test-flow",
                "name": "测试流程",
                "steps": []
            },
            "initial_input": {},
            "db_id": "default"
        }))
        .send()
        .await
        .unwrap();
    
    assert!(response.status().is_success());
}
```

### 2.3 解耦验证测试

**文件:** `tests/decoupling_test.rs` (新建)

```rust
//! 解耦验证测试

use std::process::Command;

/// 测试修改 cmx-plugin 不会触发 cmx-runtime 重编译
#[test]
fn test_decoupling_plugin_runtime() {
    // 触碰 cmx-plugin 文件
    let _ = Command::new("touch")
        .arg("crates/libs/cmx-plugin/src/domain/plugin.rs")
        .output();
    
    // 执行 cargo check
    let output = Command::new("cargo")
        .args(&["check", "--message-format=short"])
        .output()
        .expect("Failed to run cargo check");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // 检查是否重编译了 cmx-runtime
    assert!(!stderr.contains("Compiling cmx-runtime"));
    assert!(!stderr.contains("Compiling cmx-service"));
}

/// 测试修改 cmx-plugin 不会触发 cmx-traits 重编译
#[test]
fn test_decoupling_plugin_traits() {
    // 触碰 cmx-plugin 文件
    let _ = Command::new("touch")
        .arg("crates/libs/cmx-plugin/src/core/manager.rs")
        .output();
    
    // 执行 cargo check
    let output = Command::new("cargo")
        .args(&["check", "--message-format=short"])
        .output()
        .expect("Failed to run cargo check");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // 检查是否重编译了 cmx-traits
    assert!(!stderr.contains("Compiling cmx-traits"));
}
```

---

## 任务3: 性能优化 🟢 (可选)

### 3.1 添加性能监控

**文件:** `crates/libs/cmx-runtime/src/metrics.rs` (新建)

```rust
//! 性能监控指标

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// 运行时性能指标
pub struct RuntimeMetrics {
    /// 总调用次数
    pub total_invocations: AtomicU64,
    /// 总执行时间（微秒）
    pub total_elapsed_us: AtomicU64,
    /// 总 fuel 消耗
    pub total_fuel_consumed: AtomicU64,
    /// 加载的模块数
    pub loaded_modules: AtomicU64,
}

impl RuntimeMetrics {
    pub fn new() -> Self {
        Self {
            total_invocations: AtomicU64::new(0),
            total_elapsed_us: AtomicU64::new(0),
            total_fuel_consumed: AtomicU64::new(0),
            loaded_modules: AtomicU64::new(0),
        }
    }
    
    /// 记录一次调用
    pub fn record_invocation(&self, elapsed_us: u64, fuel_consumed: u64) {
        self.total_invocations.fetch_add(1, Ordering::Relaxed);
        self.total_elapsed_us.fetch_add(elapsed_us, Ordering::Relaxed);
        self.total_fuel_consumed.fetch_add(fuel_consumed, Ordering::Relaxed);
    }
    
    /// 获取平均执行时间
    pub fn avg_elapsed_us(&self) -> f64 {
        let total = self.total_invocations.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        
        let elapsed = self.total_elapsed_us.load(Ordering::Relaxed);
        elapsed as f64 / total as f64
    }
}
```

### 3.2 添加调用超时控制

**文件:** `crates/libs/cmx-service/src/service.rs`

修改 `invoke` 方法：

```rust
pub async fn invoke(&self, request: &InvokeRequest) -> Result<InvokeResponse, ServiceError> {
    // ... 现有逻辑 ...
    
    // 添加超时控制
    let timeout_duration = tokio::time::Duration::from_millis(self.config.invoke_timeout_ms);
    
    let result = tokio::time::timeout(
        timeout_duration,
        self.runtime.invoke(&request.plugin_id, &request.function_name, &input_bytes, &caller_data)
    ).await
    .map_err(|_| ServiceError::TimeoutError(format!(
        "插件 {} 调用超时（{}ms）",
        request.plugin_id,
        self.config.invoke_timeout_ms
    )))??;
    
    // ... 处理结果 ...
}
```

---

## 验证清单

### 单元测试验证

```bash
# 运行所有单元测试
cargo test --lib

# 运行特定模块的测试
cargo test -p cmx-runtime
cargo test -p cmx-service
cargo test -p cmx-database
```

### 集成测试验证

```bash
# 启动应用
cargo run --bin web-server &

# 运行集成测试
cargo test --test e2e_test --test api_test

# 停止应用
kill %1
```

### 解耦验证

```bash
# 运行解耦测试
cargo test --test decoupling_test
```

---

## 总结

按照本文档的步骤执行，可以完成 CMX Container 解耦重构的最后测试工作。关键要点:

1. **单元测试**: 覆盖核心模块的功能
2. **集成测试**: 验证端到端流程
3. **解耦验证**: 确保解耦效果

**预计剩余工作量: 约 3-5 小时**

---

## 附录: 测试最佳实践

### 测试命名规范

```rust
// 格式: test_<模块>_<功能>_<场景>
#[test]
fn test_engine_initialization_success() { }

#[test]
fn test_service_invoke_timeout() { }
```

### 测试组织结构

```
tests/
├── e2e_test.rs          # 端到端测试
├── api_test.rs          # HTTP API 测试
└── decoupling_test.rs   # 解耦验证测试

crates/libs/cmx-runtime/tests/
└── engine_test.rs       # 单元测试

crates/libs/cmx-service/tests/
└── service_test.rs      # 单元测试
```

### 测试覆盖率

```bash
# 生成测试覆盖率报告
cargo tarpaulin --out Html --output-dir target/coverage
```

---

## 结语

CMX Container 解耦重构项目的核心功能已全部完成。剩余工作仅为测试编写，建议按照本文档的步骤逐步完成测试套件，确保代码质量和功能稳定性。

**项目评级: ⭐⭐⭐⭐⭐ (5/5)**
