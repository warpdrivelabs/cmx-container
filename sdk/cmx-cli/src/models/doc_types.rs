//! 文档类型定义
//!
//! 定义插件文档的 JSON 结构。

use serde::{Deserialize, Serialize};

/// 插件文档根结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDocument {
    /// 插件信息
    pub plugin: PluginInfo,
    /// 函数列表
    pub functions: Vec<FunctionDoc>,
    /// 类型定义
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<serde_json::Value>,
}

/// 插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// 插件名称
    pub name: String,
    /// 版本号
    pub version: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 生成时间 (ISO 8601)
    pub generated_at: String,
}

/// 函数文档
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDoc {
    /// 函数名
    pub name: String,
    /// 文档类型：func（普通函数）或 branch_fn（分支函数）
    #[serde(rename = "type")]
    pub doc_type: String,
    /// 简短描述
    pub summary: String,
    /// 详细描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 输入说明
    pub input: InputSpec,
    /// 输出说明
    pub output: OutputSpec,
    /// 示例
    pub examples: Vec<Example>,
    /// 错误说明
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// 注意事项
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// 源码位置
    pub location: SourceLocation,
}

/// 输入规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSpec {
    /// 编码方式
    pub encoding: String,
    /// 类型名称
    #[serde(rename = "type")]
    pub type_name: String,
    /// 字段说明
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
}

/// 输出规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpec {
    /// 编码方式
    pub encoding: String,
    /// 类型名称
    #[serde(rename = "type")]
    pub type_name: String,
    /// 字段说明
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
}

/// 字段规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    /// 字段名
    pub name: String,
    /// 字段类型
    #[serde(rename = "type")]
    pub type_name: String,
    /// 是否必填
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// 字段说明
    pub description: String,
}

/// 示例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    /// 输入 JSON
    pub input: String,
    /// 输出 JSON
    pub output: String,
}

/// 源码位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    /// 相对文件路径
    pub file: String,
    /// 起始行号
    pub line: usize,
}

/// 编码方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    /// JSON 编码
    Json,
    /// MessagePack 编码
    Msgpack,
    /// 原始字节
    Raw,
}

impl std::fmt::Display for Encoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Encoding::Json => write!(f, "json"),
            Encoding::Msgpack => write!(f, "msgpack"),
            Encoding::Raw => write!(f, "raw"),
        }
    }
}

impl From<&str> for Encoding {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Encoding::Json,
            "msgpack" => Encoding::Msgpack,
            _ => Encoding::Raw,
        }
    }
}
