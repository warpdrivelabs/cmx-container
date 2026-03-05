use std::{any::Any, collections::HashMap};

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


// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Request {
//     /// 请求ID
//     pub request_id: String,
//     /// 目标服务名称
//     pub service_name: String,
//     /// 要调用的函数名
//     pub function_name: String,
//     /// 函数参数 (JSON格式)
//     pub parameters: String,
//     /// 请求头
//     pub headers: HashMap<String, String>,
//     /// 调用超时时间（毫秒）
//     pub timeout: u64,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Response {
//     /// 对应的请求ID
//     pub request_id: String,
//     /// 响应状态码
//     pub status_code: i32,
//     /// 响应数据 (JSON格式)
//     pub data: Option<String>,
//     /// 错误信息
//     pub error: Option<String>,
// }

// impl RequestTrait for Request {
//     fn get_request_id(&self) -> &str {
//         &self.request_id
//     }
    
//     fn get_service_name(&self) -> &str {
//         &self.service_name
//     }
    
//     fn get_function_name(&self) -> &str {
//         &self.function_name
//     }
    
//     fn get_parameters(&self) -> &str {
//         &self.parameters
//     }
    
//     fn get_headers(&self) -> &HashMap<String, String> {
//         &self.headers
//     }
    
//     fn get_timeout(&self) -> u64 {
//         self.timeout
//     }
    
//     fn set_timeout(&mut self, timeout: u64) {
//         self.timeout = timeout;
//     }
    
//     fn add_header(&mut self, key: String, value: String) {
//         self.headers.insert(key, value);
//     }
// }

// impl ResponseTrait for Response {
//     fn get_request_id(&self) -> &str {
//         &self.request_id
//     }
    
//     fn get_status_code(&self) -> i32 {
//         self.status_code
//     }
    
//     fn get_data(&self) -> Option<&str> {
//         self.data.as_deref()
//     }
    
//     fn get_error(&self) -> Option<&str> {
//         self.error.as_deref()
//     }
// }

// impl Request {
//     pub fn new(service_name: String, function_name: String, parameters: String) -> Self {
//         Request {
//             request_id: uuid::Uuid::new_v4().to_string(),
//             service_name,
//             function_name,
//             parameters,
//             headers: HashMap::new(),
//             timeout: 30000, // 默认30秒超时
//         }
//     }

//     pub fn with_timeout(mut self, timeout: u64) -> Self {
//         self.set_timeout(timeout);
//         self
//     }
// }

// impl Response {
//     pub fn success(request_id: String, data: String) -> Self {
//         Response {
//             request_id,
//             status_code: 200,
//             data: Some(data),
//             error: None,
//         }
//     }

//     pub fn error(request_id: String, error_code: i32, error_message: String) -> Self {
//         Response {
//             request_id,
//             status_code: error_code,
//             data: None,
//             error: Some(error_message),
//         }
//     }
// }
