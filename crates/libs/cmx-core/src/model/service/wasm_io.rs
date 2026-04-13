//! WASM 输入输出模块
//!
//! 包含服务编排中函数调用的输入输出结构体。

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::context::SVRContext;

/// 函数输入结构体 — 固定入参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为入参。
///
/// # 字段说明
///
/// - `input`: 当前步骤输入数据（JSON 字符串或纯文本）
/// - `context`: 服务调用上下文，包含初始入参、请求头、各步骤输出、事务ID
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `as_str()`: 获取输入作为字符串
/// - `as_json_value()`: 解析为 JSON Value（宽松模式）
/// - `parse_json::<T>()`: 解析为指定类型
///
/// # 示例
///
/// ```rust
/// use cmx_core::model::service::{FunctionInput, SVRContext};
/// use std::collections::HashMap;
///
/// let input = FunctionInput {
///     input: r#"{"name":"test"}"#.to_string(),
///     context: SVRContext::new("初始入参".to_string(), HashMap::new()),
///     binary_data: HashMap::new(),
/// };
///
/// // 使用辅助方法
/// let json = input.as_json_value();
/// let name = input.parse_json::<serde_json::Value>();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInput {
    /// 当前步骤输入数据（JSON 字符串或纯文本）
    pub input: String,
    /// 服务调用上下文（包含 txn_id）
    pub context: SVRContext,
    /// 二进制数据（文件、图像等）
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}

impl FunctionInput {
    /// 将 input 解析为指定类型
    ///
    /// # 类型参数
    /// - `T`: 目标类型，需实现 `DeserializeOwned`
    ///
    /// # 返回值
    /// - `Ok(T)`: 解析成功
    /// - `Err`: 解析失败
    pub fn parse_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.input)
    }

    /// 将 input 解析为 JSON Value（宽松模式，失败返回 Null）
    ///
    /// # 返回值
    /// - 解析成功返回对应的 JSON Value
    /// - 解析失败返回 `Value::Null`
    pub fn as_json_value(&self) -> serde_json::Value {
        serde_json::from_str(&self.input).unwrap_or(serde_json::Value::Null)
    }

    /// 获取 input 作为字符串
    pub fn as_str(&self) -> &str {
        &self.input
    }
}

/// 函数输出结构体 — 固定出参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为出参。
///
/// # 字段说明
///
/// - `result`: 函数执行结果（JSON 字符串或纯文本）
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `new(result)`: 从字符串创建输出
/// - `from_json(value)`: 从 JSON Value 创建输出
/// - `with_binary(key, data)`: 添加二进制数据
///
/// # 示例
///
/// ```rust
/// use cmx_core::model::service::FunctionOutput;
///
/// // 从字符串创建
/// let output = FunctionOutput::new("处理结果");
///
/// // 从 JSON 创建
/// let output = FunctionOutput::from_json(serde_json::json!({
///     "status": "success"
/// }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionOutput {
    /// 函数执行结果（JSON 字符串或纯文本）
    pub result: String,
    /// 二进制数据（文件、图像等）
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}

impl FunctionOutput {
    /// 创建新的输出
    ///
    /// # 参数
    /// - `result`: 执行结果字符串
    pub fn new(result: impl Into<String>) -> Self {
        Self {
            result: result.into(),
            binary_data: HashMap::new(),
        }
    }

    /// 从 JSON Value 创建输出
    ///
    /// # 参数
    /// - `value`: JSON 值，会被序列化为字符串
    pub fn from_json(value: serde_json::Value) -> Self {
        Self {
            result: serde_json::to_string(&value).unwrap_or_default(),
            binary_data: HashMap::new(),
        }
    }

    /// 添加二进制数据
    ///
    /// # 参数
    /// - `key`: 数据键名
    /// - `data`: 二进制数据
    pub fn with_binary(mut self, key: impl Into<String>, data: Vec<u8>) -> Self {
        self.binary_data.insert(key.into(), data);
        self
    }
}
