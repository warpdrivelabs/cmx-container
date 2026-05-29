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

    fn call_plugin(&self, request: PluginFunRequest) -> Result<PluginFunCallResponse, String> {
        HostCaller::call_plugin(request).map_err(|e| e.to_string())
    }

    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<CallServiceResponse, String> {
        HostCaller::call_service_by_key(request).map_err(|e| e.to_string())
    }
}

// ==================== 功能函数 ====================

/// 统计字符串中的元音字母数量
///
/// 这是一个简单的字符串处理函数，展示标准入参出参的使用方式。
///
/// # Arguments
///
/// * `input` - `string` 待统计的字符串。
///
/// # Returns
///
/// 成功时返回包含统计结果的 `FunctionOutput`。
///
/// # Examples
///
/// ```
/// let input = FunctionInput {
///     input: serde_json::json!("hello world"),
///     context: Default::default(),
///     binary_data: Default::default(),
/// };
/// // 返回: {"count": 3, "total": 3, "input": "hello world"}
/// ```
#[plugin_fn]
pub fn count_vowels(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.count_vowels(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 记录不同级别的日志信息
///
/// 调用宿主的日志函数，记录 info、error、debug、warn 四个级别的日志。
///
/// # Arguments
///
/// * `input` - 函数输入。
///
/// # Returns
///
/// 成功时返回包含日志记录结果的 `FunctionOutput`。
#[plugin_fn]
pub fn demo_log(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_log(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 执行缓存的写入和读取操作
///
/// 调用宿主的缓存接口，将数据写入缓存后再读取验证。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DemoRequest` 格式的缓存键名和计数值。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 缓存键名 |
/// | `count` | integer | 是 | 计数值 |
///
/// # Returns
///
/// 成功时返回包含缓存操作结果的 `FunctionOutput`。
#[plugin_fn]
pub fn demo_cache(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_cache(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 执行数据库查询操作
///
/// 调用宿主的数据库接口，执行一条 SELECT 查询。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DemoRequest` 格式的查询参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 查询名称 |
/// | `count` | integer | 是 | 计数值 |
///
/// # Returns
///
/// 成功时返回包含查询结果的 `FunctionOutput`。
#[plugin_fn]
pub fn demo_database(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_database(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 调用指定插件
///
/// 通过宿主调用另一个指定的插件函数。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DemoRequest` 格式的请求参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 请求名称 |
/// | `count` | integer | 是 | 计数值 |
///
/// # Returns
///
/// 成功时返回包含调用结果的 `FunctionOutput`，失败时返回包含错误信息的 `FunctionOutput`。
#[plugin_fn]
pub fn demo_call_plugin(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_call_plugin(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 调用服务编排
///
/// 通过宿主调用服务编排接口。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DemoRequest` 格式的请求参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 请求名称 |
/// | `count` | integer | 是 | 计数值 |
///
/// # Returns
///
/// 成功时返回包含调用结果的 `FunctionOutput`，失败时返回包含错误信息的 `FunctionOutput`。
#[plugin_fn]
pub fn demo_call_service_by_key(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.demo_call_service_by_key(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 执行多项功能测试
///
/// 依次执行日志、缓存、数据库等功能的测试。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DemoRequest` 格式的测试参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 测试名称 |
/// | `count` | integer | 是 | 计数值 |
///
/// # Returns
///
/// 返回包含各项测试结果的 `FunctionOutput`。
#[plugin_fn]
pub fn run_all_demos(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.run_all_demos(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

// ==================== 服务编排函数 ====================

/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `RouteInput` 格式的路由参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `route` | string | 是 | 路由标识，取值为 "1"、"2"、"3" 或 "4" |
///
/// # Returns
///
/// 返回 "1"、"2"、"3" 或 "4"，对应四个分支。
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
/// # Arguments
///
/// * `input` - 函数输入，包含前序步骤的输出和初始入参。输入为动态数据，来源于上一步骤的输出。
///
/// # Returns
///
/// 返回包含分支标识和处理结果的 `FunctionOutput`。
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
/// # Arguments
///
/// * `input` - 函数输入，包含前序步骤的输出和初始入参。输入为动态数据，来源于上一步骤的输出。
///
/// # Returns
///
/// 返回包含分支标识和处理结果的 `FunctionOutput`。
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
/// # Arguments
///
/// * `input` - 函数输入，包含前序步骤的输出和初始入参。输入为动态数据，来源于上一步骤的输出。
///
/// # Returns
///
/// 返回包含分支标识和处理结果的 `FunctionOutput`。
#[plugin_fn]
pub fn branch_3_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.branch_3_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 合并结果函数
///
/// 合并各分支的处理结果，从上下文获取各分支的输出并合并。
///
/// # Arguments
///
/// * `input` - 函数输入，包含前序步骤的输出和各步骤的输出缓存。输入为动态数据，来源于上一步骤的输出及上下文中的 `step_outputs` 缓存。
///
/// # Returns
///
/// 返回包含合并结果的 `FunctionOutput`。
#[plugin_fn]
pub fn merge_result(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.merge_result(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务插入函数
///
/// 在事务中执行插入操作，通过上下文获取事务ID确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `InsertData` 格式的插入数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
/// | `value` | integer | 是 | 数值字段值 |
///
/// # Returns
///
/// 返回包含操作结果的 `FunctionOutput`。
#[plugin_fn]
pub fn tx_insert(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_insert(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务更新函数
///
/// 在事务中执行更新操作，通过上下文获取事务ID确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `UpdateData` 格式的更新数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
/// | `value` | integer | 是 | 数值字段值 |
///
/// # Returns
///
/// 返回包含操作结果的 `FunctionOutput`。
#[plugin_fn]
pub fn tx_update(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_update(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务查询函数
///
/// 在事务中执行查询操作，通过上下文获取事务ID确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `QueryData` 格式的查询条件。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
///
/// # Returns
///
/// 返回包含查询结果的 `FunctionOutput`。
#[plugin_fn]
pub fn tx_query(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let coe = PluginCore::new(ExtismHost);
    let output = core.tx_query(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 事务删除函数
///
/// 在事务中执行删除操作，通过上下文获取事务ID确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `DeleteData` 格式的删除条件。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
///
/// # Returns
///
/// 返回包含操作结果的 `FunctionOutput`。
#[plugin_fn]
pub fn tx_delete(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.tx_delete(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}

/// 最终处理函数
///
/// 执行最终处理并返回结果，整合各步骤的输出。
///
/// # Arguments
///
/// * `input` - 函数输入，包含前序步骤的输出和各步骤的输出缓存。输入为动态数据，来源于上一步骤的输出及上下文中的 `step_outputs` 缓存。
///
/// # Returns
///
/// 返回包含最终结果的 `FunctionOutput`。
#[plugin_fn]
pub fn final_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.final_process(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}