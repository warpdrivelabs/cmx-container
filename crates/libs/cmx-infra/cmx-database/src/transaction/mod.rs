/// 事务管理模块，负责数据库事务的创建、提交和回滚
///
/// 该模块提供了完整的事务管理功能，包括：
/// - 事务的创建、提交和回滚
/// - 事务传播机制
/// - 事务状态跟踪
/// - 事务元数据管理
/// - 通过事务ID操作事务
/// - 通过数据库ID和事务ID获取底层事务对象

// 子模块
mod core;
pub mod metadata;
pub mod registry;
mod conversion;
mod api;
pub mod txcontext;

// 导出核心类型和函数
pub use core::{Dbx, DbTransaction, TxnHolder, Propagation};

// 导出元数据相关类型和函数
pub use metadata::{TransactionStatus, TransactionMetadata, register_txn, update_txn_status, get_txn_metadata, get_active_transactions, cleanup_completed_transactions, check_long_running_transactions};

// 导出注册表相关函数
pub use registry::{get_txn_holder_by_id};

// 导出上下文相关类型和函数
pub use txcontext::{TransactionFrame, TransactionContextStack, SuspendedTransaction};

// 导出API相关函数
pub use api::{commit_txn_by_id, rollback_txn_by_id, get_dbx_by_db_id, with_transaction_by_id, execute_sql_by_ids, execute_sql_with_params_by_ids, query_sql_by_ids, query_sql_with_params_by_ids};

// 声明式事务管理宏
///
/// 提供了一种简洁的方式来管理事务，自动处理事务的开始、提交和回滚
#[macro_export]
macro_rules! transaction {
    // 基本用法，使用默认传播行为
    ($db_id:expr, $dbx:expr, $body:expr) => {
        async {
            // 开始事务，使用默认传播行为
            let txn_id = $dbx.begin_txn_default($db_id).await?;
            // 执行事务体
            let result = $body.await;
            // 根据执行结果决定提交或回滚
            match result {
                Ok(value) => {
                    // 提交事务
                    $dbx.commit_txn().await?;
                    Ok(value)
                },
                Err(err) => {
                    // 回滚事务
                    $dbx.rollback_txn().await.ok();
                    Err(err)
                }
            }
        }
    };

    // 带传播行为的事务宏
    ($db_id:expr, $dbx:expr, $propagation:expr, $body:expr) => {
        async {
            // 开始事务，使用指定的传播行为
            let txn_id = $dbx.begin_txn($db_id, $propagation).await?;
            // 执行事务体
            let result = $body.await;
            // 根据执行结果决定提交或回滚
            match result {
                Ok(value) => {
                    // 提交事务
                    $dbx.commit_txn().await?;
                    Ok(value)
                },
                Err(err) => {
                    // 回滚事务
                    $dbx.rollback_txn().await.ok();
                    Err(err)
                }
            }
        }
    };
}
