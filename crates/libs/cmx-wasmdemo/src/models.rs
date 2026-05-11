use serde::{Deserialize, Serialize};

pub use cmx_plugin_sdk::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, CallServiceRequest, CallServiceResponse,
};

/// 示例请求 — 用于演示函数的业务参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequest {
    pub name: String,
    pub count: u32,
}

/// 示例响应 — 用于演示函数的业务结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoResponse {
    pub message: String,
    pub total: u32,
}

impl Default for DemoResponse {
    fn default() -> Self {
        Self {
            message: String::new(),
            total: 0,
        }
    }
}

//route_check 使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInput {
    pub route: String,
}

//tx_insert使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertData {
    pub table: String,
    pub name: String,
    pub value: i32,
}

//tx_update使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateData {
    pub table: String,
    pub name: String,
    pub value: i32,
}

// tx_querys使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryData {
    pub table: String,
    pub name: String,
}

//tx_delete使用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteData {
    pub table: String,
    pub name: String,
}
