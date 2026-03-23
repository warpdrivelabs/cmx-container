//! 审计日志模块
//!
//! 记录操作日志

use std::collections::VecDeque;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use super::record::AuditRecord;

/// 审计日志配置
pub struct AuditLoggerConfig {
    /// 最大日志条数
    pub max_records: usize,
    /// 是否持久化
    pub persist: bool,
    /// 持久化文件路径
    pub persist_path: Option<std::path::PathBuf>,
    /// 数据库管理器
    pub db_manager: Option<Arc<cmx_database::DatabaseManager>>,
    /// 默认数据库ID
    pub default_db_id: Option<String>,
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            max_records: 10000,
            persist: false,
            persist_path: None,
            db_manager: None,
            default_db_id: Some("default".to_string()),
        }
    }
}

/// 审计日志记录器
pub struct AuditLogger {
    /// 配置
    config: AuditLoggerConfig,
    /// 日志记录（内存缓存）
    records: Arc<RwLock<VecDeque<AuditRecord>>>,
}

impl AuditLogger {
    /// 创建新的审计日志记录器
    pub fn new(config: AuditLoggerConfig) -> Self {
        Self {
            config,
            records: Arc::new(RwLock::new(VecDeque::new())),
        }
    }
    
    /// 创建支持数据库持久化的审计日志记录器
    pub fn with_persistence(
        db_manager: Arc<cmx_database::DatabaseManager>,
        default_db_id: String,
    ) -> Self {
        let config = AuditLoggerConfig {
            persist: true,
            db_manager: Some(db_manager),
            default_db_id: Some(default_db_id),
            ..Default::default()
        };
        Self::new(config)
    }
    
    /// 记录操作（异步持久化到数据库）
    pub async fn log(&self, mut record: AuditRecord) {
        // 设置完成时间和耗时
        if let Some(duration) = record.duration_ms {
            record.completed_at = Some(Utc::now());
            record.duration_ms = Some(duration);
        } else {
            let duration = (Utc::now() - record.started_at).num_milliseconds();
            record.completed_at = Some(Utc::now());
            record.duration_ms = Some(duration);
        }

        // 记录到内存缓存
        let mut records = self.records.write().await;
        if records.len() >= self.config.max_records {
            records.pop_front();
        }
        records.push_back(record.clone());
        drop(records);

        // 持久化到数据库
        if self.config.persist {
            if let Err(e) = self.persist_to_database(&record).await {
                tracing::error!("审计日志持久化失败: {}", e);
            }
        }
    }
    
    /// 异步持久化到数据库
    async fn persist_to_database(&self, record: &AuditRecord) -> Result<(), String> {
        let db_manager = self.config.db_manager.as_ref()
            .ok_or("数据库管理器未设置")?;
        let db_id = self.config.default_db_id.as_ref()
            .ok_or("数据库ID未设置")?;

        let sql = r#"
            INSERT INTO cmx_plugin_audit_log (
                id, plugin_id, version_id, deployment_id, operation_type, 
                operation_status, operator, operator_ip, operator_session,
                request_id, correlation_id, details, old_value, new_value,
                error_code, error_message, stack_trace, started_at, 
                completed_at, duration_ms
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20
            )
        "#;

        let details_str = record.details.as_ref().map(|d| d.to_string());
        let params = serde_json::json!([
            record.id,
            record.plugin_id,
            record.version_id,
            record.deployment_id,
            record.operation.to_string(),
            record.result.to_string(),
            record.operator,
            record.operator_ip,
            record.operator_session,
            record.request_id,
            record.correlation_id,
            details_str,
            record.old_value,
            record.new_value,
            record.error_code,
            record.error_message,
            record.stack_trace,
            record.started_at,
            record.completed_at,
            record.duration_ms,
        ]);

        db_manager
            .execute_sql_with_json(db_id, None, sql, params)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }
    
    /// 获取所有记录
    pub async fn get_all(&self) -> Vec<AuditRecord> {
        let records = self.records.read().await;
        records.iter().cloned().collect()
    }
    
    /// 获取指定插件的记录
    pub async fn get_by_plugin(&self, plugin_id: &str) -> Vec<AuditRecord> {
        let records = self.records.read().await;
        records.iter()
            .filter(|r| r.plugin_id == plugin_id)
            .cloned()
            .collect()
    }
    
    /// 获取指定时间范围的记录
    pub async fn get_by_time_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<AuditRecord> {
        let records = self.records.read().await;
        records.iter()
            .filter(|r| r.started_at >= start && r.started_at <= end)
            .cloned()
            .collect()
    }
    
    /// 清空记录
    pub async fn clear(&self) {
        let mut records = self.records.write().await;
        records.clear();
    }
    
    /// 获取记录数量
    pub async fn len(&self) -> usize {
        let records = self.records.read().await;
        records.len()
    }
    
    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(AuditLoggerConfig::default())
    }
}
