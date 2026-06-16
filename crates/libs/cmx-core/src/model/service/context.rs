//! 服务运行时上下文模块
//!
//! 包含服务调用上下文 SVRContext，用于在节点间传递数据。

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 服务调用上下文 key 常量
pub mod svrkey {
    /// 请求进入时间 key
    pub const KEY_TIME_IN: &str = "cmx_time_in";
    /// 请求ID key
    pub const KEY_REQUEST_ID: &str = "cmx_request_id";
}

/// 认证上下文
///
/// 携带已认证用户的身份信息，在整个调用链中传播。
/// 由 mw_auth 中间件或 gRPC interceptor 从 JWT 解析后构建，
/// 注入到 SVRContext 中，供业务逻辑和 WASM 插件读取调用者身份。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// 用户 ID
    pub user_id: String,
    /// 用户名
    pub username: String,
    /// 角色列表
    #[serde(default)]
    pub roles: Vec<String>,
    /// 权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 所属组织 ID
    pub org_id: Option<String>,
    /// 会话 ID（关联 Redis session）
    #[serde(default)]
    pub session_id: Option<String>,
    /// 设备类型（web/mobile/desktop/api）
    #[serde(default)]
    pub device_type: Option<String>,
    /// 认证方式（password/oauth2/api_key）
    #[serde(default)]
    pub auth_method: Option<String>,
}

impl AuthContext {
    /// 创建新的认证上下文
    pub fn new(user_id: impl Into<String>, username: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            username: username.into(),
            roles: Vec::new(),
            permissions: Vec::new(),
            org_id: None,
            session_id: None,
            device_type: None,
            auth_method: None,
        }
    }

    /// 检查是否拥有指定权限
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }

    /// 检查是否拥有指定角色
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

/// 服务调用上下文
///
/// 用于在服务编排的各节点间传递数据，包含：
/// - 初始输入
/// - 请求头
/// - 各步骤的输出缓存
/// - 事务ID（仅在事务框内执行时设置）
/// - 请求进入时间
/// - 请求ID
/// - 认证上下文（由中间件注入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SVRContext {
    /// 初始输入数据
    pub initial_input: serde_json::Value,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 各步骤的输出缓存（key: 节点ID，value: 输出 Value）
    #[serde(default)]
    pub step_outputs: HashMap<String, serde_json::Value>,
    /// 事务ID（仅在事务框内执行时设置）
    pub txn_id: Option<String>,
    /// 请求进入时间
    pub time_in: DateTime<Utc>,
    /// 请求ID
    pub request_id: String,
    /// 认证上下文（由 mw_auth 中间件或 gRPC interceptor 注入）
    #[serde(default)]
    pub auth_context: Option<AuthContext>,
}

impl SVRContext {
    /// 创建新的上下文
    ///
    /// # 参数
    /// - `initial_input`: 初始输入数据
    /// - `headers`: 请求头
    /// - `time_in`: 请求进入时间
    /// - `request_id`: 请求ID
    pub fn new(initial_input: serde_json::Value, headers: HashMap<String, String>, time_in: DateTime<Utc>, request_id: String) -> Self {
        Self {
            initial_input,
            headers,
            step_outputs: HashMap::new(),
            txn_id: None,
            time_in,
            request_id,
            auth_context: None,
        }
    }

    /// 获取指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    ///
    /// # 返回值
    /// - `Option<&serde_json::Value>`: 该步骤的输出 Value，不存在则返回 None
    pub fn get_step_output(&self, step_id: &str) -> Option<&serde_json::Value> {
        self.step_outputs.get(step_id)
    }

    /// 设置指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    /// - `output`: 该步骤的输出 Value
    pub fn set_step_output(&mut self, step_id: impl Into<String>, output: serde_json::Value) {
        self.step_outputs.insert(step_id.into(), output);
    }

    /// 添加指定步骤的输出（set_step_output 的别名）
    pub fn add_step_output(&mut self, step_id: impl Into<String>, output: serde_json::Value) {
        self.set_step_output(step_id, output);
    }

    /// 清除指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    pub fn remove_step_output(&mut self, step_id: &str) {
        self.step_outputs.remove(step_id);
    }

    /// 清除所有步骤输出
    pub fn clear_step_outputs(&mut self) {
        self.step_outputs.clear();
    }

    /// 设置事务ID
    ///
    /// # 参数
    /// - `txn_id`: 事务ID
    pub fn set_txn_id(&mut self, txn_id: String) {
        self.txn_id = Some(txn_id);
    }

    /// 清除事务ID
    pub fn clear_txn_id(&mut self) {
        self.txn_id = None;
    }

    /// 获取请求进入时间
    pub fn get_time_in(&self) -> DateTime<Utc> {
        self.time_in
    }

    /// 获取请求ID
    pub fn get_request_id(&self) -> &str {
        &self.request_id
    }
}
