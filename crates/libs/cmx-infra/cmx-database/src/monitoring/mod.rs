/*
 * @Author: yqs
 * @Date: 2026-03-05 19:30:00
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-05 19:58:24
 */
/// 监控模块，负责数据库连接池健康检查和事务超时监控
use crate::transaction::{check_long_running_transactions, metadata::update_txn_status, registry::{get_txn_holder_by_id, get_txn_holder_registry}, TransactionStatus, TxnHolder};
use crate::{get_default_db_manager};
use tracing::{error, info};


/// 启动数据库连接池健康检查和事务超时监控
pub async fn start_monitoring() {
    // 启动健康检查
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            perform_health_check().await;
        }
    });

    // 启动事务超时监控
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            check_transaction_timeouts().await;
        }
    });
}

/// 执行健康检查
async fn perform_health_check() {
    let db_keys = get_default_db_manager().list_data_sources().await;

    for key in db_keys {
        if let Ok((dbx, config)) = get_default_db_manager().get_db(&key).await {
            let _ = check_db_health(&dbx, &config).await;
        }
    }
}

/// 检查数据库健康状态
async fn check_db_health(dbx: &crate::transaction::Dbx, config: &crate::config::DbConfig) -> crate::Result<()> {
    let timeout = tokio::time::Duration::from_secs(config.health_check_timeout);

    tokio::time::timeout(timeout, async {
        match dbx.db() {
            crate::connection::DbPool::Postgres(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            },
            crate::connection::DbPool::MySql(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            },
            crate::connection::DbPool::Sqlite(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            },
        }
        Ok(())
    }).await.map_err(|_| crate::Error::ConnectionTimeout)?
}

/// 检查事务超时
async fn check_transaction_timeouts() {
    let default_timeout = std::time::Duration::from_secs(300);

    let long_running_txs = check_long_running_transactions(default_timeout).await;

    for tx_meta in long_running_txs {
        info!("检测到长时间运行的事务: txn_id={}, db_id={}, 运行时间={:?}",
              tx_meta.txn_id, tx_meta.db_id, tx_meta.create_time.elapsed());

        // 直接通过 txn_id 从注册表获取事务句柄并回滚
        if let Some(txn_holder_mutex) = get_txn_holder_by_id(&tx_meta.txn_id).await {
            let mut should_rollback = false;
            let mut txn_to_rollback: Option<TxnHolder> = None;

            // 获取锁并取出事务
            {
                let mut txh_g = txn_holder_mutex.lock().await;
                if let Some(txn_holder) = txh_g.take() {
                    txn_to_rollback = Some(txn_holder);
                    should_rollback = true;
                }
            }

            // 执行回滚
            if should_rollback && txn_to_rollback.is_some() {
                let txn = txn_to_rollback.unwrap();
                if let Err(e) = txn.rollback().await {
                    error!("回滚超时事务失败: txn_id={}, error={}", tx_meta.txn_id, e);
                } else {
                    info!("已自动回滚超时事务: txn_id={}", tx_meta.txn_id);
                    // 更新事务状态
                    update_txn_status(&tx_meta.txn_id, TransactionStatus::RolledBack).await;
                    // 从注册表中移除
                    get_txn_holder_registry().write().await.remove(&tx_meta.txn_id);
                }
            }
        }
    }
}
