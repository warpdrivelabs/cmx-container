//! 审计记录模块
//!
//! 定义审计记录结构

use chrono::{DateTime, Utc};
use cmx_utils::snowflake_id_str;
use serde::{Deserialize, Serialize};

/// 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationType {
    /// 安装
    Install,
    /// 卸载
    Uninstall,
    /// 激活
    Activate,
    /// 停用
    Deactivate,
    /// 升级
    Upgrade,
    /// 降级
    Downgrade,
    /// 回滚
    Rollback,
    /// 配置更新
    ConfigUpdate,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Install => write!(f, "install"),
            OperationType::Uninstall => write!(f, "uninstall"),
            OperationType::Activate => write!(f, "activate"),
            OperationType::Deactivate => write!(f, "deactivate"),
            OperationType::Upgrade => write!(f, "upgrade"),
            OperationType::Downgrade => write!(f, "downgrade"),
            OperationType::Rollback => write!(f, "rollback"),
            OperationType::ConfigUpdate => write!(f, "config_update"),
        }
    }
}

/// 操作结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationResult {
    /// 成功
    Success,
    /// 失败
    Failure,
}

impl std::fmt::Display for OperationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationResult::Success => write!(f, "success"),
            OperationResult::Failure => write!(f, "failure"),
        }
    }
}

/// 审计记录
///
/// 对应 cmx_plugin_audit_log 表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// 主键ID
    pub id: String,
    /// 关联插件ID
    pub plugin_id: String,
    /// 节点ID
    pub node_id: Option<String>,
    /// 关联版本
    pub version: Option<String>,
    /// 关联部署ID
    pub deployment_id: Option<String>,
    /// 操作类型
    pub operation_type: OperationType,
    /// 操作状态
    pub operation_status: OperationResult,

    /// 请求ID（用于链路追踪）
    pub request_id: Option<String>,
    /// 操作详情（JSON）
    pub details: Option<serde_json::Value>,
    /// 旧值
    pub old_value: Option<String>,
    /// 新值
    pub new_value: Option<String>,
    /// 错误代码
    pub error_code: Option<String>,
    /// 错误消息
    pub error_message: Option<String>,
    /// 堆栈跟踪
    pub stack_trace: Option<String>,
    /// 操作开始时间
    pub started_at: DateTime<Utc>,
    /// 操作完成时间
    pub completed_at: Option<DateTime<Utc>>,
    /// 操作耗时（毫秒）
    pub duration_ms: Option<i64>,
}

impl AuditRecord {
    /// 创建新的审计记录
    pub fn new(plugin_id: String, operation: OperationType, result: OperationResult) -> Self {
        Self {
            id: snowflake_id_str(),
            plugin_id,
            node_id: None,
            version: None,
            deployment_id: None,
            operation_type: operation,
            operation_status: result,
            request_id: None,
            details: None,
            old_value: None,
            new_value: None,
            error_code: None,
            error_message: None,
            stack_trace: None,
            started_at: Utc::now(),
            completed_at: None,
            duration_ms: None,
        }
    }

    /// 创建成功记录
    pub fn success(plugin_id: String, operation: OperationType) -> Self {
        Self::new(plugin_id, operation, OperationResult::Success)
    }

    /// 创建失败记录
    pub fn failure(plugin_id: String, operation: OperationType, error: String) -> Self {
        let mut record = Self::new(plugin_id, operation, OperationResult::Failure);
        record.error_message = Some(error);
        record
    }

    /// 设置详细信息
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 设置版本
    pub fn with_version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }

    /// 设置部署ID
    pub fn with_deployment_id(mut self, deployment_id: String) -> Self {
        self.deployment_id = Some(deployment_id);
        self
    }

    /// 设置节点ID
    pub fn with_node_id(mut self, node_id: String) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// 设置请求ID
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// 设置旧值
    pub fn with_old_value(mut self, old_value: String) -> Self {
        self.old_value = Some(old_value);
        self
    }

    /// 设置新值
    pub fn with_new_value(mut self, new_value: String) -> Self {
        self.new_value = Some(new_value);
        self
    }

    /// 设置错误代码
    pub fn with_error_code(mut self, error_code: String) -> Self {
        self.error_code = Some(error_code);
        self
    }

    /// 设置堆栈跟踪
    pub fn with_stack_trace(mut self, stack_trace: String) -> Self {
        self.stack_trace = Some(stack_trace);
        self
    }

    /// 设置操作完成时间和耗时
    pub fn with_completed(mut self, duration_ms: i64) -> Self {
        self.completed_at = Some(Utc::now());
        self.duration_ms = Some(duration_ms);
        self
    }
}
