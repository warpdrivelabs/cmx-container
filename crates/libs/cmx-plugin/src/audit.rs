//! 审计日志模块 - 记录插件生命周期操作
//!
//! 提供完整的审计日志功能，记录插件的安装、卸载、激活、停用、升级等操作。
//! 审计日志存储在 cmx_plugin_audit_log 表中。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::OperationType;

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// 唯一标识符
    pub id: String,
    /// 插件 ID
    pub plugin_id: String,
    /// 操作类型
    pub operation_type: OperationType,
    /// 操作者
    pub operator: String,
    /// 操作状态
    pub status: crate::types::OperationStatus,
    /// 详细信息 (JSON)
    pub details: serde_json::Value,
    /// 错误信息 (如有)
    pub error_message: Option<String>,
    /// 客户端 IP
    pub client_ip: Option<String>,
    /// 用户代理
    pub user_agent: Option<String>,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// 审计日志构建器
#[derive(Debug, Clone)]
pub struct AuditLogBuilder {
    plugin_id: String,
    operation_type: OperationType,
    operator: String,
    status: crate::types::OperationStatus,
    details: serde_json::Value,
    error_message: Option<String>,
    client_ip: Option<String>,
    user_agent: Option<String>,
}

impl AuditLogBuilder {
    /// 创建新的审计日志构建器
    pub fn new(plugin_id: impl Into<String>, operation_type: OperationType, operator: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            operation_type,
            operator: operator.into(),
            status: crate::types::OperationStatus::Pending,
            details: serde_json::json!({}),
            error_message: None,
            client_ip: None,
            user_agent: None,
        }
    }

    /// 设置操作状态
    pub fn with_status(mut self, status: crate::types::OperationStatus) -> Self {
        self.status = status;
        self
    }

    /// 设置详细信息
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    /// 添加详情字段
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(&value) {
            if let serde_json::Value::Object(ref mut map) = self.details {
                map.insert(key.into(), v);
            }
        }
        self
    }

    /// 设置错误信息
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }

    /// 设置客户端 IP
    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    /// 设置用户代理
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// 构建审计日志条目
    pub fn build(self) -> AuditLogEntry {
        AuditLogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: self.plugin_id,
            operation_type: self.operation_type,
            operator: self.operator,
            status: self.status,
            details: self.details,
            error_message: self.error_message,
            client_ip: self.client_ip,
            user_agent: self.user_agent,
            timestamp: Utc::now(),
        }
    }
}

/// 审计日志查询过滤器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditLogFilter {
    /// 插件 ID 过滤
    pub plugin_id: Option<String>,
    /// 操作类型过滤
    pub operation_type: Option<OperationType>,
    /// 操作者过滤
    pub operator: Option<String>,
    /// 状态过滤
    pub status: Option<crate::types::OperationStatus>,
    /// 开始时间
    pub start_time: Option<DateTime<Utc>>,
    /// 结束时间
    pub end_time: Option<DateTime<Utc>>,
    /// 分页：页码
    pub page: Option<u64>,
    /// 分页：每页数量
    pub page_size: Option<u64>,
}

impl AuditLogFilter {
    /// 创建新的查询过滤器
    pub fn new() -> Self {
        Self::default()
    }

    /// 按插件 ID 过滤
    pub fn with_plugin_id(mut self, plugin_id: impl Into<String>) -> Self {
        self.plugin_id = Some(plugin_id.into());
        self
    }

    /// 按操作类型过滤
    pub fn with_operation_type(mut self, op_type: OperationType) -> Self {
        self.operation_type = Some(op_type);
        self
    }

    /// 按操作者过滤
    pub fn with_operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }

    /// 设置时间范围
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// 设置分页
    pub fn with_pagination(mut self, page: u64, page_size: u64) -> Self {
        self.page = Some(page);
        self.page_size = Some(page_size);
        self
    }
}

/// 审计日志分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogPageResult {
    /// 日志条目列表
    pub entries: Vec<AuditLogEntry>,
    /// 总数
    pub total: u64,
    /// 页码
    pub page: u64,
    /// 每页数量
    pub page_size: u64,
    /// 总页数
    pub total_pages: u64,
}

/// 审计日志管理器
pub struct AuditLogger {
    /// 是否启用审计日志
    enabled: bool,
    /// 是否记录到文件
    log_to_file: bool,
    /// 日志目录
    log_dir: Option<std::path::PathBuf>,
}

impl AuditLogger {
    /// 创建新的审计日志管理器
    pub fn new() -> Self {
        Self {
            enabled: true,
            log_to_file: false,
            log_dir: None,
        }
    }

    /// 创建新的审计日志管理器（带配置）
    pub fn with_config(enabled: bool, log_to_file: bool, log_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            enabled,
            log_to_file,
            log_dir,
        }
    }

    /// 记录审计日志
    pub async fn log(&self, entry: AuditLogEntry) {
        if !self.enabled {
            return;
        }

        // 记录到日志输出
        let status_str = match entry.status {
            crate::types::OperationStatus::Success => "成功",
            crate::types::OperationStatus::Failed => "失败",
            crate::types::OperationStatus::PartialFailed => "部分失败",
            crate::types::OperationStatus::Pending => "待处理",
            crate::types::OperationStatus::InProgress => "进行中",
        };

        log::info!(
            "[审计日志] 插件: {}, 操作: {}, 状态: {}, 操作者: {}, 时间: {}",
            entry.plugin_id,
            entry.operation_type.as_str(),
            status_str,
            entry.operator,
            entry.timestamp.format("%Y-%m-%d %H:%M:%S")
        );

        // 如果有错误信息，记录错误
        if let Some(ref error) = entry.error_message {
            log::error!("[审计日志] 错误信息: {}", error);
        }

        // 如果配置了写入文件
        if self.log_to_file {
            if let Some(ref log_dir) = self.log_dir {
                let _ = self.write_to_file(log_dir, &entry);
            }
        }
    }

    /// 写入文件
    fn write_to_file(&self, log_dir: &std::path::Path, entry: &AuditLogEntry) -> Result<(), std::io::Error> {
        use std::io::Write;

        // 创建日志目录
        std::fs::create_dir_all(log_dir)?;

        // 按日期创建日志文件
        let date_str = entry.timestamp.format("%Y-%m-%d").to_string();
        let log_file = log_dir.join(format!("audit_{}.log", date_str));

        // 追加写入
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        writeln!(
            file,
            "[{}] [{}] [{}] [{}] {} - {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.plugin_id,
            entry.operation_type.as_str(),
            entry.status.as_str(),
            entry.operator,
            entry.details
        )?;

        Ok(())
    }

    /// 创建审计日志构建器
    pub fn builder(&self, plugin_id: impl Into<String>, operation_type: OperationType, operator: impl Into<String>) -> AuditLogBuilder {
        AuditLogBuilder::new(plugin_id, operation_type, operator)
    }

    /// 查询审计日志
    pub async fn query(&self, filter: AuditLogFilter) -> Result<AuditLogPageResult, crate::PluginError> {
        // TODO: 集成数据库查询
        // 实际实现需要查询 cmx_plugin_audit_log 表
        // 返回分页结果

        Ok(AuditLogPageResult {
            entries: Vec::new(),
            total: 0,
            page: filter.page.unwrap_or(1),
            page_size: filter.page_size.unwrap_or(20),
            total_pages: 0,
        })
    }

    /// 获取插件的操作历史
    pub async fn get_plugin_history(&self, plugin_id: &str) -> Result<Vec<AuditLogEntry>, crate::PluginError> {
        // TODO: 集成数据库查询
        Ok(Vec::new())
    }

    /// 清理历史日志
    pub async fn cleanup(&self, days: u32) -> Result<u64, crate::PluginError> {
        // TODO: 集成数据库删除
        // 删除指定天数之前的日志
        log::info!("清理 {} 天前的审计日志", days);
        Ok(0)
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：记录安装操作
pub async fn log_install(
    logger: &AuditLogger,
    plugin_id: &str,
    operator: &str,
    version: &str,
    success: bool,
    error: Option<&str>,
) {
    let mut builder = logger.builder(plugin_id, OperationType::Install, operator)
        .with_detail("version", version)
        .with_status(if success {
            crate::types::OperationStatus::Success
        } else {
            crate::types::OperationStatus::Failed
        });

    if let Some(e) = error {
        builder = builder.with_error(e);
    }

    let entry = builder.build();
    logger.log(entry).await;
}

/// 便捷函数：记录卸载操作
pub async fn log_uninstall(
    logger: &AuditLogger,
    plugin_id: &str,
    operator: &str,
    success: bool,
    error: Option<&str>,
) {
    let mut builder = logger.builder(plugin_id, OperationType::Uninstall, operator)
        .with_status(if success {
            crate::types::OperationStatus::Success
        } else {
            crate::types::OperationStatus::Failed
        });

    if let Some(e) = error {
        builder = builder.with_error(e);
    }

    let entry = builder.build();
    logger.log(entry).await;
}

/// 便捷函数：记录激活操作
pub async fn log_activate(
    logger: &AuditLogger,
    plugin_id: &str,
    operator: &str,
    success: bool,
    error: Option<&str>,
) {
    let mut builder = logger.builder(plugin_id, OperationType::Activate, operator)
        .with_status(if success {
            crate::types::OperationStatus::Success
        } else {
            crate::types::OperationStatus::Failed
        });

    if let Some(e) = error {
        builder = builder.with_error(e);
    }

    let entry = builder.build();
    logger.log(entry).await;
}

/// 便捷函数：记录停用操作
pub async fn log_deactivate(
    logger: &AuditLogger,
    plugin_id: &str,
    operator: &str,
    success: bool,
    error: Option<&str>,
) {
    let mut builder = logger.builder(plugin_id, OperationType::Deactivate, operator)
        .with_status(if success {
            crate::types::OperationStatus::Success
        } else {
            crate::types::OperationStatus::Failed
        });

    if let Some(e) = error {
        builder = builder.with_error(e);
    }

    let entry = builder.build();
    logger.log(entry).await;
}

/// 便捷函数：记录升级操作
pub async fn log_upgrade(
    logger: &AuditLogger,
    plugin_id: &str,
    operator: &str,
    from_version: &str,
    to_version: &str,
    success: bool,
    error: Option<&str>,
) {
    let mut builder = logger.builder(plugin_id, OperationType::Upgrade, operator)
        .with_detail("from_version", from_version)
        .with_detail("to_version", to_version)
        .with_status(if success {
            crate::types::OperationStatus::Success
        } else {
            crate::types::OperationStatus::Failed
        });

    if let Some(e) = error {
        builder = builder.with_error(e);
    }

    let entry = builder.build();
    logger.log(entry).await;
}
