//! 审计记录模块
//! 
//! 定义审计记录结构

use chrono::{DateTime, Utc};
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

/// 审计记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// 记录ID
    pub id: String,
    /// 插件ID
    pub plugin_id: String,
    /// 操作类型
    pub operation: OperationType,
    /// 操作结果
    pub result: OperationResult,
    /// 操作时间
    pub timestamp: DateTime<Utc>,
    /// 操作者
    pub operator: Option<String>,
    /// 来源IP
    pub source_ip: Option<String>,
    /// 详细信息
    pub details: Option<serde_json::Value>,
    /// 错误信息
    pub error_message: Option<String>,
}

impl AuditRecord {
    /// 创建新的审计记录
    pub fn new(
        plugin_id: String,
        operation: OperationType,
        result: OperationResult,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id,
            operation,
            result,
            timestamp: Utc::now(),
            operator: None,
            source_ip: None,
            details: None,
            error_message: None,
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
    
    /// 设置操作者
    pub fn with_operator(mut self, operator: String) -> Self {
        self.operator = Some(operator);
        self
    }
    
    /// 设置来源IP
    pub fn with_source_ip(mut self, source_ip: String) -> Self {
        self.source_ip = Some(source_ip);
        self
    }
    
    /// 设置详细信息
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}
