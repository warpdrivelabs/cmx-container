use crate::models::*;
use crate::host_traits::HostFunctions;
use crate::core::PluginCore;
use cmx_plugin_sdk::HostCaller;
use extism_pdk::*;

struct ExtismHost;

impl HostFunctions for ExtismHost {
    fn log_info(&self, message: &str) -> Result<(), String> {
        HostCaller::log_info(message).map_err(|e| e.to_string())
    }

    fn log_error(&self, message: &str) -> Result<(), String> {
        HostCaller::log_error(message).map_err(|e| e.to_string())
    }

    fn log_debug(&self, message: &str) -> Result<(), String> {
        HostCaller::log_debug(message).map_err(|e| e.to_string())
    }

    fn log_warn(&self, message: &str) -> Result<(), String> {
        HostCaller::log_warn(message).map_err(|e| e.to_string())
    }

    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_query(request).map_err(|e| e.to_string())
    }

    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_execute(request).map_err(|e| e.to_string())
    }

    fn cache_get(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_get(key).map_err(|e| e.to_string())
    }

    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String> {
        HostCaller::cache_set(key, value, ttl_seconds).map_err(|e| e.to_string())
    }

    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_delete(key).map_err(|e| e.to_string())
    }

    fn call_plugin(&self, request: PluginFunRequest) -> Result<serde_json::Value, String> {
        HostCaller::call_plugin(request).map_err(|e| e.to_string())
    }

    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<serde_json::Value, String> {
        HostCaller::call_service_by_key(request).map_err(|e| e.to_string())
    }
}

// ==================== 演示函数 ====================

/// 统计字符串中的元音字母数量
///
/// 这是一个简单的字符串处理函数，演示标准入参出参的使用。
///
/// # 输入处理
/// - `input.input`: 要统计的字符串
///
/// # 输出
/// - `result`: JSON 格式的统计结果
///
/// # 示例
/// 输入: `{"input": "hello world", "context": {...}}`
/// 输出: `{"result": "{\"count\":3,\"total\":3,\"input\":\"hello world\"}"}`
#[plugin_fn]
pub fn count_vowels(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.count_vowels(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}


/// 演示日志功能
///
/// 调用宿主的日志函数，记录不同级别的日志信息。
/// 此函数不需要业务输入，直接执行日志演示。
///
/// # 输入处理
/// - 忽略 `input.input`，仅用于演示
///
/// # 输出
/// - `result`: JSON 格式的演示结果
#[plugin_fn]
pub fn demo_log(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_log(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 演示缓存功能
///
/// 演示缓存的写入、读取和删除操作。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，包含 name 和 count
///
/// # 输出
/// - `result`: JSON 格式的操作结果
///
/// # 示例
/// 输入: `{"input": "{\"name\":\"test\",\"count\":100}", "context": {...}}`
/// 输出: `{"result": "{\"message\":\"缓存操作成功: ...\",\"total\":100}"}`
#[plugin_fn]
pub fn demo_cache(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_cache(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 演示数据库查询功能
///
/// 执行一个简单的数据库查询。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，用于构建 SQL
///
/// # 输出
/// - `result`: JSON 格式的查询结果
///
/// # 事务支持
/// 如果 `input.txn_id` 存在，函数将在指定事务中执行 SQL。
#[plugin_fn]
pub fn demo_database(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_database(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

#[plugin_fn]
pub fn demo_call_plugin(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_call_plugin(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

#[plugin_fn]
pub fn demo_call_service_by_key(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_call_service_by_key(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 综合测试入口
///
/// 执行所有演示功能，用于验证插件环境是否正常。
///
/// # 输入处理
/// - `input.input`: JSON 格式的 DemoRequest，用于测试参数传递
///
/// # 输出
/// - `result`: JSON 数组，包含各项测试的结果
///
/// # 测试项
/// 1. 日志功能测试
/// 2. 缓存写入测试
/// 3. 缓存读取测试
/// 4. 数据库查询测试
/// 5. 插件调用测试
#[plugin_fn]
pub fn run_all_demos(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.run_all_demos(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

// ==================== 服务编排测试函数 ====================

/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识。
/// 用于 skylake-switch 节点，返回值对应 options 中的选项。
///
/// # 输入处理
/// - `input.input`: JSON 格式，包含 route 字段
///
/// # 输出
/// - `result`: 返回 "1"、"2" 或 "3"，对应三个分支
///
/// # 示例
/// 输入: `{"input": "{\"route\":\"1\"}", "context": {...}}`
/// 输出: `{"result": "1"}`
#[plugin_fn]
#[doc_type = "branch_fn"]
pub fn route_check(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.route_check(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
/// 分支1处理函数
///
/// 处理分支1的业务逻辑。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_1_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.branch_1_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 分支2处理函数
///
/// 处理分支2的业务逻辑。
///
/// # 输入处理
/// - `input.input`: JSON 格式的业务数据
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_2_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.branch_2_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}


/// 分支3处理函数
///
/// 处理分支3的业务逻辑。
///
/// # 输入处理
/// - `input.input`: JSON 格式的业务数据
/// - `input.context.initial_input`: 初始入参
///
/// # 输出
/// - `result`: JSON 格式的处理结果，包含 branch 字段标识来源
#[plugin_fn]
pub fn branch_3_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.branch_3_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 合并结果函数
///
/// 合并各分支的处理结果。
/// 可以通过 step_outputs 获取各分支的输出。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.step_outputs`: 各步骤的输出缓存
///
/// # 输出
/// - `result`: JSON 格式的合并结果
#[plugin_fn]
pub fn merge_result(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.merge_result(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务插入函数
///
/// 在事务中执行插入操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的插入数据
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_insert(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_insert(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务更新函数
///
/// 在事务中执行更新操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的更新数据
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_update(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_update(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务查询函数
///
/// 在事务中执行查询操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的查询条件
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的查询结果
#[plugin_fn]
pub fn tx_query(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_query(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务删除函数
///
/// 在事务中执行删除操作。
/// 通过 context.txn_id 获取事务ID，确保在同一事务中执行。
///
/// # 输入处理
/// - `input.input`: JSON 格式的删除条件
/// - `input.context.txn_id`: 事务ID（由事务框设置）
///
/// # 输出
/// - `result`: JSON 格式的操作结果
#[plugin_fn]
pub fn tx_delete(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_delete(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 最终处理函数
///
/// 最终处理并返回结果。
///
/// # 输入处理
/// - `input.input`: 前序步骤的输出
/// - `input.context.step_outputs`: 各步骤的输出缓存
///
/// # 输出
/// - `result`: JSON 格式的最终结果
#[plugin_fn]
pub fn final_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.final_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}