//! 通用审计记录定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 审计域（标识审计事件来源的业务领域）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditDomain {
    /// 认证域（登录/登出/Token 刷新等）
    Auth,
    /// IAM 域（角色分配/权限变更/用户管理等）
    Iam,
    /// 插件域（安装/卸载/升级等）
    Plugin,
    /// 业务域（Domain/Application/Module 等业务操作）
    Biz,
}

/// 操作结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OperationResult {
    /// 操作成功
    Success,
    /// 操作失败
    Failure,
}

/// 通用审计记录（领域无关）
///
/// 记录平台中所有关键操作的审计日志，不耦合具体业务领域。
/// 各域（Auth/Iam/Plugin/Biz）通过 `domain` 字段区分。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// 记录唯一 ID
    pub id: String,
    /// 审计域
    pub domain: AuditDomain,
    /// 操作名称（如 "login", "role_assign", "plugin_install"）
    pub operation: String,
    /// 操作结果
    pub result: OperationResult,
    /// 操作者 ID
    pub actor_id: Option<String>,
    /// 操作者名称
    pub actor_name: Option<String>,
    /// 目标资源类型（如 "user", "role", "plugin"）
    pub target_type: Option<String>,
    /// 目标资源 ID
    pub target_id: Option<String>,
    /// 操作详情（JSON）
    pub details: Option<Value>,
    /// 请求 ID（用于链路追踪）
    pub request_id: Option<String>,
    /// 来源 IP 地址
    pub ip_address: Option<String>,
    /// 操作开始时间
    pub started_at: DateTime<Utc>,
    /// 操作耗时（毫秒）
    pub duration_ms: Option<i64>,
}

impl AuditRecord {
    /// 创建新的审计记录（自动生成 ID 和时间戳）
    pub fn new(domain: AuditDomain, operation: impl Into<String>, result: OperationResult) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            domain,
            operation: operation.into(),
            result,
            actor_id: None,
            actor_name: None,
            target_type: None,
            target_id: None,
            details: None,
            request_id: None,
            ip_address: None,
            started_at: Utc::now(),
            duration_ms: None,
        }
    }

    /// 设置操作者信息
    pub fn with_actor(
        mut self,
        actor_id: impl Into<String>,
        actor_name: impl Into<String>,
    ) -> Self {
        self.actor_id = Some(actor_id.into());
        self.actor_name = Some(actor_name.into());
        self
    }

    /// 设置目标资源信息
    pub fn with_target(
        mut self,
        target_type: impl Into<String>,
        target_id: impl Into<String>,
    ) -> Self {
        self.target_type = Some(target_type.into());
        self.target_id = Some(target_id.into());
        self
    }

    /// 设置操作详情
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 设置请求 ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// 设置 IP 地址
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    /// 设置耗时
    pub fn with_duration(mut self, duration_ms: i64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}
