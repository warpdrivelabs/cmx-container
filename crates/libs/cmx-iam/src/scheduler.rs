//! 临时授权清理任务。
//!
//! 定时将过期的临时授权（effective_until < NOW()）状态置为已失效（status = 0）。
//! 使用 tokio::time::interval 调度，失败时仅记录 warn 日志，不阻塞下一轮。
//! 清理后查询失效记录并通过 AuditLogger trait 写入审计日志（按用户分组聚合）。

use std::sync::Arc;

use cmx_audit::{AuditDomain, AuditLogger, AuditRecord, OperationResult};
use cmx_database::DatabaseManager;
use serde_json::Value;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// 启动临时授权过期清理任务。
///
/// # Arguments
///
/// * `mm` - 数据库管理器。
/// * `db_id` - 数据库 ID。
/// * `interval_secs` - 执行间隔（秒）。
/// * `audit_batch_size` - 审计日志批量阈值（超过则只记统计）。
/// * `audit` - 审计日志记录器（可选）。
///
/// # Returns
///
/// 返回 `JoinHandle<()>` 任务句柄，可用于取消任务。
pub fn start_assignment_cleanup(
    mm: Arc<DatabaseManager>,
    db_id: String,
    interval_secs: u64,
    audit_batch_size: u32,
    audit: Option<Arc<dyn AuditLogger>>,
) -> JoinHandle<()> {
    info!(
        "{:<12} - 启动临时授权清理任务，间隔: {} 秒，审计批量阈值: {}",
        "IAM-SCHED", interval_secs, audit_batch_size
    );

    tokio::spawn(async move {
        // 首次启动延迟 60 秒，避免与启动初始化冲突
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        // 第一次 tick 立即返回（但前面已 sleep 60 秒），后续按间隔触发
        interval.tick().await;

        loop {
            interval.tick().await;
            if let Err(e) = run_cleanup_once(&mm, &db_id, audit_batch_size, audit.as_ref()).await {
                warn!("临时授权清理任务执行失败: {}", e);
            }
        }
    })
}

/// 执行一次清理（供测试直接调用）。
///
/// 返回受影响的行数。
///
/// # Arguments
///
/// * `mm` - 数据库管理器。
/// * `db_id` - 数据库 ID。
/// * `audit_batch_size` - 审计日志批量阈值（超过则只记统计）。
/// * `audit` - 审计日志记录器（可选）。
///
/// # Returns
///
/// 成功时返回受影响的行数。
///
/// # Errors
///
/// 当查询过期记录或执行清理 SQL 失败时返回错误字符串。
pub async fn run_cleanup_once(
    mm: &DatabaseManager,
    db_id: &str,
    audit_batch_size: u32,
    audit: Option<&Arc<dyn AuditLogger>>,
) -> Result<u64, String> {
    // 1. 查询即将过期的记录（用于审计）
    let query_sql = r#"
        SELECT id, user_id, role_id
        FROM cmx_user_role_assignment
        WHERE effective_until < NOW() AND status = 1 AND archived = 0
    "#;
    let params = Value::Array(vec![]);
    let dataset = mm
        .query_sql_with_json(db_id, None, query_sql, params, "cleanup_query_expired")
        .await
        .map_err(|e| format!("查询过期记录失败: {e}"))?;

    let schema = dataset.schema.as_ref();
    let expired_records: Vec<(String, String, String)> = dataset
        .iter()
        .filter_map(|row| {
            Some((
                row.get_by_name_as::<String>(schema, "id")?,
                row.get_by_name_as::<String>(schema, "user_id")?,
                row.get_by_name_as::<String>(schema, "role_id")?,
            ))
        })
        .collect();

    if expired_records.is_empty() {
        return Ok(0);
    }

    // 2. 执行清理
    let sql = r#"
        UPDATE cmx_user_role_assignment
        SET status = 0, update_time = NOW()
        WHERE effective_until < NOW() AND status = 1 AND archived = 0
    "#;
    let params = Value::Array(vec![]);
    let affected = mm
        .execute_sql_with_json(db_id, None, sql, params)
        .await
        .map_err(|e| format!("执行清理SQL失败: {e}"))?;

    if affected > 0 {
        info!(
            "{:<12} - 临时授权清理完成，失效记录数: {}",
            "IAM-SCHED", affected
        );

        // 3. 通过 AuditLogger trait 写审计日志
        if let Some(audit_logger) = audit {
            use std::collections::HashMap;
            let mut user_groups: HashMap<String, Vec<(String, String)>> = HashMap::new();
            for (id, user_id, role_id) in &expired_records {
                user_groups
                    .entry(user_id.clone())
                    .or_default()
                    .push((id.clone(), role_id.clone()));
            }

            let total_users = user_groups.len();
            if (affected as u32) > audit_batch_size {
                // 超过阈值，只记统计
                let detail = serde_json::json!({
                    "expired_count": affected,
                    "user_count": total_users,
                    "sample_ids": expired_records.iter().take(10).map(|(id, _, _)| id).collect::<Vec<_>>(),
                });
                let mut record = AuditRecord::new(
                    AuditDomain::Iam,
                    "temp_role_expired",
                    OperationResult::Success,
                );
                record = record.with_target("user", "batch");
                record = record.with_details(detail);
                if let Err(e) = audit_logger.log(record).await {
                    warn!("写入审计日志失败: {}", e);
                }
            } else {
                // 按用户分组写审计
                for (user_id, items) in user_groups {
                    let detail = serde_json::json!({
                        "assignments": items.iter().map(|(id, role_id)| {
                            serde_json::json!({"assignment_id": id, "role_id": role_id})
                        }).collect::<Vec<_>>(),
                        "expired_at": chrono::Utc::now().to_rfc3339(),
                    });
                    let mut record = AuditRecord::new(
                        AuditDomain::Iam,
                        "temp_role_expired",
                        OperationResult::Success,
                    );
                    record = record.with_target("user", &user_id);
                    record = record.with_details(detail);
                    if let Err(e) = audit_logger.log(record).await {
                        warn!("写入审计日志失败: {}", e);
                    }
                }
            }
        }
    }

    Ok(affected)
}
