use std::any::Any;



pub trait CMXResponse : Any + Send + Sync{
    fn get_request_id(&self) -> &str;
    fn get_status_code(&self) -> i32;
    fn get_data(&self) -> Option<&str>;
    fn get_error(&self) -> Option<&str>;
}
