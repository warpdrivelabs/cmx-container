use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cmx_core::dv;
use cmx_core::model::cell::DataValue;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

use super::error::{MigrationError, MigrationResult};
use super::loader::MigrationLoader;
use super::record::{
    ChecksumMismatch, FailedMigration, MigrationRecord, MigrationStatus, MigrationSummary,
    PendingMigration, ValidationResult,
};
use crate::manager::DatabaseManager;

/// 迁移锁的 Redis 键名
const MIGRATION_LOCK_KEY: &str = "cmx:database:migration";

/// 迁移运行器
///
/// 负责执行数据库迁移，包括创建迁移表、获取分布式锁、
/// 校验已执行迁移、执行待执行迁移等
pub struct MigrationRunner {
    /// 数据库管理器
    db: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
    /// 分布式锁管理器（可选）
    lock_manager: Option<Arc<cmx_buffer::LockManager>>,
    /// 迁移文件目录路径
    migration_dir: PathBuf,
    /// 是否启用迁移
    enabled: bool,
    /// 锁超时时间（秒）
    lock_timeout: u64,
    /// 获取锁失败后的等待超时（秒）
    lock_wait_timeout: u64,
    /// 等待锁时的轮询间隔（秒）
    lock_poll_interval: u64,
    /// 是否校验迁移文件校验和
    validate_checksum: bool,
}

impl MigrationRunner {
    /// 创建新的迁移运行器
    ///
    /// # 参数
    /// * `db` - 数据库管理器实例
    /// * `default_db_id` - 默认数据库ID
    /// * `migration_dir` - 迁移文件目录路径
    pub fn new(db: Arc<DatabaseManager>, default_db_id: String, migration_dir: PathBuf) -> Self {
        Self {
            db,
            default_db_id,
            lock_manager: None,
            migration_dir,
            enabled: false,
            lock_timeout: 60,
            lock_wait_timeout: 120,
            lock_poll_interval: 3,
            validate_checksum: true,
        }
    }

    /// 设置分布式锁管理器
    ///
    /// # 参数
    /// * `lock_manager` - 分布式锁管理器实例
    pub fn with_lock_manager(mut self, lock_manager: Arc<cmx_buffer::LockManager>) -> Self {
        self.lock_manager = Some(lock_manager);
        self
    }

    /// 设置锁超时时间
    ///
    /// # 参数
    /// * `timeout` - 锁超时时间（秒）
    pub fn with_lock_timeout(mut self, timeout: u64) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// 设置锁等待超时时间
    ///
    /// # 参数
    /// * `timeout` - 获取锁失败后的等待超时（秒），默认120秒
    pub fn with_lock_wait_timeout(mut self, timeout: u64) -> Self {
        self.lock_wait_timeout = timeout;
        self
    }

    /// 设置锁轮询间隔
    ///
    /// # 参数
    /// * `interval` - 轮询间隔（秒），默认3秒
    pub fn with_lock_poll_interval(mut self, interval: u64) -> Self {
        self.lock_poll_interval = interval;
        self
    }

    /// 设置是否校验迁移文件校验和
    ///
    /// # 参数
    /// * `validate` - 是否校验，默认为 true
    ///
    /// 设置为 false 时，只要 cmx_schema_migrations 表中有记录，
    /// 就认为该迁移已执行，不会再校验文件内容是否被修改
    pub fn with_validate_checksum(mut self, validate: bool) -> Self {
        self.validate_checksum = validate;
        self
    }

    /// 设置是否启用迁移
    ///
    /// # 参数
    /// * `enabled` - 是否启用，默认为 false
    ///
    /// 设置为 false 时，跳过迁移执行，直接返回
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 执行所有待执行的迁移
    ///
    /// 流程：
    /// 1. 检查是否启用迁移
    /// 2. 确保迁移表存在
    /// 3. 尝试获取分布式锁
    /// 4. 加载迁移文件
    /// 5. 查询已执行迁移
    /// 6. 校验和验证
    /// 7. 过滤待执行迁移
    /// 8. 依次执行
    /// 9. 返回执行摘要
    pub async fn run_pending_migrations(&self) -> MigrationResult<MigrationSummary> {
        // 0. 检查是否启用迁移
        if !self.enabled {
            debug!("数据库迁移已禁用，跳过迁移执行");
            return Ok(MigrationSummary {
                executed_count: 0,
                skipped_count: 0,
                failed: Vec::new(),
            });
        }

        // 1. 确保迁移表存在
        self.ensure_migration_table().await?;

        // 2. 尝试获取分布式锁
        let _lock_guard = match self.try_acquire_migration_lock().await {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => {
                // 锁获取失败，进入等待模式
                let waited = self.wait_for_migration_lock().await;
                if waited {
                    info!("其他节点已完成数据库迁移，本节点继续启动");
                } else {
                    warn!(
                        "等待迁移锁超时（{}秒），继续启动（迁移可能仍在进行中）",
                        self.lock_wait_timeout
                    );
                }
                return Ok(MigrationSummary {
                    executed_count: 0,
                    skipped_count: 0,
                    failed: Vec::new(),
                });
            }
            Err(e) => {
                warn!("获取迁移锁失败: {:?}，继续执行（单机模式）", e);
                None
            }
        };

        // 3. 加载迁移文件
        let loader = MigrationLoader::new(self.migration_dir.clone());
        let all_migrations = loader.load_migrations()?;

        if all_migrations.is_empty() {
            info!("没有找到迁移文件");
            return Ok(MigrationSummary {
                executed_count: 0,
                skipped_count: 0,
                failed: Vec::new(),
            });
        }

        // 4. 查询已执行迁移
        let executed = self.get_executed_migrations().await?;

        // 分离成功和失败的迁移版本
        let successful_versions: std::collections::HashSet<String> = executed
            .iter()
            .filter(|r| r.status == MigrationStatus::Success)
            .map(|r| r.version.clone())
            .collect();

        let failed_versions: std::collections::HashSet<String> = executed
            .iter()
            .filter(|r| r.status == MigrationStatus::Failed)
            .map(|r| r.version.clone())
            .collect();

        // 5. 校验和验证
        if self.validate_checksum {
            let validation = self.validate_checksums(&all_migrations, &executed)?;
            if !validation.is_valid {
                for mismatch in &validation.mismatches {
                    error!(
                        version = %mismatch.version,
                        recorded = %mismatch.recorded,
                        actual = %mismatch.actual,
                        "校验和不匹配"
                    );
                }
                return Err(MigrationError::ChecksumMismatch {
                    version: validation.mismatches[0].version.clone(),
                    recorded: validation.mismatches[0].recorded.clone(),
                    actual: validation.mismatches[0].actual.clone(),
                });
            }
        } else {
            debug!("已跳过迁移文件校验和验证");
        }

        // 6. 过滤待执行迁移
        // 跳过已成功的迁移，但重新执行失败的迁移
        let pending: Vec<&PendingMigration> = all_migrations
            .iter()
            .filter(|m| !successful_versions.contains(&m.version))
            .collect();

        let skipped_count = successful_versions.len();
        let retry_count = failed_versions.len();
        if skipped_count > 0 {
            info!("跳过 {} 个已成功的迁移", skipped_count);
        }
        if retry_count > 0 {
            info!("重新执行 {} 个失败的迁移", retry_count);
        }

        // 7. 依次执行
        let mut executed_count = 0usize;
        let mut failed = Vec::new();

        for migration in &pending {
            let is_retry = failed_versions.contains(&migration.version);
            if is_retry {
                info!(
                    version = %migration.version,
                    name = %migration.name,
                    "重新执行失败的迁移"
                );
            } else {
                info!(
                    version = %migration.version,
                    name = %migration.name,
                    "开始执行迁移"
                );
            }

            match self.execute_migration(migration).await {
                Ok(duration_ms) => {
                    info!(
                        version = %migration.version,
                        name = %migration.name,
                        duration_ms = duration_ms,
                        "迁移执行成功"
                    );
                    executed_count += 1;
                }
                Err(e) => {
                    error!(
                        version = %migration.version,
                        name = %migration.name,
                        error = %e,
                        "迁移执行失败"
                    );
                    failed.push(FailedMigration {
                        version: migration.version.clone(),
                        name: migration.name.clone(),
                        error: e.to_string(),
                    });
                    // 遇到失败停止后续迁移
                    break;
                }
            }
        }

        // 8. 返回摘要
        Ok(MigrationSummary {
            executed_count,
            skipped_count,
            failed,
        })
    }

    /// 确保迁移表存在
    ///
    /// 创建 cmx_schema_migrations 表（如果不存在）
    async fn ensure_migration_table(&self) -> MigrationResult<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS cmx_schema_migrations (
                version VARCHAR(100) NOT NULL PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                checksum VARCHAR(64) NOT NULL,
                status VARCHAR(30) NOT NULL DEFAULT 'pending',
                executed_by VARCHAR(100),
                execution_time_ms BIGINT,
                error_message TEXT,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
            );

            COMMENT ON TABLE cmx_schema_migrations IS '数据库迁移版本记录表';
            COMMENT ON COLUMN cmx_schema_migrations.version IS '迁移版本号';
            COMMENT ON COLUMN cmx_schema_migrations.name IS '迁移名称';
            COMMENT ON COLUMN cmx_schema_migrations.checksum IS '校验和（SHA256）';
            COMMENT ON COLUMN cmx_schema_migrations.status IS '执行状态: pending, success, failed, rolled_back';
            COMMENT ON COLUMN cmx_schema_migrations.executed_by IS '执行节点ID';
            COMMENT ON COLUMN cmx_schema_migrations.execution_time_ms IS '执行耗时（毫秒）';
            COMMENT ON COLUMN cmx_schema_migrations.error_message IS '错误信息';
            COMMENT ON COLUMN cmx_schema_migrations.created_at IS '创建时间';
            COMMENT ON COLUMN cmx_schema_migrations.updated_at IS '更新时间';
        "#;

        self.execute_sql_statements(sql).await?;

        info!("迁移表 cmx_schema_migrations 已就绪");
        Ok(())
    }

    /// 尝试获取迁移分布式锁
    ///
    /// 使用 LockManager.try_lock() 非阻塞获取迁移锁。
    ///
    /// # 返回值
    /// * `Ok(Some(LockGuard))` - 成功获取锁
    /// * `Ok(None)` - 其他节点持有锁
    /// * `Err` - 锁管理器不可用
    async fn try_acquire_migration_lock(&self) -> MigrationResult<Option<cmx_buffer::LockGuard>> {
        let lock_manager = match &self.lock_manager {
            Some(lm) => lm,
            None => {
                debug!("未配置分布式锁管理器，跳过锁获取");
                return Ok(None);
            }
        };

        match lock_manager.try_lock(MIGRATION_LOCK_KEY).await {
            Ok(Some(guard)) => {
                info!("成功获取数据库迁移分布式锁");
                Ok(Some(guard))
            }
            Ok(None) => {
                info!("其他节点正在执行数据库迁移，获取锁失败，进入等待模式");
                Ok(None)
            }
            Err(e) => {
                warn!("检查迁移锁失败: {:?}", e);
                Err(MigrationError::LockAcquireFailed)
            }
        }
    }

    /// 等待迁移锁释放（轮询模式）
    ///
    /// 轮询检查锁是否释放，最多等待 lock_wait_timeout 秒。
    /// 等到锁释放后返回 true（不再执行迁移），超时返回 false。
    async fn wait_for_migration_lock(&self) -> bool {
        let lock_manager = match &self.lock_manager {
            Some(lm) => lm,
            None => return true,
        };

        let timeout = Duration::from_secs(self.lock_wait_timeout);
        let poll_interval = Duration::from_secs(self.lock_poll_interval);
        let start = Instant::now();

        info!(
            "等待其他节点完成数据库迁移（超时: {}秒，轮询间隔: {}秒）",
            self.lock_wait_timeout, self.lock_poll_interval
        );

        while start.elapsed() < timeout {
            sleep(poll_interval).await;

            match lock_manager.is_locked(MIGRATION_LOCK_KEY).await {
                Ok(false) => {
                    info!("其他节点已完成数据库迁移，锁已释放");
                    return true;
                }
                Ok(true) => {
                    debug!(
                        "迁移锁仍被持有，继续等待（已等待 {}秒）",
                        start.elapsed().as_secs()
                    );
                }
                Err(e) => {
                    warn!("轮询迁移锁状态失败: {:?}", e);
                }
            }
        }

        false
    }

    /// 查询已执行的迁移记录
    async fn get_executed_migrations(&self) -> MigrationResult<Vec<MigrationRecord>> {
        let sql = "SELECT version, name, checksum, status, executed_by, execution_time_ms, error_message, created_at, updated_at FROM cmx_schema_migrations ORDER BY version";

        let dataset = self
            .db
            .query_sql(&self.default_db_id, None, sql, "schema_migrations")
            .await
            .map_err(|e| MigrationError::QueryFailed(e.to_string()))?;

        let mut records = Vec::new();
        let schema = dataset.schema.as_ref();

        // 从 DataSet 中解析迁移记录
        for row in &dataset.rows {
            /// 从行中获取字符串值
            fn get_string(
                row: &cmx_core::model::data::dataset::Row,
                schema: &cmx_core::model::data::dataset::Schema,
                col_name: &str,
            ) -> Option<String> {
                row.get_by_name(schema, col_name).and_then(|v| match v {
                    cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                    cmx_core::model::cell::DataValue::ShortStr(s) => Some(s.to_string()),
                    cmx_core::model::cell::DataValue::LongStr(s) => Some(s.to_string()),
                    cmx_core::model::cell::DataValue::Int(i) => Some(i.to_string()),
                    cmx_core::model::cell::DataValue::Null => None,
                    _ => {
                        // 尝试 TryFrom 转换
                        <String as TryFrom<cmx_core::model::cell::DataValue>>::try_from(v.clone())
                            .ok()
                    }
                })
            }

            let version = get_string(row, schema, "version").unwrap_or_default();
            let name = get_string(row, schema, "name").unwrap_or_default();
            let checksum = get_string(row, schema, "checksum").unwrap_or_default();
            let status_str =
                get_string(row, schema, "status").unwrap_or_else(|| "pending".to_string());
            let status: MigrationStatus = status_str.parse().unwrap_or(MigrationStatus::Pending);
            let executed_by = get_string(row, schema, "executed_by").unwrap_or_default();
            let execution_time_ms =
                get_string(row, schema, "execution_time_ms").and_then(|s| s.parse::<i64>().ok());
            let error_message = get_string(row, schema, "error_message");
            let created_at = get_string(row, schema, "created_at");
            let updated_at = get_string(row, schema, "updated_at");

            records.push(MigrationRecord {
                version,
                name,
                checksum,
                status,
                executed_by,
                execution_time_ms,
                error_message,
                created_at,
                updated_at,
            });
        }

        debug!("查询到 {} 条已执行迁移记录", records.len());
        Ok(records)
    }

    /// 校验已执行迁移的校验和
    ///
    /// 对比数据库中记录的校验和与当前文件的校验和，
    /// 如果不匹配说明迁移文件被修改
    fn validate_checksums(
        &self,
        migrations: &[PendingMigration],
        executed: &[MigrationRecord],
    ) -> MigrationResult<ValidationResult> {
        let migration_map: std::collections::HashMap<&str, &PendingMigration> =
            migrations.iter().map(|m| (m.version.as_str(), m)).collect();

        let mut mismatches = Vec::new();

        for record in executed {
            if record.status != MigrationStatus::Success {
                continue;
            }

            if let Some(migration) = migration_map.get(record.version.as_str())
                && record.checksum != migration.checksum
            {
                mismatches.push(ChecksumMismatch {
                    version: record.version.clone(),
                    recorded: record.checksum.clone(),
                    actual: migration.checksum.clone(),
                });
            }
        }

        Ok(ValidationResult {
            is_valid: mismatches.is_empty(),
            mismatches,
        })
    }

    /// 执行单个迁移
    ///
    /// 在事务中执行 SQL 语句，成功则提交事务并记录迁移结果，失败则回滚事务
    async fn execute_migration(&self, migration: &PendingMigration) -> MigrationResult<i64> {
        let start = Instant::now();

        // 开启事务执行迁移 SQL
        let txn_result: MigrationResult<crate::transaction::TransactionGuard> = async {
            let guard = crate::transaction::begin_transaction_guard_by_db_id(
                &self.default_db_id,
                crate::manager::TransactionOptions::default(),
            )
            .await
            .map_err(|e| MigrationError::SqlExecutionError(format!("开启事务失败: {}", e)))?;
            let txn_id = guard.txn_id().to_string();

            self.execute_sql_statements_with_txn(&migration.up_sql, Some(&txn_id))
                .await?;

            Ok(guard)
        }
        .await;

        match txn_result {
            Ok(guard) => {
                // 提交事务
                guard.commit().await.map_err(|e| {
                    MigrationError::SqlExecutionError(format!("提交事务失败: {}", e))
                })?;

                let duration_ms = start.elapsed().as_millis() as i64;

                // 记录迁移成功（事务外独立写入）
                self.record_migration(migration, MigrationStatus::Success, duration_ms, None)
                    .await?;

                Ok(duration_ms)
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as i64;
                let error_msg = e.to_string();

                // 事务已在 RAII 中自动回滚，记录迁移失败状态（事务外独立写入）
                self.record_migration(
                    migration,
                    MigrationStatus::Failed,
                    duration_ms,
                    Some(&error_msg),
                )
                .await
                .unwrap_or_else(|record_err| {
                    error!("记录迁移失败状态时出错: {:?}", record_err);
                });

                Err(e)
            }
        }
    }

    /// 在事务中执行 SQL 语句
    ///
    /// 将 SQL 按分号分割（跳过单引号字符串内的分号），逐条在指定事务中执行
    async fn execute_sql_statements_with_txn(
        &self,
        sql: &str,
        txn_id: Option<&str>,
    ) -> MigrationResult<()> {
        let statements = split_sql_statements(sql);

        for statement in statements {
            let trimmed = statement.trim();
            if trimmed.is_empty() {
                continue;
            }

            let is_only_comments = trimmed
                .lines()
                .all(|line| line.trim().is_empty() || line.trim().starts_with("--"));

            if is_only_comments {
                continue;
            }

            let full_sql = format!("{};", trimmed);

            debug!(sql = %full_sql.chars().take(200).collect::<String>(), "执行 SQL 语句");

            self.db
                .execute_sql(&self.default_db_id, txn_id, &full_sql)
                .await
                .map_err(|e| MigrationError::SqlExecutionError(e.to_string()))?;
        }

        Ok(())
    }

    /// 执行 SQL 语句（无事务，用于建表等操作）
    async fn execute_sql_statements(&self, sql: &str) -> MigrationResult<()> {
        self.execute_sql_statements_with_txn(sql, None).await
    }

    /// 记录迁移执行结果
    ///
    /// 使用 INSERT ... ON CONFLICT UPDATE 模式写入迁移记录
    async fn record_migration(
        &self,
        migration: &PendingMigration,
        status: MigrationStatus,
        execution_time_ms: i64,
        error_message: Option<&str>,
    ) -> MigrationResult<()> {
        let sql = r#"
            INSERT INTO cmx_schema_migrations (version, name, checksum, status, executed_by, execution_time_ms, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (version) DO UPDATE SET
                status = EXCLUDED.status,
                checksum = EXCLUDED.checksum,
                executed_by = EXCLUDED.executed_by,
                execution_time_ms = EXCLUDED.execution_time_ms,
                error_message = EXCLUDED.error_message,
                updated_at = NOW()
        "#;

        let params: Vec<DataValue> = dv![
            migration.version.clone(),
            migration.name.clone(),
            migration.checksum.clone(),
            status.to_string(),
            "".to_string(),
            execution_time_ms,
            error_message
        ];

        self.db
            .execute_sql_with_datavalues(&self.default_db_id, None, sql, params)
            .await
            .map_err(|e| MigrationError::SqlExecutionError(e.to_string()))?;

        Ok(())
    }

    /// 回滚指定版本的迁移
    ///
    /// 查找已执行的迁移记录，执行对应的 down SQL，更新状态为 rolled_back
    pub async fn rollback_migration(&self, version: &str) -> MigrationResult<()> {
        // 查询已执行迁移
        let executed = self.get_executed_migrations().await?;

        // 查找指定版本的记录
        let record = executed
            .iter()
            .find(|r| r.version == version)
            .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

        // 检查状态是否允许回滚
        if record.status != MigrationStatus::Success {
            return Err(MigrationError::InvalidRollbackState(format!(
                "迁移 {} 当前状态为 {}，只有成功状态的迁移才能回滚",
                version, record.status
            )));
        }

        // 加载迁移文件查找 down SQL
        let loader = MigrationLoader::new(self.migration_dir.clone());
        let migrations = loader.load_migrations()?;

        let migration = migrations
            .iter()
            .find(|m| m.version == version)
            .ok_or_else(|| MigrationError::MigrationNotFound(version.to_string()))?;

        let down_sql = migration
            .down_sql
            .as_ref()
            .ok_or_else(|| MigrationError::NoRollbackScript(version.to_string()))?;

        // 执行回滚 SQL
        self.execute_sql_statements(down_sql).await?;

        // 更新状态为 rolled_back
        let update_sql =
            "UPDATE cmx_schema_migrations SET status = $1, updated_at = NOW() WHERE version = $2";
        let params: Vec<DataValue> = dv!["rolled_back", version];

        self.db
            .execute_sql_with_datavalues(&self.default_db_id, None, update_sql, params)
            .await
            .map_err(|e| MigrationError::SqlExecutionError(e.to_string()))?;

        info!(version = %version, "迁移已回滚");
        Ok(())
    }
}

/// 按分号分割 SQL 语句（感知单引号字符串边界）
///
/// 遍历 SQL 文本，跟踪单引号状态，只在单引号外的分号处分割语句。
/// 支持单引号内转义（连续两个单引号 '' 表示一个单引号字面量）。
///
/// # 参数
/// * `sql` - 完整的 SQL 文本
///
/// # 返回值
/// 分割后的 SQL 语句片段列表（不含尾部分号）
fn split_sql_statements(sql: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut last_pos = 0;

    let mut iter = sql.char_indices().peekable();

    while let Some((byte_pos, ch)) = iter.next() {
        match ch {
            '\'' => {
                while let Some(&(_, c)) = iter.peek() {
                    iter.next();
                    if c == '\'' {
                        if iter.peek().map(|&(_, c)| c) == Some('\'') {
                            iter.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            ';' => {
                statements.push(&sql[last_pos..byte_pos]);
                last_pos = byte_pos + 1;
            }
            '-' if iter.peek().map(|&(_, c)| c) == Some('-') => {
                iter.next();
                while let Some(&(_, c)) = iter.peek() {
                    if c == '\n' {
                        break;
                    }
                    iter.next();
                }
            }
            _ => {}
        }
    }

    if last_pos < sql.len() {
        let remaining = &sql[last_pos..];
        if !remaining.trim().is_empty() {
            statements.push(remaining);
        }
    }

    statements
}
