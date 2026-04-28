//! 种子数据执行器
//!
//! 批量执行 DML 语句，收集错误，校验数据条数。

use std::path::Path;

use cmx_core::model::cell::TableDefine;
use cmx_database::get_default_db_manager;
use tracing::{info, warn, error, debug};

use crate::MetadataError;
use crate::seed::config::{SeedDataConfig, SeedDataTableResult, SeedDataFailure, SeedDataSummary};
use crate::seed::loader::load_seed_data;
use crate::seed::dml::{generate_pg_insert_or_upsert, generate_pg_single_insert_or_upsert};

/// 默认批次大小
const DEFAULT_BATCH_SIZE: usize = 100;

/// PostgreSQL 种子数据执行器
pub struct PgSeedDataExecutor {
    /// 数据库ID
    db_id: String,
    /// 事务ID（可选）
    txn_id: Option<String>,
    /// 批次大小
    batch_size: usize,
}

impl PgSeedDataExecutor {
    /// 创建执行器实例
    pub fn new(db_id: impl Into<String>, txn_id: Option<String>) -> Self {
        Self {
            db_id: db_id.into(),
            txn_id,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    /// 创建执行器实例（可指定批次大小）
    pub fn with_batch_size(db_id: impl Into<String>, txn_id: Option<String>, batch_size: usize) -> Self {
        Self {
            db_id: db_id.into(),
            txn_id,
            batch_size: batch_size.max(1),
        }
    }

    /// 批量执行多个表的种子数据
    ///
    /// # 参数
    /// - `table_defines`: 所有表定义列表
    /// - `seed_configs`: 种子数据配置列表
    /// - `base_path`: 插件安装根路径
    ///
    /// # 返回
    /// 执行汇总结果
    pub async fn execute_all_seed_data(
        &self,
        table_defines: &[TableDefine],
        seed_configs: &[SeedDataConfig],
        base_path: &Path,
    ) -> SeedDataSummary {
        let start = std::time::Instant::now();
        let mut table_results = Vec::with_capacity(seed_configs.len());

        // 构建表名 → TableDefine 的映射
        let table_map: std::collections::HashMap<&str, &TableDefine> = table_defines
            .iter()
            .map(|t| (t.table_name.as_str(), t))
            .collect();

        for config in seed_configs {
            if !config.enabled {
                debug!("跳过已禁用的种子数据配置: 表={}, 文件={}", config.table_name, config.file);
                continue;
            }

            let table_define = match table_map.get(config.table_name.as_str()) {
                Some(td) => td,
                None => {
                    warn!("种子数据配置中的表 '{}' 在表定义中不存在，跳过", config.table_name);
                    continue;
                }
            };

            info!("开始执行种子数据: 表={}, 文件={}", config.table_name, config.file);

            let result = self
                .execute_seed_data(table_define, config, base_path)
                .await;

            table_results.push(result);
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;

        SeedDataSummary {
            table_results,
            total_duration_ms,
        }
    }

    /// 执行单表的种子数据初始化
    ///
    /// # 执行流程
    /// 1. 检查数据文件是否存在
    /// 2. 加载数据文件（支持 JSON 和 CSV）
    /// 3. 按批次执行 INSERT/UPSERT 语句
    /// 4. 批次失败时降级为逐行执行
    /// 5. 执行后校验数据条数一致性
    ///
    /// # 错误处理策略
    /// - 批次执行失败时，自动降级为逐行执行，提高容错性
    /// - 单行执行失败时记录到 failures 列表，不阻断其他行
    /// - 加载文件失败、SQL 生成失败等记录到 failures
    ///
    /// # 参数
    /// * `table_define` - 目标表的完整定义
    /// * `seed_config` - 种子数据配置
    /// * `base_path` - 插件安装根路径
    pub async fn execute_seed_data(
        &self,
        table_define: &TableDefine,
        seed_config: &SeedDataConfig,
        base_path: &Path,
    ) -> SeedDataTableResult {
        let file_path = base_path.join(&seed_config.file);

        // ============================================
        // 步骤1：检查文件是否存在
        // ============================================
        if !file_path.exists() {
            return SeedDataTableResult::new_load_failure(
                table_define.table_name.clone(),
                seed_config.file.clone(),
                &format!("文件不存在: {:?}", file_path),
            );
        }

        // ============================================
        // 步骤2：加载数据文件
        // ============================================
        let result = load_seed_data(&file_path, &table_define.columns);
        let rows = match result {
            Ok(rows) => rows,
            Err(e) => {
                return SeedDataTableResult::new_load_failure(
                    table_define.table_name.clone(),
                    seed_config.file.clone(),
                    &e.to_string(),
                );
            }
        };

        let file_row_count = rows.len();
        let mut table_result = SeedDataTableResult::new(
            table_define.table_name.clone(),
            seed_config.file.clone(),
        );
        table_result.file_row_count = file_row_count;

        if file_row_count == 0 {
            info!("种子数据文件为空: {}", seed_config.file);
            return table_result;
        }

        let schema = table_define.schema.as_deref();
        let table_name = &table_define.table_name;
        let columns = &table_define.columns;
        let conflict_cols = &seed_config.conflict_columns;

        let mut success_count = 0usize;
        let mut failures = Vec::new();

        // ============================================
        // 步骤3：按批次执行数据
        // ============================================
        let batches = rows.chunks(self.batch_size);
        for (batch_idx, batch) in batches.enumerate() {
            info!(
                "执行批次 {}/{}: 表={}, {} 行",
                batch_idx + 1,
                rows.len().div_ceil(self.batch_size),
                table_name,
                batch.len()
            );

            match generate_pg_insert_or_upsert(
                table_name,
                schema,
                columns,
                batch,
                conflict_cols,
            ) {
                Ok(sql) => {
                    // 尝试批次执行
                    match self.execute_sql(&sql).await {
                        Ok(_) => {
                            success_count += batch.len();
                        }
                        Err(batch_err) => {
                            // 批次执行失败，降级为逐行执行
                            warn!("批次 {} 执行失败，降级为逐行执行: {}", batch_idx + 1, batch_err);
                            for (row_idx_in_batch, row) in batch.iter().enumerate() {
                                match generate_pg_single_insert_or_upsert(
                                    table_name,
                                    schema,
                                    columns,
                                    row,
                                    conflict_cols,
                                ) {
                                    Ok(single_sql) => {
                                        match self.execute_sql(&single_sql).await {
                                            Ok(_) => {
                                                success_count += 1;
                                            }
                                            Err(single_err) => {
                                                failures.push(SeedDataFailure {
                                                    row_index: batch_idx * self.batch_size + row_idx_in_batch + 1,
                                                    row_data: row.clone(),
                                                    error_message: single_err.to_string(),
                                                });
                                            }
                                        }
                                    }
                                    Err(gen_err) => {
                                        failures.push(SeedDataFailure {
                                            row_index: batch_idx * self.batch_size + row_idx_in_batch + 1,
                                            row_data: row.clone(),
                                            error_message: format!("生成 SQL 失败: {}", gen_err),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Err(gen_err) => {
                    // SQL 生成失败，记录所有行的失败信息
                    error!("批次 {} SQL 生成失败: {}", batch_idx + 1, gen_err);
                    for (row_idx_in_batch, row) in batch.iter().enumerate() {
                        failures.push(SeedDataFailure {
                            row_index: batch_idx * self.batch_size + row_idx_in_batch + 1,
                            row_data: row.clone(),
                            error_message: format!("SQL 生成失败: {}", gen_err),
                        });
                    }
                }
            }
        }

        table_result.success_count = success_count;
        table_result.failed_count = failures.len();
        table_result.failures = failures;

        // ============================================
        // 步骤4：校验数据条数一致性
        // ============================================
        table_result.db_row_count = self.verify_row_count(table_name).await;

        if let Some(db_count) = table_result.db_row_count {
            if db_count < file_row_count {
                // 数据库条数少于文件条数，可能有部分数据执行失败
                warn!(
                    "种子数据条数不一致: 表={}, 文件={}条, 数据库={}条, 可能部分数据执行失败",
                    table_name, file_row_count, db_count
                );
            } else if db_count == file_row_count {
                // 完全一致，执行成功
                debug!("种子数据条数一致: 表={}, 数据库={}条", table_name, db_count);
            } else {
                // 数据库条数多于文件，可能是历史数据
                info!(
                    "种子数据条数: 表={}, 文件={}条, 数据库={}条 (数据库可能已有历史数据)",
                    table_name, file_row_count, db_count
                );
            }
        }

        table_result
    }

    /// 执行 SQL 语句
    async fn execute_sql(&self, sql: &str) -> Result<u64, MetadataError> {
        get_default_db_manager()
            .execute_sql(&self.db_id, self.txn_id.as_deref(), sql)
            .await
            .map_err(|e| MetadataError::SeedData(format!("执行 DML 失败: {}", e)))
    }

    /// 查询表中的行数
    async fn verify_row_count(&self, table_name: &str) -> Option<usize> {
        let count_sql = format!(
            "SELECT COUNT(*) FROM \"{}\"",
            table_name
        );

        match get_default_db_manager()
            .query_sql(&self.db_id, self.txn_id.as_deref(), &count_sql, "count")
            .await
        {
            Ok(ds) => {
                if let Some(row) = ds.rows.first()
                    && let Some(val) = row.get(0) {
                        return match val {
                            cmx_core::model::cell::DataValue::Int(v) => Some(*v as usize),
                            cmx_core::model::cell::DataValue::String(s) => s.parse().ok(),
                            _ => None,
                        };
                    }
                None
            }
            Err(e) => {
                warn!("查询表 '{}' 行数失败: {}", table_name, e);
                None
            }
        }
    }
}
