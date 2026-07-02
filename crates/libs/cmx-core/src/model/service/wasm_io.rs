//! WASM 输入输出模块
//!
//! 包含服务编排中函数调用的输入输出结构体。

use super::context::SVRContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 函数输入结构体 — 固定入参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为入参。
///
/// # 字段说明
///
/// - `input`: 当前步骤输入数据（serde_json::Value）
/// - `context`: 服务调用上下文，包含初始入参、请求头、各步骤输出、事务ID
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `as_json_value()`: 获取输入作为 JSON Value
/// - `as_str()`: 获取输入作为字符串（如果是字符串类型）
///
/// # 示例
///
/// ```rust
/// use cmx_core::model::service::{FunctionInput, SVRContext};
/// use std::collections::HashMap;
///
/// let input = FunctionInput {
///     input: serde_json::json!({"name": "test"}),
///     context: SVRContext::new(serde_json::Value::Null, HashMap::new(), ...),
///     binary_data: HashMap::new(),
/// };
///
/// // 使用辅助方法
/// let json = input.as_json_value();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInput {
    /// 当前步骤输入数据（serde_json::Value）
    pub input: serde_json::Value,
    /// 服务调用上下文（包含 txn_id）
    pub context: SVRContext,
    /// 二进制数据（文件、图像等）
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}

impl FunctionInput {
    /// 获取输入作为 JSON Value
    ///
    /// # 返回值
    /// 返回 input 字段（已经是 Value 类型）
    pub fn as_json_value(&self) -> &serde_json::Value {
        &self.input
    }

    /// 获取输入作为字符串（如果是字符串类型）
    ///
    /// # 返回值
    /// - 如果 input 是字符串，返回对应的 &str
    /// - 否则返回空字符串
    pub fn as_str(&self) -> &str {
        self.input.as_str().unwrap_or("")
    }

    /// 将任意类型转换为 serde_json::Value 并创建 FunctionInput
    ///
    /// # 类型参数
    /// - `T`: 需要实现 Serialize trait 的类型
    ///
    /// # 参数
    /// - `input`: 任意可序列化类型
    /// - `context`: 服务调用上下文
    ///
    /// # 示例
    ///
    /// ```rust
    /// use cmx_core::model::service::{FunctionInput, SVRContext};
    ///
    /// let input = FunctionInput::from_value(
    ///     serde_json::json!({"name": "test", "count": 42}),
    ///     context,
    /// );
    /// ```
    pub fn from_value(input: serde_json::Value, context: SVRContext) -> Self {
        Self {
            input,
            context,
            binary_data: HashMap::new(),
        }
    }

    /// 将可序列化类型转换为 Value 并创建 FunctionInput
    ///
    /// # 类型参数
    /// - `T`: 需要实现 Serialize trait 的类型
    ///
    /// # 参数
    /// - `input`: 任意可序列化类型
    /// - `context`: 服务调用上下文
    ///
    /// # 示例
    ///
    /// ```rust
    /// use cmx_core::model::service::{FunctionInput, SVRContext};
    ///
    /// #[derive(Serialize)]
    /// struct MyInput {
    ///     name: String,
    ///     count: u32,
    /// }
    ///
    /// let input = FunctionInput::from_input(
    ///     MyInput { name: "test".to_string(), count: 42 },
    ///     context,
    /// );
    /// ```
    pub fn from_input<T: serde::Serialize>(input: T, context: SVRContext) -> Self {
        Self {
            input: serde_json::to_value(input).unwrap_or(serde_json::Value::Null),
            context,
            binary_data: HashMap::new(),
        }
    }
}

/// 函数输出结构体 — 固定出参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为出参。
///
/// # 字段说明
///
/// - `result`: 函数执行结果（serde_json::Value）
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `new(result)`: 从 Value 创建输出
/// - `from_json(value)`: 从 JSON Value 创建输出（别名）
/// - `with_binary(key, data)`: 添加二进制数据
///
/// # 示例
///
/// ```rust
/// use cmx_core::model::service::FunctionOutput;
///
/// // 从 JSON 创建
/// let output = FunctionOutput::new(serde_json::json!({
///     "status": "success"
/// }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionOutput {
    /// 函数执行结果（serde_json::Value）
    pub result: serde_json::Value,
    /// 二进制数据（文件、图像等）
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}

impl FunctionOutput {
    /// 创建新的输出
    ///
    /// # 参数
    /// - `result`: 执行结果 Value
    pub fn new(result: serde_json::Value) -> Self {
        Self {
            result,
            binary_data: HashMap::new(),
        }
    }

    /// 从 JSON Value 创建输出（别名）
    ///
    /// # 参数
    /// - `value`: JSON 值
    pub fn from_json(value: serde_json::Value) -> Self {
        Self::new(value)
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

    /// 将任意类型转换为 serde_json::Value 并创建 FunctionOutput
    ///
    /// # 参数
    /// - `value`: JSON 值
    ///
    /// # 示例
    ///
    /// ```rust
    /// use cmx_core::model::service::FunctionOutput;
    ///
    /// let output = FunctionOutput::from_value(serde_json::json!({
    ///     "status": "success",
    ///     "data": [1, 2, 3]
    /// }));
    /// ```
    pub fn from_value(value: serde_json::Value) -> Self {
        Self::new(value)
    }

    /// 将可序列化类型转换为 Value 并创建 FunctionOutput
    ///
    /// # 类型参数
    /// - `T`: 需要实现 Serialize trait 的类型
    ///
    /// # 参数
    /// - `result`: 任意可序列化类型
    ///
    /// # 示例
    ///
    /// ```rust
    /// use cmx_core::model::service::FunctionOutput;
    ///
    /// #[derive(Serialize)]
    /// struct MyResult {
    ///     status: String,
    ///     count: u32,
    /// }
    ///
    /// let output = FunctionOutput::from_result(MyResult {
    ///     status: "success".to_string(),
    ///     count: 42,
    /// });
    /// ```
    pub fn from_result<T: serde::Serialize>(result: T) -> Self {
        Self {
            result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
            binary_data: HashMap::new(),
        }
    }
}
