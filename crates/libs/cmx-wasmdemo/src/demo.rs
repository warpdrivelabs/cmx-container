//! WASM 演示函数实现
//!
//! 提供各功能演示, 用于调用宿主函数验证 WASM 宿主函数功能。
//!
//! # 数据传递协议
//!
//! 1. WASM 调用宿主函数，传入 (input_ptr, input_len)
//! 2. 宿主函数返回输出数据长度（负值表示错误）
//! 3. WASM 分配内存，调用 get_output 读取结果
//! 4. WASM 处理结果数据

use crate::host_funcs::*;

// cmx:runtime 命名空间 - 运行时函数
#[link(wasm_import_module = "cmx:runtime")]
extern "C" {
    /// 从宿主读取输出数据
    fn get_output(output_ptr: i32, output_len: i32) -> i32;
}

/// 调用宿主函数并获取结果
///
/// # 参数
/// * `host_fn` - 宿主函数指针
/// * `input` - 输入数据
///
/// # 返回值
/// 返回结果数据
unsafe fn call_host_and_get_result(
    host_fn: unsafe extern "C" fn(i32, i32) -> i32,
    input: &[u8],
) -> Option<Vec<u8>> {
    // 调用宿主函数
    let output_len = host_fn(input.as_ptr() as i32, input.len() as i32);
    
    // 负值表示错误
    if output_len < 0 {
        return None;
    }
    
    // 没有输出
    if output_len == 0 {
        return Some(Vec::new());
    }
    
    // 分配缓冲区读取输出
    let mut buffer = vec![0u8; output_len as usize];
    let actual_len = get_output(buffer.as_mut_ptr() as i32, output_len);
    
    if actual_len < 0 {
        return None;
    }
    
    buffer.truncate(actual_len as usize);
    Some(buffer)
}

/// 演示日志功能
///
/// 调用 cmx:log 命名空间的 info/warn/error 函数
#[no_mangle]
pub extern "C" fn demo_log() -> i32 {
    let msg = b"Hello from WASM demo!";
    unsafe { info(msg.as_ptr() as i32, msg.len() as i32) };
    
    let msg = b"This is a warning from WASM demo!";
    unsafe { warn(msg.as_ptr() as i32, msg.len() as i32) };
    
    let msg = b"This is an error from WASM demo!";
    unsafe { error(msg.as_ptr() as i32, msg.len() as i32) };
    
    0
}

/// 演示缓存功能
#[no_mangle]
pub extern "C" fn demo_cache() -> i32 {
    // 设置缓存
    let set_req = br#"{"key": "demo_key", "value": "demo_value", "ttl_seconds": 60}"#;
    unsafe { cache_set(set_req.as_ptr() as i32, set_req.len() as i32) };
    
    // 读取缓存并打印结果
    let get_req = br#"{"key": "demo_key"}"#;
    unsafe {
        if let Some(result) = call_host_and_get_result(cache_get, get_req) {
            // 将结果打印到日志
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    // 删除缓存
    let del_req = br#"{"key": "demo_key"}"#;
    unsafe { cache_delete(del_req.as_ptr() as i32, del_req.len() as i32) };
    
    0
}

/// 演示数据库查询功能
#[no_mangle]
pub extern "C" fn demo_database() -> i32 {
    let query_req = br#"{"sql": "SELECT 1 as value"}"#;
    
    unsafe {
        // 调用查询并打印结果
        if let Some(result) = call_host_and_get_result(query_sql, query_req) {
            // 将查询结果打印到日志
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    0
}

/// 演示数据库事务功能
#[no_mangle]
pub extern "C" fn demo_database_txn() -> i32 {
    // 开启事务
    unsafe {
        if let Some(result) = call_host_and_get_result(txn_begin, b"") {
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    // 执行写操作
    let exec_req = br#"{"sql": "INSERT INTO test (name) VALUES ('demo')"}"#;
    unsafe {
        if let Some(result) = call_host_and_get_result(execute_sql, exec_req) {
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    // 提交事务
    unsafe { txn_commit(0, 0) };
    
    0
}

/// 演示数据库事务回滚功能
#[no_mangle]
pub extern "C" fn demo_database_txn_rollback() -> i32 {
    // 开启事务
    unsafe { txn_begin(0, 0) };
    
    // 执行写操作
    let exec_req = br#"{"sql": "INSERT INTO test (name) VALUES ('will_rollback')"}"#;
    unsafe { execute_sql(exec_req.as_ptr() as i32, exec_req.len() as i32) };
    
    // 回滚事务
    unsafe { txn_rollback(0, 0) };
    
    0
}

/// 演示插件信息获取
#[no_mangle]
pub extern "C" fn demo_plugin_info() -> i32 {
    unsafe {
        if let Some(result) = call_host_and_get_result(get_info, b"") {
            // 打印插件信息到日志
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    0
}

/// 演示插件间调用
#[no_mangle]
pub extern "C" fn demo_call_service() -> i32 {
    let call_req = br#"{
    "target_plugin_id": "other-plugin",
    "function_name": "process",
    "input": {"data": "test"}
}"#;
    
    unsafe {
        if let Some(result) = call_host_and_get_result(call_service, call_req) {
            // 打印调用结果到日志
            info(result.as_ptr() as i32, result.len() as i32);
        }
    }
    
    0
}

/// 综合测试入口 - 运行所有演示
#[no_mangle]
pub extern "C" fn run_all_demos() -> i32 {
    demo_log();
    demo_cache();
    demo_database();
    demo_database_txn();
    demo_plugin_info();
    demo_call_service();
    0
}
