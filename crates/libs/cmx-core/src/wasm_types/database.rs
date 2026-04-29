//! 数据库操作相关类型
//!
//! 定义宿主与 WASM 之间数据库操作的请求和响应结构体。

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use crate::model::data::dataset::DataSet;

/// 数据库请求
///
/// 用于 WASM 插件向宿主发起数据库查询或执行操作。
/// 支持指定数据库ID和事务ID。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRequest {
    /// SQL 语句
    pub sql: String,
    /// SQL 参数（JSON 数组）
    #[serde(default)]
    pub params: Option<JsonValue>,
    /// 数据集ID(可选，用于查询结果标识)
    #[serde(default)]
    pub dataset_id: Option<String>,
    /// 数据库ID(可选，未指定时使用默认数据库)
    #[serde(default)]
    pub db_id: Option<String>,
    /// 事务ID(可选，用于在指定事务中执行)
    #[serde(default)]
    pub txn_id: Option<String>,
}

/// 数据库操作响应
///
/// 宿主返回给 WASM 插件的数据库操作结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbResponse {
    /// 是否成功
    pub success: bool,
    /// 影响行数(写操作返回)
    // #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub affected_rows: Option<u64>,
    /// 查询结果数据集(查询操作返回)
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset: Option<DataSet>,
    /// 事务ID(事务操作返回)
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
    /// 错误信息
    // #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub error: Option<String>,
}
