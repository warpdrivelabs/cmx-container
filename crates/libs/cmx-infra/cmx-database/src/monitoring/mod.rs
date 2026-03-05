/*
 * @Author: yqs
 * @Date: 2026-03-05 19:30:00
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-05 19:58:24
 */
/// 监控模块，负责数据库连接池健康检查和事务超时监控

use crate::connection::{get_db_access, get_registry};
use crate::transaction::check_long_running_transactions;
use tracing::info;

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
    let registry = get_registry();
    let db_keys: Vec<String> = registry.read().unwrap().keys().cloned().collect();
    
    for key in db_keys {
        let db_entry = {
            let registry_read = registry.read().unwrap();
            registry_read.get(&key).cloned()
        };
        
        if let Some((dbx, config)) = db_entry {
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
            // DbPool::MySql(pool) => {
            //     sqlx::query("SELECT 1").execute(pool).await?;
            // },
            // DbPool::Sqlite(pool) => {
            //     sqlx::query("SELECT 1").execute(pool).await?;
            // },
        }
        Ok(())
    }).await.map_err(|_| crate::Error::ConnectionTimeout)?
}

/// 检查事务超时
async fn check_transaction_timeouts() {
    // 默认事务超时时间：300秒（5分钟）
    let default_timeout = std::time::Duration::from_secs(300);
    
    let long_running_txs = check_long_running_transactions(default_timeout);
    
    for tx_meta in long_running_txs {
        info!("检测到长时间运行的事务: txn_id={}, db_id={}, 运行时间={:?}", 
              tx_meta.txn_id, tx_meta.db_id, tx_meta.created_at.elapsed());
        
        // 尝试获取数据库连接并回滚事务
        if let Some(dbx) = get_db_access(&tx_meta.db_id) {
            let _ = dbx.rollback_txn().await;
            info!("已自动回滚超时事务: txn_id={}", tx_meta.txn_id);
        }
    }
}
