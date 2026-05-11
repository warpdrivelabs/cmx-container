use crate::models::*;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait HostFunctions {
    fn log_info(&self, message: &str) -> Result<(), String>;
    fn log_error(&self, message: &str) -> Result<(), String>;
    fn log_debug(&self, message: &str) -> Result<(), String>;
    fn log_warn(&self, message: &str) -> Result<(), String>;
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String>;
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String>;
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String>;
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String>;
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String>;
    fn call_plugin(&self, request: PluginFunRequest) -> Result<serde_json::Value, String>;
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<serde_json::Value, String>;
}
