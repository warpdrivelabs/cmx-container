//! `BuildJobStore` 的 PG 实现（W1）。落表 `cmx_plugin_build_job`。
//!
//! DB 纪律同 cmx-plugin 系：`execute_sql*` + `SqlParams::DataValues(Vec<DataValue>)`；
//! TIMESTAMPTZ 用 `DataValue::DateTime`，文本 `DataValue::String`，整数 `DataValue::Int`。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cmx_build::{BuildJob, BuildJobStore, BuildStatus};
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_database::{execute_sql, execute_sql_with_params, query_sql_with_params, SqlParams};

/// 建表 DDL（幂等）。
pub const DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS cmx_plugin_build_job (
        id              VARCHAR(64)  PRIMARY KEY,
        workspace_id    VARCHAR(128) NOT NULL,
        plugin_id       VARCHAR(128),
        tenant_id       VARCHAR(128),
        status          VARCHAR(16)  NOT NULL,
        target          VARCHAR(64)  NOT NULL,
        profile         VARCHAR(16)  NOT NULL,
        wasm_path       TEXT,
        artifact_zip_path TEXT,
        rev             VARCHAR(32),
        error_summary   TEXT,
        submitted_by    VARCHAR(128),
        submitted_at    TIMESTAMPTZ  NOT NULL,
        finished_at     TIMESTAMPTZ,
        duration_ms     BIGINT
    )"#,
    "CREATE INDEX IF NOT EXISTS idx_cmx_build_job_status ON cmx_plugin_build_job (status)",
    "CREATE INDEX IF NOT EXISTS idx_cmx_build_job_submitted ON cmx_plugin_build_job (submitted_at)",
];

/// PG 构建作业存储。
#[derive(Clone)]
pub struct PgBuildJobStore {
    db_id: String,
}

impl PgBuildJobStore {
    pub fn new(db_id: impl Into<String>) -> Self {
        Self { db_id: db_id.into() }
    }

    pub async fn ensure_schema(&self) -> Result<(), String> {
        for stmt in DDL {
            execute_sql(&self.db_id, None, stmt)
                .await
                .map_err(|e| format!("建构建作业表失败: {e}"))?;
        }
        Ok(())
    }

    async fn exec(&self, sql: &str, params: Vec<DataValue>) -> Result<u64, String> {
        execute_sql_with_params(&self.db_id, None, sql, SqlParams::DataValues(params))
            .await
            .map_err(|e| format!("执行失败: {e}"))
    }
}

#[async_trait]
impl BuildJobStore for PgBuildJobStore {
    async fn create(&self, job: &BuildJob) -> Result<(), String> {
        self.exec(
            "INSERT INTO cmx_plugin_build_job \
             (id, workspace_id, plugin_id, tenant_id, status, target, profile, submitted_by, submitted_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            vec![
                DataValue::String(job.id.clone()),
                DataValue::String(job.workspace_id.clone()),
                opt_str(&job.plugin_id),
                opt_str(&job.tenant_id),
                DataValue::String(job.status.as_str().into()),
                DataValue::String(job.target.clone()),
                DataValue::String(job.profile.clone()),
                opt_str(&job.submitted_by),
                DataValue::DateTime(job.submitted_at),
            ],
        )
        .await?;
        Ok(())
    }

    async fn update_status(
        &self,
        id: &str,
        status: BuildStatus,
        error_summary: Option<&str>,
    ) -> Result<(), String> {
        // 终态时补 finished_at + duration_ms（从 submitted_at 起算）。
        if status.is_terminal() {
            self.exec(
                "UPDATE cmx_plugin_build_job SET status=$2, error_summary=$3, finished_at=$4, \
                 duration_ms=EXTRACT(EPOCH FROM ($4 - submitted_at))*1000 WHERE id=$1",
                vec![
                    DataValue::String(id.into()),
                    DataValue::String(status.as_str().into()),
                    error_summary.map(|s| DataValue::String(s.into())).unwrap_or(DataValue::Null),
                    DataValue::DateTime(Utc::now()),
                ],
            )
            .await?;
        } else {
            self.exec(
                "UPDATE cmx_plugin_build_job SET status=$2, error_summary=$3 WHERE id=$1",
                vec![
                    DataValue::String(id.into()),
                    DataValue::String(status.as_str().into()),
                    error_summary.map(|s| DataValue::String(s.into())).unwrap_or(DataValue::Null),
                ],
            )
            .await?;
        }
        Ok(())
    }

    async fn set_artifact(&self, id: &str, wasm_path: &str, rev: &str) -> Result<(), String> {
        self.exec(
            "UPDATE cmx_plugin_build_job SET wasm_path=$2, rev=$3 WHERE id=$1",
            vec![
                DataValue::String(id.into()),
                DataValue::String(wasm_path.into()),
                DataValue::String(rev.into()),
            ],
        )
        .await?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<BuildJob>, String> {
        let ds = query_sql_with_params(
            &self.db_id,
            None,
            "SELECT id, workspace_id, plugin_id, tenant_id, status, target, profile, wasm_path, \
             artifact_zip_path, rev, error_summary, submitted_by, submitted_at, finished_at, duration_ms \
             FROM cmx_plugin_build_job WHERE id = $1",
            SqlParams::DataValues(vec![DataValue::String(id.into())]),
            "build_job_one",
        )
        .await
        .map_err(|e| format!("查询构建作业失败: {e}"))?;
        Ok(rows_to_jobs(&ds).into_iter().next())
    }

    async fn list_recent(&self, limit: i64) -> Result<Vec<BuildJob>, String> {
        let ds = query_sql_with_params(
            &self.db_id,
            None,
            "SELECT id, workspace_id, plugin_id, tenant_id, status, target, profile, wasm_path, \
             artifact_zip_path, rev, error_summary, submitted_by, submitted_at, finished_at, duration_ms \
             FROM cmx_plugin_build_job ORDER BY submitted_at DESC LIMIT $1",
            SqlParams::DataValues(vec![DataValue::Int(limit.clamp(1, 500))]),
            "build_job_list",
        )
        .await
        .map_err(|e| format!("列构建作业失败: {e}"))?;
        Ok(rows_to_jobs(&ds))
    }
}

// ————————————————————————— 助手 —————————————————————————

fn parse_status(s: &str) -> BuildStatus {
    match s {
        "building" => BuildStatus::Building,
        "scanning" => BuildStatus::Scanning,
        "signing" => BuildStatus::Signing,
        "deploying" => BuildStatus::Deploying,
        "success" => BuildStatus::Success,
        "failed" => BuildStatus::Failed,
        _ => BuildStatus::Queued,
    }
}

fn rows_to_jobs(ds: &DataSet) -> Vec<BuildJob> {
    let schema = ds.schema.as_ref();
    let mut out = Vec::new();
    for r in ds.iter() {
        out.push(BuildJob {
            id: get_string(r, schema, "id"),
            workspace_id: get_string(r, schema, "workspace_id"),
            plugin_id: get_opt_string(r, schema, "plugin_id"),
            tenant_id: get_opt_string(r, schema, "tenant_id"),
            status: parse_status(&get_string(r, schema, "status")),
            target: get_string(r, schema, "target"),
            profile: get_string(r, schema, "profile"),
            wasm_path: get_opt_string(r, schema, "wasm_path"),
            artifact_zip_path: get_opt_string(r, schema, "artifact_zip_path"),
            rev: get_opt_string(r, schema, "rev"),
            error_summary: get_opt_string(r, schema, "error_summary"),
            submitted_by: get_opt_string(r, schema, "submitted_by"),
            submitted_at: get_ts(r, schema, "submitted_at"),
            finished_at: get_opt_ts(r, schema, "finished_at"),
            duration_ms: get_opt_i64(r, schema, "duration_ms"),
        });
    }
    out
}

fn opt_str(v: &Option<String>) -> DataValue {
    match v {
        Some(s) => DataValue::String(s.clone()),
        None => DataValue::Null,
    }
}

fn get_string(row: &Row, schema: &Schema, col: &str) -> String {
    match row.get_by_name(schema, col) {
        Some(DataValue::String(s)) => s.clone(),
        Some(DataValue::ShortStr(s)) | Some(DataValue::LongStr(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn get_opt_string(row: &Row, schema: &Schema, col: &str) -> Option<String> {
    match row.get_by_name(schema, col) {
        Some(DataValue::String(s)) => Some(s.clone()),
        Some(DataValue::ShortStr(s)) | Some(DataValue::LongStr(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn get_ts(row: &Row, schema: &Schema, col: &str) -> DateTime<Utc> {
    match row.get_by_name(schema, col) {
        Some(DataValue::DateTime(dt)) => *dt,
        _ => Utc::now(),
    }
}

fn get_opt_ts(row: &Row, schema: &Schema, col: &str) -> Option<DateTime<Utc>> {
    match row.get_by_name(schema, col) {
        Some(DataValue::DateTime(dt)) => Some(*dt),
        _ => None,
    }
}

fn get_opt_i64(row: &Row, schema: &Schema, col: &str) -> Option<i64> {
    match row.get_by_name(schema, col) {
        Some(DataValue::Int(v)) => Some(*v),
        _ => None,
    }
}

#[cfg(test)]
mod it {
    //! 真机集成测试（需本机 PG + `fico` 库）。默认 `#[ignore]`，跑：
    //! `CMX_IT_PG=1 cargo test -p cmx-build-store-pg -- --ignored --nocapture`
    use super::*;
    use cmx_build::BuildStatus;
    use cmx_database::get_default_db_manager;
    use cmx_database::{DbConfig, DbType};

    async fn setup() -> String {
        let db_id = "cmx_it_build".to_string();
        let cfg = DbConfig {
            db_type: DbType::Postgres,
            db_url: std::env::var("CMX_IT_PG_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/fico".into()),
            db_id: db_id.clone(),
            db_name: Some("it".into()),
            db_schema: Some("public".into()),
            default: true,
            pool_config: Default::default(),
            health_check_interval: 60,
            health_check_timeout: 5,
            domain_code: None,
            application_code: None,
            module_code: None,
            source_type: Some("default".into()),
        };
        get_default_db_manager().register_data_source(cfg).await.unwrap();
        db_id
    }

    #[tokio::test]
    #[ignore]
    async fn build_job_crud_roundtrip() {
        if std::env::var("CMX_IT_PG").is_err() {
            eprintln!("跳过：设 CMX_IT_PG=1 启用真机集成");
            return;
        }
        let db_id = setup().await;
        let store = PgBuildJobStore::new(db_id);
        store.ensure_schema().await.unwrap();

        let job = BuildJob {
            id: format!("it-{}", Utc::now().timestamp_micros()),
            workspace_id: "ws-it".into(),
            plugin_id: None,
            tenant_id: None,
            status: BuildStatus::Queued,
            target: "wasm32-wasip1".into(),
            profile: "release".into(),
            wasm_path: None,
            artifact_zip_path: None,
            rev: None,
            error_summary: None,
            submitted_by: Some("tester".into()),
            submitted_at: Utc::now(),
            finished_at: None,
            duration_ms: None,
        };
        store.create(&job).await.unwrap();

        // 状态推进 + 产物。
        store.update_status(&job.id, BuildStatus::Building, None).await.unwrap();
        store.set_artifact(&job.id, "/tmp/x.wasm", "deadbeef").await.unwrap();
        store.update_status(&job.id, BuildStatus::Success, None).await.unwrap();

        let got = store.get(&job.id).await.unwrap().expect("应存在");
        assert_eq!(got.status, BuildStatus::Success);
        assert_eq!(got.wasm_path.as_deref(), Some("/tmp/x.wasm"));
        assert_eq!(got.rev.as_deref(), Some("deadbeef"));
        assert!(got.finished_at.is_some());
        assert!(got.duration_ms.is_some(), "终态应有 duration_ms");

        let recent = store.list_recent(10).await.unwrap();
        assert!(recent.iter().any(|j| j.id == job.id));
        eprintln!("✅ build_job CRUD roundtrip 通过（真机 PG）");
    }
}
