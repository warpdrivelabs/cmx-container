pub mod params;
pub use params::{PageParams, ListParams,UpdatePayload, DeletePayload};

use std::{any::Any, collections::HashMap};
use serde::{Deserialize, Serialize};

pub trait CMXRequest: Any + Send + Sync {
    fn get_request_id(&self) -> &str;
    fn get_service_name(&self) -> &str;
    fn get_function_name(&self) -> &str;
    fn get_parameters(&self) -> &str;
    fn get_headers(&self) -> &HashMap<String, String>;
    fn get_timeout(&self) -> u64;
    fn set_timeout(&mut self, timeout: u64);
    fn add_header(&mut self, key: String, value: String);
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestRequest {
    /// 请求ID
    pub request_id: String,
    /// 目标服务名称
    pub service_name: String,
    /// 要调用的函数名
    pub function_name: String,
    /// 函数参数 (JSON格式)
    pub parameters: String,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 调用超时时间（毫秒）
    pub timeout: u64,
}

impl CMXRequest for RestRequest {
    fn get_request_id(&self) -> &str {
        &self.request_id
    }

    fn get_service_name(&self) -> &str {
        &self.service_name
    }

    fn get_function_name(&self) -> &str {
        &self.function_name
    }

    fn get_parameters(&self) -> &str {
        &self.parameters
    }

    fn get_headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    fn get_timeout(&self) -> u64 {
        self.timeout
    }

    fn set_timeout(&mut self, timeout: u64) {
        self.timeout = timeout;
    }

    fn add_header(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }
}


// 分页响应的分页元数据
#[derive(Debug, Serialize)]
pub struct PageInfo {
    /// 匹配查询的项目总数
    pub total: i64,
    /// 每页项目数
    pub page_size: i64,
    /// 当前页码（从 1 开始）
    pub page_number: i64,
    /// 总页数
    pub total_pages: i64,
    /// 是否有更多页
    pub has_more: bool,
}

impl PageInfo {
    /// 创建新的分页信息
    ///
    /// 参数：
    /// * `total` - 项目总数
    /// * `page_size` - 每页项目数
    /// * `page_number` - 当前页码
    ///
    /// 返回：
    /// * `Self` - 分页信息实例
    pub fn new(total: i64, page_size: i64, page_number: i64) -> Self {
        let total_pages = if page_size > 0 {
            (total + page_size - 1) / page_size
        } else {
            0
        };
        Self {
            total,
            page_size,
            page_number,
            total_pages,
            has_more: page_number < total_pages,
        }
    }
}
