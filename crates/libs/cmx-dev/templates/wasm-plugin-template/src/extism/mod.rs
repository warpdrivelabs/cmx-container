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

    // ── IAM 用户/权限查询（委托到 HostCaller）──

    fn get_user_details(&self, user_id: &str) -> Result<Option<WasmUserDetails>, String> {
        HostCaller::get_user_details(user_id).map_err(|e| e.to_string())
    }

    fn get_users_details(&self, user_ids: &[String]) -> Result<Vec<WasmUserDetails>, String> {
        HostCaller::get_users_details(user_ids).map_err(|e| e.to_string())
    }

    fn get_user_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<Option<WasmEffectivePermissions>, String> {
        HostCaller::get_user_effective_permissions(user_id).map_err(|e| e.to_string())
    }

    fn has_permission(&self, user_id: &str, code: &str) -> Result<bool, String> {
        HostCaller::has_permission(user_id, code).map_err(|e| e.to_string())
    }

    fn has_role(&self, user_id: &str, code: &str) -> Result<bool, String> {
        HostCaller::has_role(user_id, code).map_err(|e| e.to_string())
    }

    fn has_permissions(
        &self,
        user_id: &str,
        codes: &[String],
    ) -> Result<Vec<WasmCheckResult>, String> {
        HostCaller::has_permissions(user_id, codes).map_err(|e| e.to_string())
    }

    fn has_roles(
        &self,
        user_id: &str,
        codes: &[String],
    ) -> Result<Vec<WasmCheckResult>, String> {
        HostCaller::has_roles(user_id, codes).map_err(|e| e.to_string())
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod iam;
pub mod plugin_call;
pub mod orchestration;
