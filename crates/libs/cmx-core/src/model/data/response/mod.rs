use serde::Serialize;

pub trait CMXResponse: Send + Sync {
    fn get_request_id(&self) -> &str;
    fn get_code(&self) -> i32;
    fn get_data_str(&self) -> Option<String>;
    fn get_error(&self) -> Option<&str>;
}

#[derive(Debug, Serialize)]
pub struct RestResponse<T>
where
    T: Serialize,
{
    ///请求id
    pub request_id: String,
    /// 响应数据
    pub data: Option<T>,
    /// 错误码
    pub code: i32,
    /// 错误信息
    pub error: Option<String>,
}
impl<T> RestResponse<T>
where
    T: Serialize + Send + Sync,
{
    pub fn new(request_id: String, code: i32, data: Option<T>, error: Option<String>) -> Self {
        RestResponse {
            request_id,
            code,
            data,
            error,
        }
    }

    pub  fn get_data(&self) -> Option<&T> {
        self.data.as_ref()
    }
}

impl<T> CMXResponse for RestResponse<T>
where
    T: Serialize + Send + Sync,
{
    fn get_request_id(&self) -> &str {
        &self.request_id
    }

    fn get_code(&self) -> i32 {
        self.code
    }

    fn get_data_str(&self) -> Option<String> {
        // 将数据序列化为 JSON 字符串
        self.data.as_ref().and_then(|d| {
            serde_json::to_string(d)
                .map_err(|e| eprintln!("序列化失败：{}", e))
                .ok()
        })
    }

    fn get_error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}


