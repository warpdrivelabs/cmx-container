//! cmx-runtime 纯逻辑单元测试
//!
//! 不依赖真实 wasm 文件，覆盖可独立测试的纯逻辑：
//! - EngineMetrics 指标记录
//! - ExtismEngineConfig 默认配置
//! - ExtismError Display 实现
//! - InvokeContext 深度限制和循环检测
//! - HostFunctionProvider 注册逻辑
//! - GlobalExtismEngine 初始化状态检查

use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmx_runtime::EngineMetrics;
use cmx_runtime::ExtismEngine;
use cmx_runtime::ExtismEngineConfig;
use cmx_runtime::ExtismError;
use cmx_traits::error::{HostFuncError, TraitError};
use cmx_traits::runtime::{
    DEFAULT_MAX_DEPTH, DEFAULT_TIMEOUT, HostFunctionDef, HostFunctionProvider, InvokeContext,
    InvokeOptions, ValType,
};

// ============================================================
// EngineMetrics 指标测试
// ============================================================

#[test]
fn test_metrics_initial_state() {
    // 验证：新建 metrics 所有计数器为 0
    let metrics = EngineMetrics::new();
    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 0);
}

#[test]
fn test_metrics_record_success() {
    // 验证：record_success 累加 total_calls 和 total_elapsed_us，不增加 failed/timeout
    let metrics = EngineMetrics::new();
    metrics.record_success(100);
    metrics.record_success(200);

    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 300);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn test_metrics_record_failure() {
    // 验证：record_failure 累加 total_calls、failed_calls 和 total_elapsed_us
    let metrics = EngineMetrics::new();
    metrics.record_failure(50);
    metrics.record_failure(150);

    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 200);
}

#[test]
fn test_metrics_record_timeout() {
    // 验证：record_timeout 累加 total_calls、timeout_calls 和 total_elapsed_us
    let metrics = EngineMetrics::new();
    metrics.record_timeout(1000);

    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 1000);
}

#[test]
fn test_metrics_mixed_records() {
    // 验证：混合记录场景下计数器独立累加
    let metrics = EngineMetrics::new();
    metrics.record_success(10);
    metrics.record_failure(20);
    metrics.record_timeout(30);
    metrics.record_success(40);

    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 4);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 100);
}

#[test]
fn test_metrics_concurrent_increment() {
    // 验证：多线程并发记录的原子性
    use std::thread;
    let metrics = Arc::new(EngineMetrics::new());
    let mut handles = Vec::new();

    for _ in 0..10 {
        let m = metrics.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                m.record_success(1);
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join 失败");
    }

    assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 1000);
    assert_eq!(metrics.total_elapsed_us.load(Ordering::Relaxed), 1000);
    assert_eq!(metrics.failed_calls.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.timeout_calls.load(Ordering::Relaxed), 0);
}

// ============================================================
// ExtismEngineConfig 配置测试
// ============================================================

#[test]
fn test_config_default_values() {
    // 验证：ExtismEngineConfig::default() 提供合理的默认值
    let config = ExtismEngineConfig::default();

    assert!(config.enable_wasi, "默认应启用 WASI");
    assert_eq!(config.memory_max, 4096, "默认内存应为 4096 页");
    assert_eq!(
        config.timeout, DEFAULT_TIMEOUT,
        "默认超时应为 DEFAULT_TIMEOUT"
    );
    assert!(
        config.pool_max_instances >= 1,
        "默认 pool_max_instances 应 >= 1，实际: {}",
        config.pool_max_instances
    );
    assert!(
        config.fuel_limit.is_none(),
        "默认 fuel_limit 应为 None（不限制）"
    );
}

#[test]
fn test_config_custom_values() {
    // 验证：可以构建自定义配置
    let config = ExtismEngineConfig {
        enable_wasi: false,
        memory_max: 2048,
        timeout: std::time::Duration::from_secs(60),
        pool_max_instances: 16,
        fuel_limit: Some(1_000_000),
    };

    assert!(!config.enable_wasi);
    assert_eq!(config.memory_max, 2048);
    assert_eq!(config.timeout, std::time::Duration::from_secs(60));
    assert_eq!(config.pool_max_instances, 16);
    assert_eq!(config.fuel_limit, Some(1_000_000));
}

#[test]
fn test_config_clone_preserves_values() {
    // 验证：Clone 实现正确保留所有字段
    let config = ExtismEngineConfig {
        enable_wasi: true,
        memory_max: 1024,
        timeout: std::time::Duration::from_secs(15),
        pool_max_instances: 4,
        fuel_limit: Some(500_000),
    };
    let cloned = config.clone();

    assert_eq!(cloned.enable_wasi, config.enable_wasi);
    assert_eq!(cloned.memory_max, config.memory_max);
    assert_eq!(cloned.timeout, config.timeout);
    assert_eq!(cloned.pool_max_instances, config.pool_max_instances);
    assert_eq!(cloned.fuel_limit, config.fuel_limit);
}

// ============================================================
// ExtismError 错误类型测试
// ============================================================

#[test]
fn test_extism_error_display() {
    // 验证：每个错误变体的 Display 实现包含期望信息
    let load_err = ExtismError::PluginLoadFailed("wasm 编译失败".to_string());
    assert!(
        format!("{}", load_err).contains("插件加载失败"),
        "PluginLoadFailed Display 异常: {}",
        load_err
    );
    assert!(
        format!("{}", load_err).contains("wasm 编译失败"),
        "错误消息应保留原始信息"
    );

    let call_err = ExtismError::PluginCallFailed("调用超时".to_string());
    assert!(
        format!("{}", call_err).contains("插件调用失败"),
        "PluginCallFailed Display 异常: {}",
        call_err
    );

    let config_err = ExtismError::ConfigError("无效参数".to_string());
    assert!(
        format!("{}", config_err).contains("配置错误"),
        "ConfigError Display 异常: {}",
        config_err
    );

    let internal_err = ExtismError::InternalError("未知错误".to_string());
    assert!(
        format!("{}", internal_err).contains("内部错误"),
        "InternalError Display 异常: {}",
        internal_err
    );
}

#[test]
fn test_extism_error_clone() {
    // 验证：ExtismError 支持 Clone
    let err = ExtismError::PluginLoadFailed("test error".to_string());
    let cloned = err.clone();
    assert_eq!(format!("{}", err), format!("{}", cloned));
}

#[test]
fn test_trait_error_variants() {
    // 验证：TraitError 关键变体的 Display 实现
    let load_failed = TraitError::WasmLoadFailed("路径不存在".to_string());
    assert!(format!("{}", load_failed).contains("WASM 模块加载失败"));

    let invoke_failed = TraitError::WasmInvokeFailed("函数不存在".to_string());
    assert!(format!("{}", invoke_failed).contains("WASM 函数调用失败"));

    let not_loaded = TraitError::WasmNotLoaded("plugin_x".to_string());
    assert!(format!("{}", not_loaded).contains("WASM 模块未加载"));
    assert!(format!("{}", not_loaded).contains("plugin_x"));
}

// ============================================================
// InvokeContext 深度限制和循环检测测试
// ============================================================

#[test]
fn test_invoke_options_default() {
    // 验证：InvokeOptions 默认值
    let opts = InvokeOptions::default();
    assert_eq!(opts.timeout, DEFAULT_TIMEOUT);
    assert_eq!(opts.max_depth, DEFAULT_MAX_DEPTH);
    assert!(!opts.debug);
}

#[test]
fn test_invoke_options_builder() {
    // 验证：InvokeOptions Builder 模式
    let opts = InvokeOptions::new()
        .with_timeout(std::time::Duration::from_secs(10))
        .with_max_depth(4);

    assert_eq!(opts.timeout, std::time::Duration::from_secs(10));
    assert_eq!(opts.max_depth, 4);
}

#[test]
fn test_invoke_context_initial_depth() {
    // 验证：InvokeContext 初始深度为 0（在同一线程内可能受其他测试影响，使用范围限定）
    // 注意：thread_local 状态，先重置到已知状态
    // 由于 CALL_DEPTH 是 thread_local，不同测试运行顺序可能影响此值
    // 这里只验证 enter/drop 后回到初始深度
    let initial = InvokeContext::current_depth();
    let guard = InvokeContext::enter("test_plugin", "test_fn", 8).expect("enter 失败");
    assert_eq!(InvokeContext::current_depth(), initial + 1);
    assert_eq!(guard.depth(), initial + 1);
    drop(guard);
    assert_eq!(InvokeContext::current_depth(), initial);
}

#[test]
fn test_invoke_context_depth_exceeded() {
    // 验证：深度超限时 enter 返回 DepthExceeded 错误
    // 先 enter 到 max_depth
    let mut guards = Vec::new();
    for _ in 0..DEFAULT_MAX_DEPTH {
        let guard = InvokeContext::enter("depth_plugin", "fn", DEFAULT_MAX_DEPTH);
        match guard {
            Ok(g) => guards.push(g),
            Err(_) => break,
        }
    }

    // 再 enter 一次应失败
    let result = InvokeContext::enter("depth_plugin", "fn", DEFAULT_MAX_DEPTH);
    assert!(result.is_err(), "深度超限应返回错误");

    // 显式释放所有 guard
    guards.clear();
}

#[test]
fn test_invoke_context_cycle_detection() {
    // 验证：同一 plugin_id/function_name 重复 enter 触发循环检测
    let plugin_id = "cycle_plugin";
    let func_name = "cycle_fn";

    // 第一次 enter 应成功
    let guard1 = InvokeContext::enter(plugin_id, func_name, 8);
    assert!(guard1.is_ok(), "第一次 enter 应成功");

    // 第二次 enter 同一 plugin_id/function_name 应触发循环检测
    let guard2 = InvokeContext::enter(plugin_id, func_name, 8);
    assert!(
        guard2.is_err(),
        "同一 plugin_id/function_name 重复 enter 应触发循环检测"
    );

    // 释放 guard1 后可以再次 enter
    drop(guard1);
    let guard3 = InvokeContext::enter(plugin_id, func_name, 8);
    assert!(guard3.is_ok(), "释放后应能再次 enter");
}

#[test]
fn test_invoke_context_different_functions_no_cycle() {
    // 验证：不同函数不触发循环检测
    let guard1 = InvokeContext::enter("plugin_a", "fn1", 8).expect("fn1 enter 失败");
    let guard2 = InvokeContext::enter("plugin_a", "fn2", 8).expect("fn2 enter 失败");
    let guard3 = InvokeContext::enter("plugin_b", "fn1", 8).expect("plugin_b fn1 enter 失败");

    // 不同函数可同时存在
    assert_eq!(InvokeContext::current_depth(), 3);

    drop(guard1);
    drop(guard2);
    drop(guard3);
}

#[test]
fn test_invoke_context_is_cycle() {
    // 验证：is_cycle 检测函数
    let plugin_id = "iscycle_plugin";
    let func_name = "iscycle_fn";

    // 进入前：is_cycle 返回 false
    assert!(!InvokeContext::is_cycle(plugin_id, func_name));

    let _guard = InvokeContext::enter(plugin_id, func_name, 8).expect("enter 失败");

    // 进入后：is_cycle 返回 true
    assert!(InvokeContext::is_cycle(plugin_id, func_name));

    // 不同函数不报告循环
    assert!(!InvokeContext::is_cycle(plugin_id, "other_fn"));
    assert!(!InvokeContext::is_cycle("other_plugin", func_name));
}

// ============================================================
// HostFunctionDef 与 HostFunctionProvider 测试
// ============================================================

#[test]
fn test_host_function_def_constructors() {
    // 验证：HostFunctionDef 各种构造器
    let msgpack_fn = HostFunctionDef::msgpack_fn("test_fn", "cmx:test");
    assert_eq!(msgpack_fn.name, "test_fn");
    assert_eq!(msgpack_fn.namespace, "cmx:test");
    assert_eq!(msgpack_fn.input_types.len(), 1);
    assert_eq!(msgpack_fn.output_types.len(), 1);
    assert_eq!(msgpack_fn.input_types[0], ValType::Ptr);
    assert_eq!(msgpack_fn.output_types[0], ValType::Ptr);

    let no_input_fn = HostFunctionDef::no_input("no_input_fn", "cmx:test", &[ValType::Ptr]);
    assert_eq!(no_input_fn.input_types.len(), 0);
    assert_eq!(no_input_fn.output_types.len(), 1);

    let no_output_fn = HostFunctionDef::no_output("no_output_fn", "cmx:test", &[ValType::Ptr]);
    assert_eq!(no_output_fn.input_types.len(), 1);
    assert_eq!(no_output_fn.output_types.len(), 0);

    let void_fn = HostFunctionDef::void_fn("void_fn", "cmx:test", &[ValType::I64]);
    assert_eq!(void_fn.input_types.len(), 1);
    assert_eq!(void_fn.input_types[0], ValType::I64);
    assert_eq!(void_fn.output_types.len(), 0);
}

#[test]
fn test_val_type_to_extism() {
    // 验证：ValType 到 extism::ValType 的映射
    assert_eq!(ValType::I32.to_extism(), extism::ValType::I32);
    assert_eq!(ValType::I64.to_extism(), extism::ValType::I64);
    assert_eq!(ValType::F32.to_extism(), extism::ValType::F32);
    assert_eq!(ValType::F64.to_extism(), extism::ValType::F64);
    // Ptr 映射到 I64
    assert_eq!(ValType::Ptr.to_extism(), extism::ValType::I64);
}

#[test]
fn test_host_function_def_new() {
    // 验证：HostFunctionDef::new 显式构造
    let def = HostFunctionDef::new(
        "explicit_fn",
        "cmx:explicit",
        &[ValType::I32, ValType::I64],
        &[ValType::F64],
    );
    assert_eq!(def.name, "explicit_fn");
    assert_eq!(def.namespace, "cmx:explicit");
    assert_eq!(def.input_types.len(), 2);
    assert_eq!(def.output_types.len(), 1);
}

// ============================================================
// MockHostFunctionProvider 用于 register_provider 测试
// ============================================================

/// 用于测试的 Mock 宿主函数提供者
struct MockHostFunctionProvider {
    namespace: &'static str,
    call_count: Arc<std::sync::atomic::AtomicU32>,
}

impl MockHostFunctionProvider {
    fn new(namespace: &'static str) -> (Self, Arc<std::sync::atomic::AtomicU32>) {
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let provider = Self {
            namespace,
            call_count: count.clone(),
        };
        (provider, count)
    }
}

impl HostFunctionProvider for MockHostFunctionProvider {
    fn namespace(&self) -> &str {
        self.namespace
    }

    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![
            HostFunctionDef::msgpack_fn("mock_fn_1", self.namespace),
            HostFunctionDef::msgpack_fn("mock_fn_2", self.namespace),
        ]
    }

    fn call(&self, name: &str, _input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match name {
            "mock_fn_1" => Ok(b"result_1".to_vec()),
            "mock_fn_2" => Ok(b"result_2".to_vec()),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }
}

#[tokio::test]
async fn test_engine_register_provider() {
    // 验证：通过 ExtismEngine::register_provider 注册宿主函数后，
    //       cached_function_count 反映注册的函数数量
    let engine = ExtismEngine::new(ExtismEngineConfig::default()).expect("创建 engine 失败");

    // 注册前：缓存函数数为 0
    assert_eq!(engine.cached_function_count(), 0);

    let (provider, _call_count) = MockHostFunctionProvider::new("cmx:mock");
    engine
        .register_provider(Arc::new(provider))
        .expect("register_provider 失败");

    // 注册后：缓存函数数为 2（mock_fn_1 + mock_fn_2）
    assert_eq!(
        engine.cached_function_count(),
        2,
        "注册后 cached_function_count 应为 2"
    );
}

#[tokio::test]
async fn test_engine_register_multiple_providers() {
    // 验证：注册多个 provider 时函数累加
    let engine = ExtismEngine::new(ExtismEngineConfig::default()).expect("创建 engine 失败");

    let (provider_a, _) = MockHostFunctionProvider::new("cmx:mock_a");
    let (provider_b, _) = MockHostFunctionProvider::new("cmx:mock_b");

    engine
        .register_provider(Arc::new(provider_a))
        .expect("注册 provider_a 失败");
    assert_eq!(engine.cached_function_count(), 2);

    engine
        .register_provider(Arc::new(provider_b))
        .expect("注册 provider_b 失败");
    assert_eq!(engine.cached_function_count(), 4);
}

#[tokio::test]
async fn test_engine_get_metrics_returns_shared_arc() {
    // 验证：get_metrics 返回的 Arc 与引擎内部 metrics 共享引用
    let engine = ExtismEngine::new(ExtismEngineConfig::default()).expect("创建 engine 失败");
    let metrics1 = engine.get_metrics();
    let metrics2 = engine.get_metrics();

    // 两个 Arc 指向同一份数据
    assert!(
        Arc::ptr_eq(&metrics1, &metrics2),
        "get_metrics 应返回同一 Arc 引用"
    );

    // 通过其中一个引用修改后，另一个应可见
    metrics1.record_success(100);
    assert_eq!(metrics2.total_calls.load(Ordering::Relaxed), 1);
}

// ============================================================
// GlobalExtismEngine 状态检查（不调用 initialize 避免污染全局）
// ============================================================

#[test]
fn test_global_engine_initialization_check() {
    // 验证：GlobalExtismEngine::is_initialized 返回 bool
    // 注意：不调用 initialize()，因为 OnceLock 一旦设置无法重置，
    //       会影响其他测试。这里仅验证 is_initialized 方法可调用且返回 bool。
    let _initialized: bool = cmx_runtime::GlobalExtismEngine::is_initialized();
    // 不对具体值做断言，因为测试运行顺序可能影响全局状态
}

// ============================================================
// HostFuncError 工具方法测试
// ============================================================

#[test]
fn test_host_func_error_helpers() {
    // 验证：HostFuncError 工厂方法
    let reg_err = HostFuncError::registration_failed("ns", "fn", "原因");
    match &reg_err {
        HostFuncError::RegistrationFailed {
            namespace,
            name,
            reason,
        } => {
            assert_eq!(namespace, "ns");
            assert_eq!(name, "fn");
            assert_eq!(reason, "原因");
        }
        _ => panic!("期望 RegistrationFailed"),
    }
    assert!(format!("{}", reg_err).contains("函数注册失败"));
    assert!(format!("{}", reg_err).contains("ns/fn"));

    let exec_err = HostFuncError::execution_failed("ns", "fn", "执行错误");
    match &exec_err {
        HostFuncError::ExecutionFailed {
            namespace,
            name,
            reason,
        } => {
            assert_eq!(namespace, "ns");
            assert_eq!(name, "fn");
            assert_eq!(reason, "执行错误");
        }
        _ => panic!("期望 ExecutionFailed"),
    }
    assert!(format!("{}", exec_err).contains("函数执行失败"));

    let invalid_fn = HostFuncError::invalid_function("unknown_fn");
    match &invalid_fn {
        HostFuncError::ExecutionFailed { name, reason, .. } => {
            assert_eq!(name, "unknown_fn");
            assert_eq!(reason, "函数不存在");
        }
        _ => panic!("期望 ExecutionFailed"),
    }
}
