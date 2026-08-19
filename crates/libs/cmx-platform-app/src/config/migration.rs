//! 数据库迁移模块
//!
//! 在应用启动时自动执行待执行的数据库迁移，
//! 支持分布式锁防止多节点并发执行。
//!
//! 迁移目录按目标库分为两轮执行（目录结构见 `docs/sql/v2/README.md`）：
//! - **主库轮**（`<dir>/platform/migrations`）：执行于 `default = true` 的默认库，
//!   收纳全部 `cmx_` 前缀平台表；
//! - **业务库轮**（`<dir>/biz/migrations`）：执行于 `source_type = "biz"` 的业务库，
//!   收纳非 `cmx_` 前缀业务表（`md_*` / `cf_*` / `cr_*` 等）。
//!   未配置业务库时整轮跳过（不回退主库，避免把业务表建进主库）。

use std::path::PathBuf;

use cmx_buffer::GlobalLockManager;
use cmx_database::get_default_db_manager;
use cmx_database::migration::MigrationRunner;
use cmx_utils::ConfigManager;
use tracing::{info, warn};

pub use crate::Error;

/// 初始化数据库迁移。
///
/// 在应用启动时依次执行主库轮、业务库轮迁移，
/// 支持分布式锁防止多节点并发执行。
///
/// # Returns
///
/// * `Ok(())` - 迁移执行成功
/// * `Err(Error::Migration)` - 迁移失败，终止启动
pub async fn init_database_migrations() -> crate::Result<()> {
    let db_manager = get_default_db_manager();
    let migration_dir = ConfigManager::global()
        .get_string("migration.dir")
        .unwrap_or("docs/sql/v2".to_string());

    let enabled = ConfigManager::global()
        .get_bool("migration.enabled")
        .unwrap_or(false);
    let validate_checksum = ConfigManager::global()
        .get_bool("migration.validate_checksum")
        .unwrap_or(true);
    let lock_wait_timeout = ConfigManager::global()
        .get_int("migration.lock_wait_timeout")
        .unwrap_or(120) as u64;

    // 第一轮：主库（default 数据源）
    let default_db_id = db_manager.get_default_db_id().await;
    run_migration_round(
        &default_db_id,
        migration_round_dir(&migration_dir, "platform"),
        enabled,
        validate_checksum,
        lock_wait_timeout,
        "platform",
    )
    .await?;

    // 第二轮：业务库（source_type = "biz"）；未配置则整轮跳过
    match db_manager.get_biz_db_id_opt().await {
        Some(biz_db_id) => {
            run_migration_round(
                &biz_db_id,
                migration_round_dir(&migration_dir, "biz"),
                enabled,
                validate_checksum,
                lock_wait_timeout,
                "biz",
            )
            .await?;
        }
        None => {
            if enabled {
                warn!("未配置业务库（source_type=\"biz\"），跳过业务库迁移目录 <dir>/biz/migrations");
            }
        }
    }

    Ok(())
}

/// 目标库轮次标签 → 其迁移目录名
const ROUND_DIRS: [&str; 2] = ["platform", "biz"];

/// 解析某轮迁移的迁移目录：`<migration.dir>/<轮次>/migrations`
///
/// MigrationLoader 为**非递归**单目录扫描（仅匹配本层 `*.up.sql`），
/// 故必须拼到 `migrations` 子目录层，不能只拼到 `<dir>/<轮次>`
fn migration_round_dir(migration_dir: &str, label: &str) -> PathBuf {
    debug_assert!(ROUND_DIRS.contains(&label), "未知迁移轮次: {}", label);
    PathBuf::from(migration_dir).join(label).join("migrations")
}

/// 执行单轮迁移（一个目标库 + 其迁移目录）。
///
/// # 参数
///
/// * `db_id` - 目标数据库 id
/// * `dir` - 该轮的迁移目录（内含 `*.up.sql` / `*.down.sql`）
/// * `enabled` - 是否启用迁移（false 时本轮直接跳过）
/// * `validate_checksum` - 是否校验迁移文件校验和
/// * `lock_wait_timeout` - 等待其他节点完成迁移的轮询超时（秒），超时后按台账决定跳过/接管
/// * `label` - 轮次标签（platform / biz），用于日志与锁键区分
async fn run_migration_round(
    db_id: &str,
    dir: PathBuf,
    enabled: bool,
    validate_checksum: bool,
    lock_wait_timeout: u64,
    label: &str,
) -> crate::Result<()> {
    let db_manager = get_default_db_manager();

    let runner = MigrationRunner::new(db_manager.clone(), db_id.to_string(), dir);
    let runner = if GlobalLockManager::is_initialized() {
        runner.with_lock_manager(GlobalLockManager::get().clone())
    } else {
        runner
    };
    let runner = runner
        .with_enabled(enabled)
        .with_validate_checksum(validate_checksum)
        .with_lock_wait_timeout(lock_wait_timeout)
        .with_lock_key(format!("cmx:database:migration:{}", label));

    let summary = runner
        .run_pending_migrations()
        .await
        .map_err(|e| Error::Migration(format!("[{}] 数据库迁移执行失败: {}", label, e)))?;

    info!(
        "[{}] 数据库迁移完成: 执行={}, 跳过={}, 失败={}",
        label, summary.executed_count, summary.skipped_count, summary.failed.len()
    );

    if !summary.failed.is_empty() {
        return Err(Error::Migration(format!(
            "[{}] 数据库迁移存在失败项: {:?}",
            label, summary.failed
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 迁移目录必须拼到 `migrations` 子目录层：MigrationLoader 为非递归单目录
    /// 扫描，只拼到 `<dir>/<轮次>` 会静默漏扫（历史 bug，见 loader.rs 契约测试）
    #[test]
    fn 迁移目录拼接_必须落到migrations子目录() {
        assert_eq!(
            migration_round_dir("docs/sql/v2", "platform"),
            PathBuf::from("docs/sql/v2/platform/migrations")
        );
        assert_eq!(
            migration_round_dir("docs/sql/v2", "biz"),
            PathBuf::from("docs/sql/v2/biz/migrations")
        );
        assert_eq!(
            migration_round_dir("/app/docs/sql/v2", "platform"),
            PathBuf::from("/app/docs/sql/v2/platform/migrations")
        );
    }
}
