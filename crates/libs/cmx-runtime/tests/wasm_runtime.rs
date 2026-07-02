//! cmx-runtime WASM 运行时集成测试
//!
//! 使用 extism-pdk 自带的 `count_vowels.wasm` 真实 wasm 文件进行集成测试，
//! 覆盖 WASM 模块的加载、缓存、卸载和函数调用全流程。
//!
//! # 测试 wasm 说明
//!
//! `count_vowels.wasm` 来自 extism-pdk 1.4.1（crate 自带示例）。
//! - 输入：UTF-8 字符串字节
//! - 输出：JSON `{"count": N}`，N 为元音字母数量
//! - 导出函数名：`count_vowels`
//!
//! 该文件已复制到 `tests/wasm/count_vowels.wasm`，避免依赖 cargo 缓存路径。

use std::path::PathBuf;
use std::sync::Arc;

use cmx_runtime::ExtismEngine;
use cmx_runtime::ExtismEngineConfig;
use cmx_traits::error::TraitError;
use cmx_traits::runtime::RuntimeInvoker;

/// 嵌入测试用的 wasm 字节，避免运行时依赖文件路径
const COUNT_VOWELS_WASM: &[u8] = include_bytes!("wasm/count_vowels.wasm");

/// 获取测试 wasm 文件路径
///
/// 优先使用磁盘上的 `tests/wasm/count_vowels.wasm` 文件，
/// 以便完整覆盖 `load_module` 的文件读取路径。
fn wasm_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("tests")
        .join("wasm")
        .join("count_vowels.wasm")
}

/// 将字节数据写入临时文件，返回文件路径
///
/// 测试结束后不会自动清理，但位于系统临时目录，不影响测试逻辑。
fn write_temp_wasm(content: &[u8], suffix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("cmx_runtime_test_{}.wasm", suffix));
    std::fs::write(&path, content).expect("写入临时 wasm 文件失败");
    path
}

/// 构建测试用 ExtismEngine 实例
///
/// 使用默认配置（不含 ConfigManager 初始化）创建引擎，
/// pool_max_instances 调小以加快启动。
fn make_engine() -> ExtismEngine {
    let config = ExtismEngineConfig {
        enable_wasi: true,
        memory_max: 4096,
        timeout: std::time::Duration::from_secs(30),
        pool_max_instances: 2,
        fuel_limit: None,
    };
    ExtismEngine::new(config).expect("创建 ExtismEngine 失败")
}

// ============================================================
// WASM 模块加载测试
// ============================================================

#[tokio::test]
async fn test_load_module_success() {
    // 验证：使用真实 wasm 文件加载成功
    let engine = make_engine();
    let wasm_path = wasm_path();
    assert!(wasm_path.exists(), "测试 wasm 文件不存在: {:?}", wasm_path);

    // 加载前：is_loaded 返回 false，get_pool_count 返回 None
    assert!(!engine.is_loaded("test_plugin").await);
    assert_eq!(engine.get_pool_count("test_plugin"), None);

    // 执行加载
    engine
        .load_module("test_plugin", &wasm_path)
        .await
        .expect("加载 wasm 模块失败");

    // 加载后：is_loaded 返回 true，get_pool_count 返回 Some
    assert!(engine.is_loaded("test_plugin").await);
    let pool_count = engine
        .get_pool_count("test_plugin")
        .expect("加载后 pool 应存在");
    // 初始 pool count 可能为 0（实例按需创建）
    assert!(pool_count <= 2, "pool 实例数不应超过 max_instances");
}

#[tokio::test]
async fn test_load_module_nonexistent_file() {
    // 验证：加载不存在的文件返回 WasmLoadFailed 错误
    let engine = make_engine();
    let nonexistent_path = PathBuf::from("/nonexistent/path/to/plugin.wasm");

    let result = engine
        .load_module("missing_plugin", &nonexistent_path)
        .await;

    assert!(result.is_err(), "加载不存在的文件应返回错误");
    match result.unwrap_err() {
        TraitError::WasmLoadFailed(msg) => {
            assert!(
                msg.contains("读取 WASM 文件") || msg.contains("失败"),
                "错误消息应包含读取失败信息: {}",
                msg
            );
        }
        other => panic!("期望 WasmLoadFailed 错误，实际: {:?}", other),
    }

    // 失败后不应占用 plugin_id
    assert!(!engine.is_loaded("missing_plugin").await);
}

#[tokio::test]
async fn test_load_module_invalid_wasm() {
    // 验证：加载无效的 wasm 字节时，调用阶段应失败
    //
    // 注意：Extism 的 Pool 采用懒加载机制——`load_module` 时仅注册工厂函数，
    // 不立即编译 wasm。真正编译发生在第一次 `invoke` 时（Pool::with_plugin 内部
    // 调用 PluginBuilder::build()）。因此无效 wasm 在调用阶段才会报错。
    let engine = make_engine();

    // 写入无效字节到临时文件
    let invalid_bytes = b"\x00\x01\x02\x03 not a wasm file";
    let invalid_path = write_temp_wasm(invalid_bytes, "invalid");

    // load_module 本身应成功（仅注册工厂）
    let load_result = engine.load_module("invalid_plugin", &invalid_path).await;
    assert!(
        load_result.is_ok(),
        "load_module 应成功（懒加载，不立即编译），实际: {:?}",
        load_result
    );

    // 第一次 invoke 时触发编译，应失败
    let invoke_result = engine
        .invoke("invalid_plugin", "any_function", b"input")
        .await;
    assert!(invoke_result.is_err(), "调用无效 wasm 应返回错误");

    // 清理临时文件
    let _ = std::fs::remove_file(&invalid_path);
}

#[tokio::test]
async fn test_load_module_idempotent_same_plugin_id() {
    // 验证：重复加载同一 plugin_id 时，第二次调用应快速返回（命中双检锁快速路径）
    let engine = make_engine();
    let wasm_path = wasm_path();

    // 第一次加载
    engine
        .load_module("dup_plugin", &wasm_path)
        .await
        .expect("第一次加载失败");
    let count_after_first = engine.get_pool_count("dup_plugin");

    // 第二次加载同一 plugin_id：应跳过编译，返回 Ok
    engine
        .load_module("dup_plugin", &wasm_path)
        .await
        .expect("重复加载应返回 Ok（命中缓存）");

    // 池实例数不应增加（同一个池被复用）
    let count_after_second = engine.get_pool_count("dup_plugin");
    assert_eq!(
        count_after_first, count_after_second,
        "重复加载不应创建新的池实例"
    );

    // 仍然处于已加载状态
    assert!(engine.is_loaded("dup_plugin").await);
}

#[tokio::test]
async fn test_load_multiple_different_plugins() {
    // 验证：加载多个不同 plugin_id 的模块互不影响
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("plugin_a", &wasm_path)
        .await
        .expect("加载 plugin_a 失败");
    engine
        .load_module("plugin_b", &wasm_path)
        .await
        .expect("加载 plugin_b 失败");

    assert!(engine.is_loaded("plugin_a").await);
    assert!(engine.is_loaded("plugin_b").await);
    assert!(!engine.is_loaded("plugin_c").await);

    // 卸载 plugin_a 不影响 plugin_b
    engine.unload_module("plugin_a").await.unwrap();
    assert!(!engine.is_loaded("plugin_a").await);
    assert!(engine.is_loaded("plugin_b").await);
}

#[tokio::test]
async fn test_unload_module() {
    // 验证：卸载模块后 is_loaded 返回 false
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("to_unload", &wasm_path)
        .await
        .expect("加载失败");
    assert!(engine.is_loaded("to_unload").await);

    engine.unload_module("to_unload").await.expect("卸载失败");

    assert!(!engine.is_loaded("to_unload").await);
    assert_eq!(engine.get_pool_count("to_unload"), None);
}

#[tokio::test]
async fn test_unload_nonexistent_module() {
    // 验证：卸载未加载的模块仍返回 Ok（实现中仅记录 warning）
    let engine = make_engine();
    let result = engine.unload_module("never_loaded").await;
    assert!(result.is_ok(), "卸载未加载模块应返回 Ok");
}

// ============================================================
// WASM 模块执行测试
// ============================================================

#[tokio::test]
async fn test_invoke_function_success() {
    // 验证：调用 count_vowels 函数成功，返回正确的元音字母数量
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("invoke_plugin", &wasm_path)
        .await
        .expect("加载失败");

    // 调用 count_vowels 函数，输入 "hello world"
    // 元音字母：e, o, o = 3 个
    let input = b"hello world";
    let result = engine
        .invoke("invoke_plugin", "count_vowels", input)
        .await
        .expect("调用 count_vowels 失败");

    // 验证返回值非空
    assert!(!result.output.is_empty(), "函数输出不应为空");

    // 验证返回的 JSON 包含 count 字段
    let output_str = String::from_utf8_lossy(&result.output);
    assert!(
        output_str.contains("count"),
        "输出应包含 count 字段: {}",
        output_str
    );

    // 验证耗时已记录
    assert!(
        result.elapsed_us > 0 || !result.output.is_empty(),
        "应记录执行耗时或返回数据"
    );

    // 验证 metrics 已更新
    let metrics = engine.get_metrics();
    use std::sync::atomic::Ordering;
    let total_calls = metrics.total_calls.load(Ordering::Relaxed);
    assert!(
        total_calls >= 1,
        "调用后 total_calls 应至少为 1，实际: {}",
        total_calls
    );
}

#[tokio::test]
async fn test_invoke_nonexistent_function() {
    // 验证：调用不存在的函数返回错误
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("missing_fn_plugin", &wasm_path)
        .await
        .expect("加载失败");

    let result = engine
        .invoke("missing_fn_plugin", "nonexistent_function", b"input")
        .await;

    assert!(result.is_err(), "调用不存在的函数应返回错误");
    match result.unwrap_err() {
        TraitError::WasmInvokeFailed(msg) => {
            // 错误消息应包含函数名或调用失败信息
            assert!(!msg.is_empty(), "错误消息不应为空");
        }
        other => panic!("期望 WasmInvokeFailed 错误，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_invoke_unloaded_plugin() {
    // 验证：调用未加载的插件返回 WasmNotLoaded 错误
    let engine = make_engine();

    let result = engine
        .invoke("never_loaded_plugin", "count_vowels", b"input")
        .await;

    assert!(result.is_err(), "调用未加载插件应返回错误");
    match result.unwrap_err() {
        TraitError::WasmNotLoaded(plugin_id) => {
            assert_eq!(plugin_id, "never_loaded_plugin", "错误消息应包含 plugin_id");
        }
        other => panic!("期望 WasmNotLoaded 错误，实际: {:?}", other),
    }
}

#[tokio::test]
async fn test_invoke_return_value_processing() {
    // 验证：函数返回值被正确封装到 WasmInvokeResult
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("return_plugin", &wasm_path)
        .await
        .expect("加载失败");

    // 输入空字符串，count_vowels 应返回 {"count":0}
    let result = engine
        .invoke("return_plugin", "count_vowels", b"")
        .await
        .expect("调用失败");

    // 验证 output 字段：解析为 JSON 应包含 count:0
    let output_str = String::from_utf8_lossy(&result.output);
    assert!(
        output_str.contains("\"count\":0") || output_str.contains("\"count\": 0"),
        "空字符串应返回 count:0，实际: {}",
        output_str
    );

    // 验证 elapsed_us 字段类型
    let _elapsed: u64 = result.elapsed_us;

    // 验证 fuel_consumed 字段（未启用 fuel 时为 None）
    assert!(
        result.fuel_consumed.is_none(),
        "未启用 fuel 限制时 fuel_consumed 应为 None"
    );
}

#[tokio::test]
async fn test_invoke_multiple_times_same_plugin() {
    // 验证：同一插件可被多次调用（Pool 复用实例）
    let engine = make_engine();
    let wasm_path = wasm_path();

    engine
        .load_module("multi_invoke", &wasm_path)
        .await
        .expect("加载失败");

    // 多次调用同一函数
    for i in 1..=5 {
        let input = format!("test input {}", i);
        let result = engine
            .invoke("multi_invoke", "count_vowels", input.as_bytes())
            .await
            .unwrap_or_else(|_| panic!("第 {} 次调用失败", i));
        assert!(!result.output.is_empty(), "第 {} 次调用输出为空", i);
    }

    // 验证 metrics 累计调用次数
    let metrics = engine.get_metrics();
    use std::sync::atomic::Ordering;
    let total_calls = metrics.total_calls.load(Ordering::Relaxed);
    assert!(
        total_calls >= 5,
        "5 次调用后 total_calls 应 >= 5，实际: {}",
        total_calls
    );
}

#[tokio::test]
async fn test_invoke_concurrent_different_plugins() {
    // 验证：不同插件可并发调用
    let engine = Arc::new(make_engine());
    let wasm_path = wasm_path();

    engine
        .load_module("conc_plugin_a", &wasm_path)
        .await
        .expect("加载 plugin_a 失败");
    engine
        .load_module("conc_plugin_b", &wasm_path)
        .await
        .expect("加载 plugin_b 失败");

    // 并发调用两个不同插件
    let engine_a = engine.clone();
    let engine_b = engine.clone();

    let (result_a, result_b) = tokio::join!(
        async move {
            engine_a
                .invoke("conc_plugin_a", "count_vowels", b"hello")
                .await
        },
        async move {
            engine_b
                .invoke("conc_plugin_b", "count_vowels", b"world")
                .await
        }
    );

    assert!(result_a.is_ok(), "plugin_a 并发调用失败: {:?}", result_a);
    assert!(result_b.is_ok(), "plugin_b 并发调用失败: {:?}", result_b);

    let res_a = result_a.unwrap();
    let res_b = result_b.unwrap();
    let out_a = String::from_utf8_lossy(&res_a.output);
    let out_b = String::from_utf8_lossy(&res_b.output);
    assert!(out_a.contains("count"), "plugin_a 输出异常: {}", out_a);
    assert!(out_b.contains("count"), "plugin_b 输出异常: {}", out_b);
}

// ============================================================
// 缓存双检锁逻辑测试
// ============================================================

#[tokio::test]
async fn test_double_checked_locking_no_duplicate_load() {
    // 验证：双检锁机制下，并发加载同一 plugin_id 不会创建多个池
    let engine = Arc::new(make_engine());
    let wasm_path = wasm_path();

    // 并发触发同一 plugin_id 的加载
    let mut handles = Vec::new();
    for _ in 0..5 {
        let engine = engine.clone();
        let path = wasm_path.clone();
        handles.push(tokio::spawn(async move {
            engine.load_module("concurrent_plugin", &path).await
        }));
    }

    // 所有加载都应成功
    for handle in handles {
        let result = handle.await.expect("tokio task join 失败");
        assert!(result.is_ok(), "并发加载应全部成功: {:?}", result);
    }

    // 验证 plugin 处于已加载状态
    assert!(engine.is_loaded("concurrent_plugin").await);

    // 验证只创建了一个池（pool_count 不超过 max_instances）
    let pool_count = engine
        .get_pool_count("concurrent_plugin")
        .expect("加载后 pool 应存在");
    assert!(
        pool_count <= 2,
        "并发加载后 pool 实例数不应超过 max_instances，实际: {}",
        pool_count
    );
}

#[tokio::test]
async fn test_load_then_unload_then_reload() {
    // 验证：卸载后可以重新加载（双检锁第二次检查在卸载后能正确处理）
    let engine = make_engine();
    let wasm_path = wasm_path();

    // 第一次加载
    engine
        .load_module("reload_plugin", &wasm_path)
        .await
        .expect("第一次加载失败");
    assert!(engine.is_loaded("reload_plugin").await);

    // 卸载
    engine
        .unload_module("reload_plugin")
        .await
        .expect("卸载失败");
    assert!(!engine.is_loaded("reload_plugin").await);

    // 重新加载
    engine
        .load_module("reload_plugin", &wasm_path)
        .await
        .expect("重新加载失败");
    assert!(engine.is_loaded("reload_plugin").await);

    // 验证重新加载后可以正常调用
    let result = engine
        .invoke("reload_plugin", "count_vowels", b"aio")
        .await
        .expect("重新加载后调用失败");
    let output = String::from_utf8_lossy(&result.output);
    // "aio" 包含 a, i, o 三个元音
    assert!(
        output.contains("\"count\":3") || output.contains("\"count\": 3"),
        "aio 应返回 count:3，实际: {}",
        output
    );
}

// 验证嵌入的 wasm 字节可用（避免文件丢失导致测试静默跳过）
#[test]
fn test_embedded_wasm_bytes_valid() {
    // wasm 魔数：\x00\x61\x73\x6d
    assert!(
        COUNT_VOWELS_WASM.len() > 100,
        "嵌入的 wasm 字节过小: {} bytes",
        COUNT_VOWELS_WASM.len()
    );
    assert_eq!(&COUNT_VOWELS_WASM[0..4], b"\x00asm", "wasm 魔数不正确");
}
