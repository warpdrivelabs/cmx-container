//! RPC 共享数据类型。

/// 插件函数调用结果。
///
/// RPC 方式调用远程插件函数后的返回结果。
/// 与 cmx-api 中的 `FunctionCallResponse` 字段一致，但定义在 cmx-traits 中避免反向依赖。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionCallResult {
    /// 是否执行成功。
    pub success: bool,
    /// 函数执行结果（JSON 格式，失败时为 `None`）。
    pub result: Option<serde_json::Value>,
    /// 执行耗时（微秒）。
    pub elapsed_us: u64,
    /// 错误信息（成功时为 `None`）。
    pub error: Option<String>,
}
