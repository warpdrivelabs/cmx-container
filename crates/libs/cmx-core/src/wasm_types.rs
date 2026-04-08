use serde::{Deserialize, Serialize};

/// 数据库查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbQueryRequest {
    /// SQL 语句
    pub sql: String,
    /// SQL 参数(JSON 字符串)
    #[serde(default)]
    pub params: Option<String>,
    /// 数据集ID(可选)
    #[serde(default)]
    pub dataset_id: Option<String>,
}

/// 数据库操作响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbResponse {
    /// 是否成功
    pub success: bool,
    /// 影响行数(写操作返回)
    pub affected_rows: Option<u64>,
    /// 查询结果数据集(查询操作返回,JSON 字符串)
    pub dataset: Option<String>,
    /// 事务ID(事务操作返回)
    pub txn_id: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

/// 缓存读取请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheGetRequest {
    /// 缓存键
    pub key: String,
}

/// 缓存写入请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSetRequest {
    /// 缓存键
    pub key: String,
    /// 缓存值
    pub value: String,
    /// 过期时间(秒)
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// 缓存操作响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponse {
    /// 是否成功
    pub success: bool,
    /// 缓存值(读取操作返回)
    pub value: Option<String>,
    /// 是否存在
    pub exists: Option<bool>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallRequest {
    /// 目标插件ID
    pub target_plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据(JSON 字符串)
    pub input: String,
}

/// 插件调用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallResponse {
    /// 是否成功
    pub success: bool,
    /// 输出数据(JSON 字符串)
    pub output: Option<String>,
    /// 执行耗时(微秒)
    pub elapsed_us: Option<u64>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfoResponse {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 当前事务ID
    pub txn_id: Option<String>,
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
}

/// 通用 WASM 函数请求
/// 
/// 用于 Host 调用 WASM 函数时的通用请求包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionRequest<T> {
    /// 调用上下文
    pub context: WasmContext,
    /// 业务请求数据
    pub data: T,
}

/// 通用 WASM 函数响应
/// 
/// 用于 WASM 函数返回时的通用响应包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionResponse<T> {
    /// 是否成功
    pub success: bool,
    /// 业务响应数据
    pub data: Option<T>,
    /// 错误信息
    pub error: Option<String>,
}

/// WASM 调用上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmContext {
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
    /// 数据库ID
    pub db_id: String,
    /// 事务ID
    pub txn_id: Option<String>,
    /// 插件ID
    pub plugin_id: String,
}
