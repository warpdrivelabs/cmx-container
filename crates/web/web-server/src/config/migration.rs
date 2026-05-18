//! 数据库迁移模块
//!
//! 提供数据库迁移的自动执行功能，支持分布式锁防止多节点并发执行。

use cmx_buffer::GlobalLockManager;
use cmx_database::get_default_db_manager;
use cmx_database::migration::MigrationRunner;
use cmx_utils::ConfigManager;
use tracing::info;

pub use crate::Error;

/// 初始化数据库迁移。
///
/// 在应用启动时自动执行待执行的数据库迁移，
/// 支持分布式锁防止多节点并发执行。
///
/// # Returns
///
/// * `Ok(())` - 迁移执行成功
/// * `Err(Error::Migration)` - 迁移失败，终止启动
pub async fn init_database_migrations() -> crate::Result<()> {
    let db_manager = get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;
    let migration_dir = ConfigManager::global()
        .get_string("migration.dir")
        .unwrap_or("docs/sql/migrations".to_string());
    let node_id = ConfigManager::global()
        .get_string("node.node_id")
        .unwrap_or("default".to_string());

    let runner = MigrationRunner::new(
        db_manager.clone(),
        default_db_id,
        std::path::PathBuf::from(migration_dir),
        node_id,
    );

    let runner = if GlobalLockManager::is_initialized() {
        runner.with_lock_manager(GlobalLockManager::get().clone())
    } else {
        runner
    };

    let validate_checksum = ConfigManager::global()
        .get_bool("migration.validate_checksum")
        .unwrap_or(true);
    let runner = runner.with_validate_checksum(validate_checksum);

    let summary = runner.run_pending_migrations().await
        .map_err(|e| Error::Migration(format!("数据库迁移执行失败: {}", e)))?;

    info!(
        "数据库迁移完成: 执行={}, 跳过={}, 失败={}",
        summary.executed_count,
        summary.skipped_count,
        summary.failed.len()
    );

    if !summary.failed.is_empty() {
        return Err(Error::Migration(format!(
            "数据库迁移存在失败项: {:?}",
            summary.failed
        )));
    }

    Ok(())
}
