//! 宿主函数桥接层
//!
//! 将 `HostFunctionProvider` 的业务逻辑函数包装为 Extism 宿主函数，
//! 使 WASM 插件能够通过 Extism PDK 调用宿主端的数据库、缓存、日志等服务。
//!
//! # 架构
//!
//! ```text
//! WASM 插件 (PDK)
//!   │  调用 host_func_name(input: i64) -> i64
//!   ▼
//! Extism 运行时
//!   │  拦截宿主函数调用，读取/写入 Extism 线性内存
//!   ▼
//! host_function_wrapper (本模块)
//!   │  解码输入 → 调用 provider.call() → 编码输出
//!   ▼
//! HostFunctionProvider (业务逻辑)
//! ```

use std::sync::Arc;

use cmx_traits::HostFunctionProvider;
use extism::{CurrentPlugin, Function, UserData, ValType};

use crate::error::ExtismError;

/// 宿主函数调用上下文
///
/// 封装了宿主函数提供者和函数名，作为 `UserData` 携带在 Extism 宿主函数中。
/// 当 WASM 插件调用宿主函数时，`host_function_wrapper` 通过此上下文
/// 定位并调用正确的业务逻辑函数。
struct HostFunctionContext {
    /// 宿主函数提供者（如 DatabaseHostFunctions、LoggingHostFunctions 等）
    provider: Arc<dyn HostFunctionProvider>,
    /// 函数名称（在提供者的命名空间内唯一）
    func_name: String,
}

impl HostFunctionContext {
    /// 创建新的宿主函数上下文
    fn new(provider: Arc<dyn HostFunctionProvider>, func_name: String) -> Self {
        Self { provider, func_name }
    }
}

/// Extism 宿主函数回调
///
/// 当 WASM 插件调用宿主函数时，Extism 运行时会调用此回调。
/// 负责从 Extism 线性内存中读取输入、调用业务逻辑、将结果写回内存。
///
/// # Extism 内存交互
///
/// - `plugin.memory_get_val(&inputs[0])`: 从 Extism 线性内存读取输入
///   inputs[0] 是 i64 类型的内存偏移量（PTR），Extism 自动解码为 Vec<u8>
/// - `plugin.memory_set_val(&mut outputs[0], output_bytes)`: 将结果写入 Extism 线性内存
///   outputs[0] 也是 i64 类型的 PTR，Extism 自动编码字节并返回偏移量
///
/// # 错误处理
///
/// 业务逻辑调用失败时，返回 MsgPack 编码的错误信息
fn host_function_wrapper(
    plugin: &mut CurrentPlugin,
    inputs: &[extism::Val],
    outputs: &mut [extism::Val],
    user_data: UserData<HostFunctionContext>,
) -> Result<(), extism::Error> {
    let ctx = user_data.get()?;
    let guard = ctx.lock().unwrap();
    // memory_get_val: 从 Extism 线性内存中读取输入
    // 将 i64 偏移量处的数据解码为 字节数组
    let input: Vec<u8> = plugin.memory_get_val(&inputs[0]).unwrap_or_default();

    let result = guard.provider.call(&guard.func_name, input);

    let output_bytes = match result {
        Ok(output) => output,
        Err(e) => {
            let err_resp = serde_json::json!({"success": false, "error": e.to_string()});
            rmp_serde::to_vec(&err_resp).unwrap_or_default()
        }
    };
    // memory_set_val: 将输出字节数组编码并写入 Extism 线性内存
    // 返回的 i64 偏移量会被写入 outputs[0]
    plugin.memory_set_val(&mut outputs[0], output_bytes)?;

    Ok(())
}

/// 为单个函数定义创建 Extism 宿主函数
///
/// 将 `HostFunctionProvider` 中的一个函数包装为 Extism `Function`，
/// 绑定到指定的命名空间。WASM 插件通过 `namespace:func_name` 格式调用。
///
/// # Extism Function 创建参数
///
/// - `name`: 函数名，WASM 插件通过此名称调用
/// - `[ValType::I64]`: 输入参数类型（Extism 内存偏移量 PTR）
/// - `[ValType::I64]`: 输出参数类型（Extism 内存偏移量 PTR）
/// - `UserData::new(ctx)`: 携带调用上下文的用户数据
/// - `host_function_wrapper`: Extism 回调函数
fn create_host_function(
    provider: Arc<dyn HostFunctionProvider>,
    func_name: &str,
    namespace: &str,
) -> Function {
    let ctx = HostFunctionContext::new(provider, func_name.to_string());

    // Function::new() 创建一个 Extism 宿主函数
    Function::new(
        func_name,
        [ValType::I64],
        [ValType::I64],
        UserData::new(ctx),
        host_function_wrapper,
    )
    // with_namespace() 设置函数命名空间，避免不同提供者的同名函数冲突
    .with_namespace(namespace)
}

/// 将提供者的所有函数注册为 Extism 宿主函数
///
/// 遍历 `HostFunctionProvider` 提供的函数定义列表，
/// 为每个函数创建 Extism `Function` 并追加到缓存列表中。
///
/// # 参数
///
/// - `provider`: 宿主函数提供者
/// - `cached_functions`: 宿主函数缓存列表（会被追加）
///
/// # 返回值
///
/// 成功返回本次注册的函数数量
pub fn register_provider_functions(
    provider: Arc<dyn HostFunctionProvider>,
    cached_functions: &mut Vec<Function>,
) -> Result<usize, ExtismError> {
    let namespace = provider.namespace();
    let functions = provider.functions();
    let func_count = functions.len();

    for func_def in functions {
        let func = create_host_function(provider.clone(), func_def.name, namespace);
        cached_functions.push(func);
    }

    tracing::info!(
        "已注册宿主函数提供者 [{}]，提供 {} 个函数，当前缓存 {} 个函数",
        namespace,
        func_count,
        cached_functions.len()
    );

    Ok(func_count)
}
