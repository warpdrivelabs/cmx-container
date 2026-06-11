//! Extism 适配层。
//!
//! 将 HostCaller 的静态方法委托为 HostFunctions trait 实现，
//! 并通过 #[plugin_fn] 宏暴露插件函数。

use crate::host::HostFunctions;
use crate::models::*;
use cmx_plugin_sdk::HostCaller;

struct ExtismHost;

impl HostFunctions for ExtismHost {
    fn log_info(&self, message: &str) -> Result<(), String> {
        HostCaller::log_info(message).map_err(|e| e.to_string())
    }
    fn log_error(&self, message: &str) -> Result<(), String> {
        HostCaller::log_error(message).map_err(|e| e.to_string())
    }
    fn log_debug(&self, message: &str) -> Result<(), String> {
        HostCaller::log_debug(message).map_err(|e| e.to_string())
    }
    fn log_warn(&self, message: &str) -> Result<(), String> {
        HostCaller::log_warn(message).map_err(|e| e.to_string())
    }
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_query(request).map_err(|e| e.to_string())
    }
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String> {
        HostCaller::db_execute(request).map_err(|e| e.to_string())
    }
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_get(key).map_err(|e| e.to_string())
    }
    fn cache_set(
        &self,
        key: &str,
        value: serde_json::Value,
        ttl_seconds: Option<u64>,
    ) -> Result<CacheResponse, String> {
        HostCaller::cache_set(key, value, ttl_seconds).map_err(|e| e.to_string())
    }
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String> {
        HostCaller::cache_delete(key).map_err(|e| e.to_string())
    }
    fn call_plugin(
        &self,
        request: PluginFunRequest,
    ) -> Result<PluginFunCallResponse, String> {
        HostCaller::call_plugin(request).map_err(|e| e.to_string())
    }
    fn call_remote_plugin(
        &self,
        server_name: &str,
        request: PluginFunRequest,
    ) -> Result<PluginFunCallResponse, String> {
        HostCaller::call_remote_plugin(server_name, request).map_err(|e| e.to_string())
    }
    fn call_service_by_key(
        &self,
        request: CallServiceRequest,
    ) -> Result<CallServiceResponse, String> {
        HostCaller::call_service_by_key(request).map_err(|e| e.to_string())
    }
    fn call_remote_service(
        &self,
        server_name: &str,
        request: CallServiceRequest,
    ) -> Result<CallServiceResponse, String> {
        HostCaller::call_remote_service(server_name, request).map_err(|e| e.to_string())
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod plugin_call;
pub mod orchestration;
