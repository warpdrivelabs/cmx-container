//! 审计日志模块
//!
//! 薄适配层：将插件域审计记录转换为统一 `cmx-audit` 记录并委托其写入。
//!
//! 历史上 cmx-plugin 自写了完整的审计日志实现（DB 插入/查询/解析），
//! 现统一复用 `cmx-audit` 基础设施，本模块仅保留插件域模型
//! （`AuditRecord` / `OperationType`）到通用 `cmx_audit::AuditRecord` 的映射逻辑。

use std::sync::Arc;

use cmx_audit::{
    AuditDomain, AuditLogger as UnifiedAuditLogger, AuditRecord as UnifiedAuditRecord,
    DatabaseAuditStore, DefaultAuditLogger, OperationResult as UnifiedOperationResult,
};
use cmx_database::DatabaseManager;
use serde_json::{Map, Value};

use crate::audit::record::{AuditRecord, OperationResult, OperationType};
use crate::error::{PluginError, PluginResult};

/// 审计日志配置
#[derive(Clone)]
pub struct AuditLoggerConfig {
    /// 数据库管理器
    pub db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    pub default_db_id: String,
    /// 节点ID
    pub node_id: String,
}

impl AuditLoggerConfig {
    /// 创建新的审计日志配置
    pub fn new(
        db_manager: Arc<DatabaseManager>,
        default_db_id: String,
        node_id: String,
    ) -> Self {
        Self {
            db_manager,
            default_db_id,
            node_id,
        }
    }
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            db_manager: Arc::new(DatabaseManager::new(Default::default())),
            default_db_id: "default".to_string(),
            node_id: "default".to_string(),
        }
    }
}

/// 审计日志记录器（适配层）
///
/// 包装统一 `cmx-audit` 的 `AuditLogger` trait，对外暴露插件域友好的 `log` 接口，
/// 内部完成插件域字段（plugin_id / version / old_value 等）到通用审计记录的映射。
///
/// 审计数据最终写入统一表 `cmx_audit_log`（domain = "plugin"）。
pub struct AuditLogger {
    /// 统一审计日志记录器
    inner: Arc<dyn UnifiedAuditLogger>,
    /// 节点ID（用于填充记录的 node_id 字段，写入 details）
    node_id: String,
}

impl AuditLogger {
    /// 创建新的审计日志记录器
    pub fn new(config: AuditLoggerConfig) -> Self {
        let store = DatabaseAuditStore::new(
            config.db_manager,
            config.default_db_id,
            // app_id 与统一审计表 DEFAULT 一致
            "default",
        );
        Self {
            inner: Arc::new(DefaultAuditLogger::new(Arc::new(store))),
            node_id: config.node_id,
        }
    }

    /// 记录操作（委托统一 cmx-audit 写入）
    ///
    /// 将插件域 `AuditRecord` 映射为通用 `cmx_audit::AuditRecord`：
    /// - `plugin_id` → `target_id`（target_type = "plugin"）
    /// - `operation_type` → `operation` 字符串
    /// - `operation_status` → `result`
    /// - `version` / `deployment_id` / `node_id` / `old_value` / `new_value`
    ///   / `error_code` / `error_message` / `stack_trace` → 合并到 `details` JSON
    /// - `request_id` / `started_at` / `duration_ms` → 直接映射
    pub async fn log(&self, mut record: AuditRecord) -> PluginResult<()> {
        // 补全完成时间和耗时（与原实现一致）
        let duration_ms = record.duration_ms.unwrap_or_else(|| {
            (chrono::Utc::now() - record.started_at).num_milliseconds()
        });
        record.completed_at = Some(chrono::Utc::now());
        record.duration_ms = Some(duration_ms);

        // 节点ID回填（与原实现一致：未设置时使用配置的默认节点ID）
        let node_id = record
            .node_id
            .clone()
            .unwrap_or_else(|| self.node_id.clone());

        // 映射操作结果
        let result = match record.operation_status {
            OperationResult::Success => UnifiedOperationResult::Success,
            OperationResult::Failure => UnifiedOperationResult::Failure,
        };

        // 构造统一审计记录
        let mut unified = UnifiedAuditRecord::new(
            AuditDomain::Plugin,
            record.operation_type.to_string(),
            result,
        )
        .with_target("plugin", record.plugin_id.clone())
        .with_duration(duration_ms);

        // started_at 保留原始开始时间
        unified.started_at = record.started_at;

        if let Some(req_id) = record.request_id.take() {
            unified = unified.with_request_id(req_id);
        }

        // 合并插件域专属字段到 details（统一审计无对应列）
        let mut details: Map<String, Value> = record
            .details
            .take()
            .map(|v| match v {
                Value::Object(map) => map,
                other => {
                    let mut m = Map::new();
                    m.insert("details".to_string(), other);
                    m
                }
            })
            .unwrap_or_default();

        if let Some(v) = record.version.take() {
            details.insert("version".to_string(), Value::String(v));
        }
        if let Some(v) = record.deployment_id.take() {
            details.insert("deployment_id".to_string(), Value::String(v));
        }
        if !node_id.is_empty() {
            details.insert("node_id".to_string(), Value::String(node_id));
        }
        if let Some(v) = record.old_value.take() {
            details.insert("old_value".to_string(), Value::String(v));
        }
        if let Some(v) = record.new_value.take() {
            details.insert("new_value".to_string(), Value::String(v));
        }
        if let Some(v) = record.error_code.take() {
            details.insert("error_code".to_string(), Value::String(v));
        }
        if let Some(v) = record.error_message.take() {
            details.insert("error_message".to_string(), Value::String(v));
        }
        if let Some(v) = record.stack_trace.take() {
            details.insert("stack_trace".to_string(), Value::String(v));
        }

        if !details.is_empty() {
            unified = unified.with_details(Value::Object(details));
        }

        self.inner
            .log(unified)
            .await
            .map_err(|e| PluginError::Database(format!("审计日志写入失败: {}", e)))?;

        Ok(())
    }
}

/// 将 `OperationType` 转换为统一审计的 operation 字符串
///
/// 保留此辅助函数以备外部调用方需要手动构造 operation 名称。
#[allow(dead_code)]
pub(crate) fn operation_to_string(op: &OperationType) -> &'static str {
    match op {
        OperationType::Install => "install",
        OperationType::Uninstall => "uninstall",
        OperationType::Activate => "activate",
        OperationType::Deactivate => "deactivate",
        OperationType::Upgrade => "upgrade",
        OperationType::Downgrade => "downgrade",
        OperationType::Rollback => "rollback",
        OperationType::ConfigUpdate => "config_update",
    }
}
