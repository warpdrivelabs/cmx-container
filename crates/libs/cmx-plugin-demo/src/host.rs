use crate::models::*;
#[cfg(test)]
use mockall::automock;

/// 宿主功能 trait。
///
/// 定义了 WASM 插件可调用的全部宿主能力，包括日志、数据库、
/// 缓存、插件调用（本地+远程）和服务编排（本地+远程）。
#[cfg_attr(test, automock)]
pub trait HostFunctions {
    // ── 日志 ──────────────────────────────────────

    /// 记录信息级别日志。
    fn log_info(&self, message: &str) -> Result<(), String>;

    /// 记录错误级别日志。
    fn log_error(&self, message: &str) -> Result<(), String>;

    /// 记录调试级别日志。
    fn log_debug(&self, message: &str) -> Result<(), String>;

    /// 记录警告级别日志。
    fn log_warn(&self, message: &str) -> Result<(), String>;

    // ── 数据库 ────────────────────────────────────

    /// 执行数据库查询。
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String>;

    /// 执行数据库修改操作。
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String>;

    // ── 缓存 ──────────────────────────────────────

    /// 获取缓存值。
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String>;

    /// 设置缓存值。
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String>;

    /// 删除缓存值。
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String>;

    // ── 插件调用 ──────────────────────────────────

    /// 调用本地插件。
    fn call_plugin(&self, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;

    /// 调用远程插件。
    fn call_remote_plugin(&self, server_name: &str, request: PluginFunRequest) -> Result<PluginFunCallResponse, String>;

    // ── 服务编排 ──────────────────────────────────

    /// 通过服务键调用本地服务编排。
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<CallServiceResponse, String>;

    /// 调用远程服务编排。
    fn call_remote_service(&self, server_name: &str, request: CallServiceRequest) -> Result<CallServiceResponse, String>;
}
