//! 数据库操作相关类型
//!
//! 定义宿主与 WASM 之间数据库操作的请求和响应结构体。

use serde::{Deserialize, Serialize};

/// 数据库查询请求
///
/// 用于 WASM 插件向宿主发起数据库查询或执行操作。
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
///
/// 宿主返回给 WASM 插件的数据库操作结果。
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
