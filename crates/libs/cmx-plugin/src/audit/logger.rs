//! 审计日志模块
//! 
//! 记录操作日志

use std::collections::VecDeque;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::record::AuditRecord;

/// 审计日志配置
#[derive(Debug, Clone)]
pub struct AuditLoggerConfig {
    /// 最大日志条数
    pub max_records: usize,
    /// 是否持久化
    pub persist: bool,
    /// 持久化文件路径
    pub persist_path: Option<std::path::PathBuf>,
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            max_records: 10000,
            persist: false,
            persist_path: None,
        }
    }
}

/// 审计日志记录器
pub struct AuditLogger {
    /// 配置
    config: AuditLoggerConfig,
    /// 日志记录
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
    
    /// 记录操作
    pub async fn log(&self, record: AuditRecord) {
        let mut records = self.records.write().await;
        
        if records.len() >= self.config.max_records {
            records.pop_front();
        }
        
        records.push_back(record);
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
            .filter(|r| r.timestamp >= start && r.timestamp <= end)
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
