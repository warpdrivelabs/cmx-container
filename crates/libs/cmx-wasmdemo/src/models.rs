use serde::{Deserialize, Serialize};

pub use cmx_plugin_sdk::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, CallServiceRequest, CallServiceResponse,
};

/// 示例请求。
///
/// 用于功能函数的业务参数，包含名称和计数值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequest {
    /// 请求名称。
    pub name: String,
    /// 计数值。
    pub count: u32,
}

/// 示例响应。
///
/// 用于功能函数的业务结果，包含消息和总计数值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoResponse {
    /// 响应消息。
    pub message: String,
    /// 总计数值。
    pub total: u32,
}

/// 路由输入。
///
/// 用于路由判断函数的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInput {
    /// 路由标识，取值为 "1"、"2"、"3" 或 "4"。
    pub route: String,
}

/// 插入数据。
///
/// 用于事务插入函数的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertData {
    /// 表名。
    pub table: String,
    /// 名称字段值。
    pub name: String,
    /// 数值字段值。
    pub value: i32,
}

/// 更新数据。
///
/// 用于事务更新函数的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateData {
    /// 表名。
    pub table: String,
    /// 名称字段值。
    pub name: String,
    /// 数值字段值。
    pub value: i32,
}

/// 查询数据。
///
/// 用于事务查询函数的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryData {
    /// 表名。
    pub table: String,
    /// 名称字段值。
    pub name: String,
}

/// 删除数据。
///
/// 用于事务删除函数的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteData {
    /// 表名。
    pub table: String,
    /// 名称字段值。
    pub name: String,
}
