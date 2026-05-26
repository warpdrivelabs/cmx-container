use crate::models::*;
#[cfg(test)]
use mockall::automock;

/// 宿主功能 trait。
///
/// 定义了 WASM 插件可调用的宿主能力，包括日志、缓存、数据库、
/// 插件调用和服务编排等功能。
#[cfg_attr(test, automock)]
pub trait HostFunctions {
    /// 记录信息级别日志。
    ///
    /// # Arguments
    ///
    /// * `message` - 日志消息内容。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`，失败时返回错误信息。
    fn log_info(&self, message: &str) -> Result<(), String>;

    /// 记录错误级别日志。
    ///
    /// # Arguments
    ///
    /// * `message` - 日志消息内容。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`，失败时返回错误信息。
    fn log_error(&self, message: &str) -> Result<(), String>;

    /// 记录调试级别日志。
    ///
    /// # Arguments
    ///
    /// * `message` - 日志消息内容。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`，失败时返回错误信息。
    fn log_debug(&self, message: &str) -> Result<(), String>;

    /// 记录警告级别日志。
    ///
    /// # Arguments
    ///
    /// * `message` - 日志消息内容。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`，失败时返回错误信息。
    fn log_warn(&self, message: &str) -> Result<(), String>;

    /// 执行数据库查询。
    ///
    /// # Arguments
    ///
    /// * `request` - 查询请求，包含 SQL 语句和参数。
    ///
    /// # Returns
    ///
    /// 成功时返回 `DbResponse`，失败时返回错误信息。
    fn db_query(&self, request: DbRequest) -> Result<DbResponse, String>;

    /// 执行数据库修改操作。
    ///
    /// # Arguments
    ///
    /// * `request` - 执行请求，包含 SQL 语句和参数。
    ///
    /// # Returns
    ///
    /// 成功时返回 `DbResponse`，失败时返回错误信息。
    fn db_execute(&self, request: DbRequest) -> Result<DbResponse, String>;

    /// 获取缓存值。
    ///
    /// # Arguments
    ///
    /// * `key` - 缓存键名。
    ///
    /// # Returns
    ///
    /// 成功时返回 `CacheResponse`，失败时返回错误信息。
    fn cache_get(&self, key: &str) -> Result<CacheResponse, String>;

    /// 设置缓存值。
    ///
    /// # Arguments
    ///
    /// * `key` - 缓存键名。
    /// * `value` - 缓存值。
    /// * `ttl_seconds` - 过期时间（秒），`None` 表示不过期。
    ///
    /// # Returns
    ///
    /// 成功时返回 `CacheResponse`，失败时返回错误信息。
    fn cache_set(&self, key: &str, value: serde_json::Value, ttl_seconds: Option<u64>) -> Result<CacheResponse, String>;

    /// 删除缓存值。
    ///
    /// # Arguments
    ///
    /// * `key` - 缓存键名。
    ///
    /// # Returns
    ///
    /// 成功时返回 `CacheResponse`，失败时返回错误信息。
    fn cache_delete(&self, key: &str) -> Result<CacheResponse, String>;

    /// 调用指定插件。
    ///
    /// # Arguments
    ///
    /// * `request` - 插件调用请求，包含插件ID、函数名和输入参数。
    ///
    /// # Returns
    ///
    /// 成功时返回插件执行结果的 JSON 值，失败时返回错误信息。
    fn call_plugin(&self, request: PluginFunRequest) -> Result<serde_json::Value, String>;

    /// 通过服务键调用服务编排。
    ///
    /// # Arguments
    ///
    /// * `request` - 服务调用请求，包含服务键和输入参数。
    ///
    /// # Returns
    ///
    /// 成功时返回服务执行结果的 JSON 值，失败时返回错误信息。
    fn call_service_by_key(&self, request: CallServiceRequest) -> Result<serde_json::Value, String>;
}
