//! WasmEngine 单元测试
//!
//! 测试 WASM 运行时引擎的核心功能。

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

/// 测试引擎默认配置初始化
#[test]
fn test_engine_default_config() {
    let config = WasmEngineConfig::default();
    let engine = WasmEngine::new(config);
    
    assert!(engine.is_ok());
}

/// 测试注册宿主函数提供者
#[test]
fn test_register_provider() {
    let config = WasmEngineConfig::default();
    let mut engine = WasmEngine::new(config).unwrap();
    
    /// 测试用宿主函数提供者
    struct TestProvider;
    
    impl HostFunctionProvider for TestProvider {
        fn namespace(&self) -> &str {
            "test"
        }
        
        fn register_functions(&self, _linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
            Ok(())
        }
        
        fn provided_functions(&self) -> Vec<&str> {
            vec!["test/func"]
        }
    }
    
    engine.register_provider(Box::new(TestProvider));
}

/// 测试全局引擎单例
#[test]
fn test_global_engine_singleton() {
    // 清理之前的状态（如果存在）
    // 注意：OnceLock 不支持重置，所以这个测试只能运行一次
    
    // 检查是否已初始化
    let was_initialized = GlobalWasmEngine::is_initialized();
    
    if !was_initialized {
        let config = WasmEngineConfig::default();
        let result = GlobalWasmEngine::initialize(config);
        assert!(result.is_ok());
    }
    
    // 验证已初始化
    assert!(GlobalWasmEngine::is_initialized());
}

/// 测试获取 invoker 适配器
#[test]
fn test_get_as_invoker() {
    // 确保已初始化
    if !GlobalWasmEngine::is_initialized() {
        let config = WasmEngineConfig::default();
        GlobalWasmEngine::initialize(config).ok();
    }
    
    let _invoker = GlobalWasmEngine::get_as_invoker();
    
    // 验证 invoker 创建成功
    // RuntimeInvoker trait 方法可以正常调用
}

/// 测试引擎配置
#[test]
fn test_engine_config() {
    let config = WasmEngineConfig {
        max_memory_bytes: 512 * 1024 * 1024,
        enable_fuel: false,
        max_fuel: 0,
        enable_wasi: true,
    };
    
    assert_eq!(config.max_memory_bytes, 512 * 1024 * 1024);
    assert!(!config.enable_fuel);
    assert!(config.enable_wasi);
}
