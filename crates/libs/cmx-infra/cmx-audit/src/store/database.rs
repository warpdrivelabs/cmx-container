//! PostgreSQL 数据库审计存储
//!
//! 通过 cmx_database::DatabaseManager 提交 sea-query 生成的 SQL。
//! 仅支持 PostgreSQL（与 cmx_plugin_audit_log 风格一致）。

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_query::{Expr, ExprTrait, Iden, Order, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder;

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::record::{AuditDomain, AuditRecord, OperationResult};
use crate::{AuditError, Result};

use super::{AuditFilter, AuditStore};

const TABLE: &str = "cmx_audit_log";

/// 字段标识（用于 sea-query 类型安全的列名引用）。
///
/// > sea-query 1.0 的 `Iden` 派生宏默认将 CamelCase 拆为 snake_case，
/// > 单词变体（如 `Id` → `id`、`Domain` → `domain`）转换结果恰好正确；
/// > 仅 `ResultCol` 需通过 `#[iden = "result"]` 显式覆盖，使其映射到
/// > 实际列名 `result`（而非默认的 `result_col`）。
///
/// 审计跟踪列（`CreateTime`/`UpdateTime`/`CreateBy`/`CreateName`/`UpdateBy`/`UpdateName`）
/// 由数据库 DEFAULT / NULL 填充，代码不显式写入，故此处仅作表结构记录，不产生 SQL 引用。
#[derive(Iden)]
#[allow(dead_code)]
enum Column {
    Id,
    AppId,
    Domain,
    Operation,
    /// 对应表列 `result`（避免与 Rust prelude `Result` 同名，故命名为 ResultCol）
    #[iden = "result"]
    ResultCol,
    ActorId,
    ActorName,
    TargetType,
    TargetId,
    Details,
    RequestId,
    IpAddress,
    StartedAt,
    DurationMs,
    Archived,
    CreateTime,
    UpdateTime,
    CreateBy,
    CreateName,
    UpdateBy,
    UpdateName,
}

/// 数据库审计存储
///
/// 持有构造时指定的 `app_id`，用于：
/// - `save` / `save_batch`：写入时绑定到记录（当前 `AuditRecord` 无 app_id 字段）
/// - `query`：默认按此 app_id 过滤（调用方可通过 `AuditFilter.app_id` 覆盖）
/// - `delete_hard`：默认按此 app_id 限定（安全约束之一）
pub struct DatabaseAuditStore {
    db_manager: Arc<DatabaseManager>,
    default_db_id: String,
    app_id: String,
}

impl DatabaseAuditStore {
    /// 创建数据库审计存储
    ///
    /// # 参数
    /// - `db_manager`: cmx-database 全局单例
    /// - `default_db_id`: 该 store 使用的 db_id（多数据源场景下可选非默认库）
    /// - `app_id`: 多租户/多应用隔离标识，应从业务配置（如 `application.id`）注入，
    ///   与表列 `DEFAULT 'default'` 保持一致
    pub fn new(
        db_manager: Arc<DatabaseManager>,
        default_db_id: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Self {
        Self {
            db_manager,
            default_db_id: default_db_id.into(),
            app_id: app_id.into(),
        }
    }

    pub fn db_id(&self) -> &str {
        &self.default_db_id
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// 按 filter 硬删除审计记录（不可恢复）。
    ///
    /// # 安全约束
    /// 为避免误删全表，filter 必须至少满足以下条件之一：
    /// - `ids` 非空（精确 ID 列表）
    /// - `from` 与 `to` 同时存在（时间窗口）
    /// - `actor_id` 存在
    /// - `target_id` 存在
    /// - `request_id` 存在
    ///
    /// 全部为空时返回 `Err(AuditError::Internal)` 并打 `warn!` 日志。
    /// 此外，删除自动限定在 `self.app_id`（或 `filter.app_id` 覆盖）范围内，
    /// 防止跨应用误删。
    pub async fn delete_hard(&self, filter: &AuditFilter) -> Result<u64> {
        if !Self::filter_has_safety_constraint(filter) {
            tracing::warn!(?filter, "delete_hard 拒绝执行：filter 无安全约束");
            return Err(AuditError::Internal(
                "delete_hard requires ids / time range / actor_id / target_id / request_id".into(),
            ));
        }
        let effective_app_id = filter.app_id.clone().unwrap_or_else(|| self.app_id.clone());

        let mut q = Query::delete();
        q.from_table(TABLE)
            .and_where(Expr::col(Column::AppId).eq(effective_app_id));
        if let Some(ids) = &filter.ids
            && !ids.is_empty()
        {
            q.and_where(Expr::col(Column::Id).is_in(ids.clone()));
        }
        if let Some(a) = &filter.actor_id {
            q.and_where(Expr::col(Column::ActorId).eq(a.clone()));
        }
        if let Some(ti) = &filter.target_id {
            q.and_where(Expr::col(Column::TargetId).eq(ti.clone()));
        }
        if let Some(rid) = &filter.request_id {
            q.and_where(Expr::col(Column::RequestId).eq(rid.clone()));
        }
        if let (Some(from), Some(to)) = (filter.from, filter.to) {
            q.and_where(Expr::col(Column::StartedAt).between(from, to));
        }

        let (sql, params) = q.build_sqlx(PostgresQueryBuilder);
        let affected = self
            .db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, None, &sql, params)
            .await
            .map_err(|e| AuditError::Database(format!("delete audit: {e}")))?;
        Ok(affected)
    }

    fn filter_has_safety_constraint(f: &AuditFilter) -> bool {
        f.ids.as_ref().is_some_and(|v| !v.is_empty())
            || (f.from.is_some() && f.to.is_some())
            || f.actor_id.is_some()
            || f.target_id.is_some()
            || f.request_id.is_some()
    }

    fn domain_to_str(d: &AuditDomain) -> &'static str {
        match d {
            AuditDomain::Auth => "auth",
            AuditDomain::Iam => "iam",
            AuditDomain::Plugin => "plugin",
            AuditDomain::Biz => "biz",
        }
    }

    fn result_to_str(r: &OperationResult) -> &'static str {
        match r {
            OperationResult::Success => "success",
            OperationResult::Failure => "failure",
        }
    }

    fn str_to_domain(s: &str) -> Result<AuditDomain> {
        match s {
            "auth" => Ok(AuditDomain::Auth),
            "iam" => Ok(AuditDomain::Iam),
            "plugin" => Ok(AuditDomain::Plugin),
            "biz" => Ok(AuditDomain::Biz),
            other => Err(AuditError::Internal(format!(
                "unknown audit domain: {other}"
            ))),
        }
    }

    fn str_to_result(s: &str) -> Result<OperationResult> {
        match s {
            "success" => Ok(OperationResult::Success),
            "failure" => Ok(OperationResult::Failure),
            other => Err(AuditError::Internal(format!(
                "unknown operation result: {other}"
            ))),
        }
    }

    /// 解析查询结果集为 `AuditRecord` 列表
    fn parse_records(ds: &DataSet) -> Result<Vec<AuditRecord>> {
        let mut records = Vec::new();
        let schema = ds.schema.as_ref();

        for row in ds.iter() {
            let get_string = |col: &str| -> Option<String> {
                row.get_by_name(schema, col).and_then(|v| match v {
                    DataValue::String(s) => Some(s.clone()),
                    DataValue::ShortStr(s) => Some(s.to_string()),
                    DataValue::LongStr(s) => Some(s.to_string()),
                    _ => None,
                })
            };

            let get_opt_datetime = |col: &str| -> Option<DateTime<Utc>> {
                row.get_by_name(schema, col).and_then(|v| match v {
                    DataValue::String(s) => DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc)),
                    DataValue::DateTime(dt) => Some(*dt),
                    _ => None,
                })
            };

            let get_opt_i64 = |col: &str| -> Option<i64> {
                row.get_by_name(schema, col).and_then(|v| match v {
                    DataValue::Int(n) => Some(*n),
                    _ => None,
                })
            };

            let get_opt_json = |col: &str| -> Option<serde_json::Value> {
                row.get_by_name(schema, col).and_then(|v| match v {
                    // details 为 TEXT 列，可能映射为 String 或 Json(String)
                    DataValue::String(s) => serde_json::from_str(s).ok(),
                    DataValue::Json(s) => serde_json::from_str(s).ok(),
                    DataValue::Null => None,
                    _ => None,
                })
            };

            // domain / result 反序列化失败直接返回错误（不静默降级）
            let domain_str = get_string("domain").unwrap_or_default();
            let domain = Self::str_to_domain(&domain_str)?;
            let result_str = get_string("result").unwrap_or_default();
            let result = Self::str_to_result(&result_str)?;

            let started_at = get_opt_datetime("started_at").unwrap_or_else(Utc::now);

            records.push(AuditRecord {
                id: get_string("id").unwrap_or_default(),
                domain,
                operation: get_string("operation").unwrap_or_default(),
                result,
                actor_id: get_string("actor_id"),
                actor_name: get_string("actor_name"),
                target_type: get_string("target_type"),
                target_id: get_string("target_id"),
                details: get_opt_json("details"),
                request_id: get_string("request_id"),
                ip_address: get_string("ip_address"),
                started_at,
                duration_ms: get_opt_i64("duration_ms"),
            });
        }

        Ok(records)
    }
}

#[async_trait]
impl AuditStore for DatabaseAuditStore {
    async fn save(&self, record: &AuditRecord) -> Result<()> {
        // details 为 TEXT：序列化 Option<JsonValue> -> Option<String>
        let details_str: Option<String> = match record.details.as_ref() {
            Some(v) => Some(serde_json::to_string(v).map_err(AuditError::from)?),
            None => None,
        };

        let mut q = Query::insert();
        q.into_table(TABLE)
            .columns([
                Column::Id,
                Column::AppId,
                Column::Domain,
                Column::Operation,
                Column::ResultCol,
                Column::ActorId,
                Column::ActorName,
                Column::TargetType,
                Column::TargetId,
                Column::Details,
                Column::RequestId,
                Column::IpAddress,
                Column::StartedAt,
                Column::DurationMs,
            ])
            .values([
                record.id.clone().into(),
                self.app_id.clone().into(),
                Self::domain_to_str(&record.domain).into(),
                record.operation.clone().into(),
                Self::result_to_str(&record.result).into(),
                record.actor_id.clone().into(),
                record.actor_name.clone().into(),
                record.target_type.clone().into(),
                record.target_id.clone().into(),
                details_str.into(),
                record.request_id.clone().into(),
                record.ip_address.clone().into(),
                record.started_at.into(),
                record.duration_ms.into(),
            ])
            .map_err(|e| AuditError::Database(format!("build insert: {e}")))?;

        let (sql, params) = q.build_sqlx(PostgresQueryBuilder);
        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, None, &sql, params)
            .await
            .map_err(|e| AuditError::Database(format!("insert audit: {e}")))?;
        Ok(())
    }

    async fn save_batch(&self, records: &[AuditRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        // 多行 INSERT：单条 SQL 一次性提交。
        // 注意：PostgreSQL 单条 INSERT 参数上限约 65535；本表每行 14 字段，
        // 1000 条 = 14000 参数，远低于上限。超过按 1000 条/批拆分。
        const BATCH_CHUNK: usize = 1000;
        for chunk in records.chunks(BATCH_CHUNK) {
            let mut q = Query::insert();
            q.into_table(TABLE).columns([
                Column::Id,
                Column::AppId,
                Column::Domain,
                Column::Operation,
                Column::ResultCol,
                Column::ActorId,
                Column::ActorName,
                Column::TargetType,
                Column::TargetId,
                Column::Details,
                Column::RequestId,
                Column::IpAddress,
                Column::StartedAt,
                Column::DurationMs,
            ]);
            for r in chunk {
                let details_str: Option<String> = match r.details.as_ref() {
                    Some(v) => Some(serde_json::to_string(v).map_err(AuditError::from)?),
                    None => None,
                };
                q.values([
                    r.id.clone().into(),
                    self.app_id.clone().into(),
                    Self::domain_to_str(&r.domain).into(),
                    r.operation.clone().into(),
                    Self::result_to_str(&r.result).into(),
                    r.actor_id.clone().into(),
                    r.actor_name.clone().into(),
                    r.target_type.clone().into(),
                    r.target_id.clone().into(),
                    details_str.into(),
                    r.request_id.clone().into(),
                    r.ip_address.clone().into(),
                    r.started_at.into(),
                    r.duration_ms.into(),
                ])
                .map_err(|e| AuditError::Database(format!("build batch insert: {e}")))?;
            }
            let (sql, params) = q.build_sqlx(PostgresQueryBuilder);
            self.db_manager
                .execute_sql_with_sqlxvalues(&self.default_db_id, None, &sql, params)
                .await
                .map_err(|e| AuditError::Database(format!("batch insert audit: {e}")))?;
        }
        Ok(())
    }

    async fn query(
        &self,
        filter: &AuditFilter,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AuditRecord>> {
        let mut q = Query::select();
        q.from(TABLE)
            .columns([
                Column::Id,
                Column::AppId,
                Column::Domain,
                Column::Operation,
                Column::ResultCol,
                Column::ActorId,
                Column::ActorName,
                Column::TargetType,
                Column::TargetId,
                Column::Details,
                Column::RequestId,
                Column::IpAddress,
                Column::StartedAt,
                Column::DurationMs,
            ])
            .and_where(Expr::col(Column::Archived).eq(0))
            // 默认按 self.app_id 过滤（多租户隔离），调用方可显式覆盖
            .and_where(
                Expr::col(Column::AppId)
                    .eq(filter.app_id.clone().unwrap_or_else(|| self.app_id.clone())),
            )
            .order_by(Column::StartedAt, Order::Desc)
            .limit(limit)
            .offset(offset);
        if let Some(d) = &filter.domain {
            q.and_where(Expr::col(Column::Domain).eq(Self::domain_to_str(d)));
        }
        if let Some(a) = &filter.actor_id {
            q.and_where(Expr::col(Column::ActorId).eq(a.clone()));
        }
        if let Some(tt) = &filter.target_type {
            q.and_where(Expr::col(Column::TargetType).eq(tt.clone()));
        }
        if let Some(ti) = &filter.target_id {
            q.and_where(Expr::col(Column::TargetId).eq(ti.clone()));
        }
        if let Some(rid) = &filter.request_id {
            q.and_where(Expr::col(Column::RequestId).eq(rid.clone()));
        }
        if let Some(r) = &filter.result {
            q.and_where(Expr::col(Column::ResultCol).eq(Self::result_to_str(r)));
        }
        if let Some(f) = filter.from {
            q.and_where(Expr::col(Column::StartedAt).gte(f));
        }
        if let Some(t) = filter.to {
            q.and_where(Expr::col(Column::StartedAt).lte(t));
        }
        if let Some(ids) = &filter.ids
            && !ids.is_empty()
        {
            q.and_where(Expr::col(Column::Id).is_in(ids.clone()));
        }

        let (sql, params) = q.build_sqlx(PostgresQueryBuilder);
        let dataset = self
            .db_manager
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                None,
                &sql,
                params,
                "cmx_audit_log_query",
            )
            .await
            .map_err(|e| AuditError::Database(format!("query audit: {e}")))?;
        Self::parse_records(&dataset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig};
    use serde_json::json;

    const TEST_DB_ID: &str = "audit_test";

    /// 注册独立测试 Pool，避免污染业务库。
    ///
    /// 返回 `None` 表示未配置测试数据库，调用方应跳过用例（不视为失败）。
    async fn setup_store(app_id: &str) -> Option<DatabaseAuditStore> {
        let url = match std::env::var("TEST_DATABASE_URL") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                eprintln!("skip: TEST_DATABASE_URL not set");
                return None;
            }
        };
        let pool_config = PoolConfig {
            max_connections: 5,
            min_connections: 1,
            connect_timeout: 30,
            acquire_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        };
        let db_config = DbConfig {
            db_type: DbType::Postgres,
            db_url: url,
            db_id: TEST_DB_ID.to_string(),
            db_schema: Some("public".to_string()),
            pool_config,
            health_check_interval: 60,
            health_check_timeout: 5,
            domain_code: None,
            application_code: None,
            module_code: None,
            default: true,

            source_type: None,
        };
        let mm = std::sync::Arc::new(DatabaseManager::new(DatabaseManagerConfig::default()));
        mm.register_data_source(db_config)
            .await
            .expect("register test data source");

        // ensure schema（幂等，无参数 DDL）
        let ddl =
            include_str!("../../../../../../docs/sql/migrations/20260624_008_cmx_audit_log.up.sql");
        mm.execute_sql(TEST_DB_ID, None, ddl)
            .await
            .expect("ensure cmx_audit_log schema");
        // 清空历史数据
        mm.execute_sql(TEST_DB_ID, None, "TRUNCATE TABLE cmx_audit_log")
            .await
            .expect("truncate cmx_audit_log");

        Some(DatabaseAuditStore::new(mm, TEST_DB_ID, app_id))
    }

    /// 清理并关闭测试数据库。
    async fn teardown(store: &DatabaseAuditStore) {
        let _ = store
            .db_manager
            .execute_sql(TEST_DB_ID, None, "TRUNCATE TABLE cmx_audit_log")
            .await;
        let _ = store.db_manager.shutdown().await;
    }

    fn sample_record(domain: AuditDomain, op: &str, actor: &str) -> AuditRecord {
        AuditRecord::new(domain, op, OperationResult::Success)
            .with_actor(actor, actor)
            .with_details(json!({"key": op}))
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_save_and_query() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        let r1 = sample_record(AuditDomain::Auth, "login", "u1");
        let r2 = sample_record(AuditDomain::Iam, "role_assign", "u2");
        let r3 = sample_record(AuditDomain::Biz, "app_create", "u3");
        store.save(&r1).await?;
        store.save(&r2).await?;
        store.save(&r3).await?;

        let all = store.query(&AuditFilter::default(), 100, 0).await?;
        assert_eq!(all.len(), 3);

        // 按 domain 过滤
        let auth = store
            .query(
                &AuditFilter {
                    domain: Some(AuditDomain::Auth),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(auth.len(), 1);
        assert_eq!(auth[0].operation, "login");

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_query_pagination() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        // 写 10 条
        let recs: Vec<AuditRecord> = (0..10)
            .map(|i| {
                let mut r = sample_record(AuditDomain::Biz, "op", "u");
                r.id = format!("rec-{i}");
                r
            })
            .collect();
        store.save_batch(&recs).await?;

        let page = store.query(&AuditFilter::default(), 3, 2).await?;
        assert_eq!(page.len(), 3);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_query_time_range() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        let mut r = sample_record(AuditDomain::Auth, "login", "u1");
        let long_ago = Utc::now() - chrono::Duration::days(30);
        r.started_at = long_ago;
        store.save(&r).await?;

        let now = Utc::now();
        // 窗口只含现在 → 不应包含 30 天前的记录
        let recent = store
            .query(
                &AuditFilter {
                    from: Some(now - chrono::Duration::hours(1)),
                    to: Some(now),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(recent.len(), 0);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_delete_hard_with_ids() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        let r1 = sample_record(AuditDomain::Auth, "login", "u1");
        let r2 = sample_record(AuditDomain::Auth, "login", "u2");
        store.save(&r1).await?;
        store.save(&r2).await?;

        let affected = store
            .delete_hard(&AuditFilter {
                ids: Some(vec![r1.id.clone()]),
                ..Default::default()
            })
            .await?;
        assert_eq!(affected, 1);

        let remaining = store.query(&AuditFilter::default(), 100, 0).await?;
        assert_eq!(remaining.len(), 1);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    async fn database_delete_hard_rejects_empty_filter() -> Result<()> {
        // 此用例不依赖数据库（仅校验安全约束），用未连接真实库的 store 即可。
        let mm = std::sync::Arc::new(DatabaseManager::new(DatabaseManagerConfig::default()));
        let store = DatabaseAuditStore::new(mm, TEST_DB_ID, "default");

        let res = store.delete_hard(&AuditFilter::default()).await;
        assert!(res.is_err());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_delete_hard_app_id_isolation() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        // 写一条 app_id=default 的记录
        let r = sample_record(AuditDomain::Auth, "login", "u1");
        store.save(&r).await?;

        // 用 filter.app_id 指向不存在的 app，不应删除任何记录
        let affected = store
            .delete_hard(&AuditFilter {
                ids: Some(vec![r.id.clone()]),
                app_id: Some("other_app".to_string()),
                ..Default::default()
            })
            .await?;
        assert_eq!(affected, 0);

        let remaining = store.query(&AuditFilter::default(), 100, 0).await?;
        assert_eq!(remaining.len(), 1);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_query_details_roundtrip() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        let mut r = sample_record(AuditDomain::Biz, "app_create", "u1");
        r.details = Some(json!({"name": "测试应用", "count": 42, "nested": {"a": 1}}));
        store.save(&r).await?;

        let got = store.query(&AuditFilter::default(), 1, 0).await?;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].details, r.details);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_query_ids_filter() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        let r1 = sample_record(AuditDomain::Auth, "login", "u1");
        let r2 = sample_record(AuditDomain::Auth, "login", "u2");
        let r3 = sample_record(AuditDomain::Auth, "login", "u3");
        store.save(&r1).await?;
        store.save(&r2).await?;
        store.save(&r3).await?;

        let got = store
            .query(
                &AuditFilter {
                    ids: Some(vec![r1.id.clone(), r3.id.clone()]),
                    ..Default::default()
                },
                100,
                0,
            )
            .await?;
        assert_eq!(got.len(), 2);

        teardown(&store).await;
        Ok(())
    }

    #[tokio::test]
    async fn database_save_batch_empty() -> Result<()> {
        let mm = std::sync::Arc::new(DatabaseManager::new(DatabaseManagerConfig::default()));
        let store = DatabaseAuditStore::new(mm, TEST_DB_ID, "default");
        // 空数组不执行 SQL，直接返回 Ok（未连接库也不应报错）
        store.save_batch(&[]).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn database_save_batch_large() -> Result<()> {
        let store = match setup_store("default").await {
            Some(s) => s,
            None => return Ok(()),
        };

        // 1500 条 > BATCH_CHUNK(1000)，验证分批不报错且全部落库
        let recs: Vec<AuditRecord> = (0..1500)
            .map(|i| {
                let mut r = sample_record(AuditDomain::Biz, "bulk", "u");
                r.id = format!("bulk-{i}");
                r
            })
            .collect();
        store.save_batch(&recs).await?;

        let all = store.query(&AuditFilter::default(), 2000, 0).await?;
        assert_eq!(all.len(), 1500);

        teardown(&store).await;
        Ok(())
    }
}
