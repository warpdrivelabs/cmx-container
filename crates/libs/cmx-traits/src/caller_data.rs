//! WASM 调用者上下文数据
//!
//! 每次从 HTTP 请求触发 WASM 调用时创建，传递给宿主函数使用。
//! 包含当前插件ID、数据库ID、事务ID等运行时上下文信息。

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// WASM 调用者上下文数据
///
/// 在每次 WASM 函数调用时由宿主注入，宿主函数可通过此结构体
/// 获取当前请求的上下文信息（如数据库ID、事务ID、插件ID等）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerData {
    /// 当前插件ID
    pub plugin_id: String,

    /// 数据库ID（从插件配置或请求上下文获取）
    pub db_id: String,

    /// 当前事务ID（可选，由宿主函数的事务管理创建）
    pub txn_id: Option<String>,

    /// 请求ID（用于链路追踪）
    pub request_id: String,

    /// 租户ID（多租户隔离，预留）
    pub tenant_id: Option<String>,

    /// 自定义扩展数据
    pub extra: HashMap<String, serde_json::Value>,
}

impl CallerData {
    /// 创建新的调用者上下文
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 当前插件ID
    /// * `db_id` - 数据库ID
    pub fn new(plugin_id: impl Into<String>, db_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            db_id: db_id.into(),
            txn_id: None,
            request_id: String::new(),
            tenant_id: None,
            extra: HashMap::new(),
        }
    }

    /// 设置请求ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    /// 设置事务ID
    pub fn with_txn_id(mut self, txn_id: impl Into<String>) -> Self {
        self.txn_id = Some(txn_id.into());
        self
    }

    /// 设置租户ID
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// 添加扩展数据
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }

    /// 获取扩展数据
    pub fn get_extra(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }
}
