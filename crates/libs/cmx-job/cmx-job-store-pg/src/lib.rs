//! cmx-job-store-pg —— 任务中心的 PostgreSQL 持久化实现（M2）。
//!
//! 实现 [`cmx_job_core::JobStore`]：作业主表 cmx_job / 日志 cmx_job_log / 断点 cmx_job_checkpoint
//! 的读写（主库 primary）。自 DDL（[`ddl`]，幂等）、崩溃恢复只读 [`PgJobStore::load_active`]、历史查询。
//!
//! DB 访问走 cmx-database-pg 全局 manager 的 DataValue 门面（`execute_sql_with_datavalues`/`query_sql_with_datavalues`），
//! 对齐 cmx-rpt-store-pg 的 query_rows/execute 惯例。所有写方法容错：失败只 warn 不 panic
//! （进度是内存权威，DB 是备份，方案 §14.1）。

pub mod ddl;

use async_trait::async_trait;
use serde_json::Value;
use tracing::warn;

use cmx_core::dv;
use cmx_core::model::cell::{DataValue, SqlTypeMarker};
use cmx_database_pg::get_default_pg_db_manager;
use cmx_job_core::{
    Job, JobError, JobOrigin, JobStatus, JobStore, ProgressSnapshot,
};

/// 任务中心主库 id（cmx_job_* 表所在库，对齐 dev-local.toml 的 primary）。
pub const JOB_DB_ID: &str = "primary";

/// PG 持久化实现。持有目标 db_id；无状态，克隆廉价。
pub struct PgJobStore {
    db_id: String,
}

impl PgJobStore {
    pub fn new(db_id: impl Into<String>) -> Self {
        Self {
            db_id: db_id.into(),
        }
    }

    /// 默认库（primary）。
    pub fn default_db() -> Self {
        Self::new(JOB_DB_ID)
    }

    /// DataValue 参数执行（非事务，单语句）。失败记 warn（容错）。
    async fn exec(&self, sql: &str, params: Vec<DataValue>, label: &str) {
        let mm = get_default_pg_db_manager();
        if let Err(e) = mm.execute_sql_with_datavalues(&self.db_id, None, sql, params).await {
            warn!(label, error = %e, "任务中心持久化写失败（已忽略，内存态为准）");
        }
    }

    /// DataValue 参数查询 → 行数组。
    async fn query(&self, sql: &str, params: Vec<DataValue>, label: &str) -> Vec<Value> {
        let mm = get_default_pg_db_manager();
        match mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, label)
            .await
        {
            Ok(ds) => serde_json::to_value(&ds)
                .ok()
                .and_then(|v| v.get("rows").and_then(|r| r.as_array()).cloned())
                .unwrap_or_default(),
            Err(e) => {
                warn!(label, error = %e, "任务中心持久化查询失败");
                Vec::new()
            }
        }
    }
}

// ───────────────────────── 行 ↔ Job 映射 ─────────────────────────

/// origin → (origin_tag, trigger)。
fn origin_cols(origin: &JobOrigin) -> (&'static str, Option<String>) {
    match origin {
        JobOrigin::Frontend { .. } => ("frontend", None),
        JobOrigin::Backend { trigger } => ("backend", Some(trigger.clone())),
    }
}

/// 状态字符串 → JobStatus。
fn parse_status(s: &str) -> JobStatus {
    match s {
        "pending" => JobStatus::Pending,
        "running" => JobStatus::Running,
        "paused" => JobStatus::Paused,
        "cancelling" => JobStatus::Cancelling,
        "cancelled" => JobStatus::Cancelled,
        "completed" => JobStatus::Completed,
        _ => JobStatus::Failed,
    }
}

/// 从查询行取值助手：字段可能是 JSON 数字或字符串（不同驱动序列化差异）。
fn row_i64(row: &Value, key: &str) -> Option<i64> {
    let v = row.get(key)?;
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// DB 行 → Job。progress/params/result/error 存 JSONB，读回反序列化。
fn row_to_job(row: &Value) -> Option<Job> {
    let id = row_i64(row, "id")?;
    let kind = row.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let status = parse_status(row.get("status").and_then(|v| v.as_str()).unwrap_or("failed"));
    // JSONB 列可能回传为 Value（对象）或字符串（text 形态），两种都兼容。
    let parse_json = |key: &str| -> Value {
        match row.get(key) {
            Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
            Some(v) => v.clone(),
            None => Value::Null,
        }
    };
    let progress: ProgressSnapshot =
        serde_json::from_value(parse_json("progress")).unwrap_or_default();
    let params = parse_json("params");
    let result = match parse_json("result") {
        Value::Null => None,
        v => Some(v),
    };
    let error: Option<JobError> = match parse_json("error") {
        Value::Null => None,
        v => serde_json::from_value(v).ok(),
    };
    let origin_tag = row.get("origin").and_then(|v| v.as_str()).unwrap_or("backend");
    let trigger = row.get("trigger").and_then(|v| v.as_str()).unwrap_or("recovered");
    let origin = if origin_tag == "frontend" {
        JobOrigin::Frontend { user: None }
    } else {
        JobOrigin::Backend {
            trigger: trigger.to_string(),
        }
    };
    Some(Job {
        id,
        kind,
        title: row.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        params,
        status,
        progress,
        result,
        error,
        priority: row_i64(row, "priority").unwrap_or(0) as i16,
        origin,
        org_id: row_i64(row, "org_id"),
        created_by: row_i64(row, "created_by"),
        created_at: row_i64(row, "created_at").unwrap_or(0),
        started_at: row_i64(row, "started_at"),
        finished_at: row_i64(row, "finished_at"),
    })
}

/// Job 的 JSONB 列（progress/params/result/error）→ DataValue 绑定值。
///
/// DataValue::Json 直接绑 `$N::jsonb`，NullTyped(Json) 绑类型化 NULL。
fn json_col(v: Option<&Value>) -> DataValue {
    match v {
        Some(x) => DataValue::Json(serde_json::to_string(x).unwrap_or_else(|_| "null".into())),
        None => DataValue::NullTyped(SqlTypeMarker::Json),
    }
}

/// 可空 bigint 列 → DataValue::Int 或 NullTyped(Int)。
fn nullable_int(v: Option<i64>) -> DataValue {
    match v {
        Some(n) => DataValue::Int(n),
        None => DataValue::NullTyped(SqlTypeMarker::Int),
    }
}

/// Job 的四个 JSONB 列绑定值（progress/params 必非空，result/error 可空）。
fn job_json_cols(job: &Job) -> (DataValue, DataValue, DataValue, DataValue) {
    let params = DataValue::Json(serde_json::to_string(&job.params).unwrap_or_else(|_| "{}".into()));
    let progress =
        DataValue::Json(serde_json::to_string(&job.progress).unwrap_or_else(|_| "{}".into()));
    let result = json_col(job.result.as_ref());
    let error_val = job.error.as_ref().and_then(|e| serde_json::to_value(e).ok());
    let error = json_col(error_val.as_ref());
    (params, progress, result, error)
}

#[async_trait]
impl JobStore for PgJobStore {
    async fn ensure_schema(&self) -> Result<(), String> {
        // 多实例并发启动时，两节点可能同时跑 CREATE TABLE/INDEX IF NOT EXISTS——Postgres 对
        // 并发 DDL（尤其 CREATE INDEX）可能报 duplicate/race 错。因每条都是幂等 IF NOT EXISTS，
        // 单条失败多为「对端已建」的良性竞争：逐条执行、失败仅 warn 不中断，最后校验主表存在即算成功。
        let mm = get_default_pg_db_manager();
        for stmt in ddl::DDL_STATEMENTS {
            if let Err(e) = mm.execute_sql_with_datavalues(&self.db_id, None, stmt, dv![]).await {
                tracing::warn!(error = %e, "任务中心 DDL 单句执行失败（多为并发建表良性竞争，忽略）");
            }
        }
        // 校验主表确已存在（无论本节点建的还是对端建的）。
        let check = mm
            .query_sql_with_datavalues(
                &self.db_id,
                None,
                "SELECT to_regclass('public.cmx_job') IS NOT NULL AS ok",
                dv![],
                "job_schema_check",
            )
            .await
            .map_err(|e| format!("schema 校验失败: {e}"))?;
        let ok = serde_json::to_value(&check)
            .ok()
            .and_then(|v| v.get("rows").and_then(|r| r.as_array()).cloned())
            .and_then(|rows| rows.first().cloned())
            .and_then(|r| r.get("ok").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err("cmx_job 表不存在（建表未成功）".into())
        }
    }

    async fn insert(&self, job: &Job) {
        let (params, progress, result, error) = job_json_cols(job);
        let (origin_tag, trigger) = origin_cols(&job.origin);
        // $5/$6/$7/$8 = params/progress/result/error，用 ::jsonb 把文本转 JSONB。
        // ON CONFLICT DO NOTHING：insert 仅负责建行一次；status/progress/result 由后续
        // update_* 各自 UPDATE 拥有。若用 DO UPDATE 回写 status，会与并发的 update_status
        // （fire-and-forget spawn）竞争——insert 后到会把 running 覆盖回 pending。
        let sql = r#"INSERT INTO cmx_job
            (id, kind, title, status, params, progress, result, error,
             priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at)
            VALUES ($1,$2,$3,$4,$5::jsonb,$6::jsonb,$7::jsonb,$8::jsonb,
                    $9,$10,$11,$12,$13,$14,$15,$16)
            ON CONFLICT (id) DO NOTHING"#;
        let params_arr = dv![
            job.id, job.kind.clone(), job.title.clone(), job.status.as_str(),
            params, progress, result, error,
            job.priority as i64, origin_tag, trigger,
            nullable_int(job.org_id), nullable_int(job.created_by),
            job.created_at, nullable_int(job.started_at), nullable_int(job.finished_at),
        ];
        self.exec(sql, params_arr, "job_insert").await;
    }

    async fn update_status(&self, job: &Job) {
        let (_p, progress, _r, _e) = job_json_cols(job);
        let sql = r#"UPDATE cmx_job SET status=$2, progress=$3::jsonb,
                     started_at=$4, finished_at=$5 WHERE id=$1"#;
        let params = dv![
            job.id, job.status.as_str(), progress,
            nullable_int(job.started_at), nullable_int(job.finished_at)
        ];
        self.exec(sql, params, "job_update_status").await;
    }

    async fn update_progress(&self, job: &Job) {
        let (_p, progress, _r, _e) = job_json_cols(job);
        let sql = "UPDATE cmx_job SET progress=$2::jsonb WHERE id=$1";
        self.exec(sql, dv![job.id, progress], "job_update_progress").await;
    }

    async fn finish(&self, job: &Job) {
        let (_p, progress, result, error) = job_json_cols(job);
        let sql = r#"UPDATE cmx_job SET status=$2, progress=$3::jsonb,
                     result=$4::jsonb, error=$5::jsonb, finished_at=$6 WHERE id=$1"#;
        let params = dv![
            job.id, job.status.as_str(), progress, result, error,
            nullable_int(job.finished_at)
        ];
        self.exec(sql, params, "job_finish").await;
    }

    async fn append_log(&self, job_id: i64, seq: i64, level: &str, event: &str, text: &str, at: i64) {
        let id = cmx_utils_next_id();
        let sql = r#"INSERT INTO cmx_job_log (id, job_id, seq, level, event, text, at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7)"#;
        self.exec(sql, dv![id, job_id, seq, level, event, text, at], "job_log").await;
    }

    async fn archive(&self, job_id: i64) {
        // RU/HI 归档：事务内 INSERT...SELECT 把作业行 + 日志转移到历史表，再从活跃表删除。
        // 用 INSERT...SELECT 全列复制（不重构行，抗 schema 漂移）；archived_at 用 now。
        // 断点(cmx_job_checkpoint)无历史价值，直接清。母版 cmx-flow RU/HI（终态归档）。
        let now = now_ms_pub();
        let mm = get_default_pg_db_manager();
        let tx = mm.get_transaction_context();
        let txn_id = match tx.begin(&self.db_id).await {
            Ok(t) => t,
            Err(e) => {
                warn!(job_id, error = %e, "归档事务开启失败（已跳过，活跃行保留）");
                return;
            }
        };
        let steps: [(&str, Vec<DataValue>); 5] = [
            (
                r#"INSERT INTO cmx_job_hi
                   (id, kind, title, status, params, progress, result, error, priority, origin,
                    trigger, org_id, created_by, created_at, started_at, finished_at, node_id,
                    heartbeat_at, control_intent, claimed_at, parent_job_id, archived_at)
                   SELECT id, kind, title, status, params, progress, result, error, priority, origin,
                    trigger, org_id, created_by, created_at, started_at, finished_at, node_id,
                    heartbeat_at, control_intent, claimed_at, parent_job_id, $2
                   FROM cmx_job WHERE id = $1
                   ON CONFLICT (id) DO NOTHING"#,
                dv![job_id, now],
            ),
            (
                r#"INSERT INTO cmx_job_hi_log (id, job_id, seq, level, event, text, data, at)
                   SELECT id, job_id, seq, level, event, text, data, at FROM cmx_job_log WHERE job_id = $1
                   ON CONFLICT (id) DO NOTHING"#,
                dv![job_id],
            ),
            ("DELETE FROM cmx_job_log WHERE job_id = $1", dv![job_id]),
            ("DELETE FROM cmx_job_checkpoint WHERE job_id = $1", dv![job_id]),
            ("DELETE FROM cmx_job WHERE id = $1", dv![job_id]),
        ];
        let mut ok = true;
        for (sql, params) in steps.iter() {
            if let Err(e) = mm
                .execute_sql_with_datavalues(&self.db_id, Some(&txn_id), sql, params.clone())
                .await
            {
                warn!(job_id, error = %e, "归档步骤失败，回滚");
                ok = false;
                break;
            }
        }
        if ok {
            if let Err(e) = tx.commit(&txn_id).await {
                warn!(job_id, error = %e, "归档提交失败");
            }
        } else {
            let _ = tx.rollback(&txn_id).await;
        }
    }

    async fn list_history(
        &self,
        kind: Option<&str>,
        status: Option<JobStatus>,
        offset: usize,
        limit: usize,
    ) -> Vec<Job> {
        let mut wheres = Vec::new();
        let mut params: Vec<DataValue> = Vec::new();
        if let Some(k) = kind {
            params.push(DataValue::String(k.to_string()));
            wheres.push(format!("kind = ${}", params.len()));
        }
        if let Some(s) = status {
            params.push(DataValue::String(s.as_str().to_string()));
            wheres.push(format!("status = ${}", params.len()));
        }
        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };
        // 分页：LIMIT n OFFSET m（archived_at DESC 稳定序，id DESC 破平）。
        let sql = format!(
            r#"SELECT id, kind, title, status, params, progress, result, error,
                      priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at,
                      node_id, archived_at
               FROM cmx_job_hi {where_sql}
               ORDER BY archived_at DESC, id DESC LIMIT {} OFFSET {}"#,
            limit.max(1),
            offset
        );
        self.query(&sql, params, "job_list_history")
            .await
            .iter()
            .filter_map(row_to_job)
            .collect()
    }

    async fn get_history(&self, job_id: i64) -> Option<Job> {
        let sql = r#"SELECT id, kind, title, status, params, progress, result, error,
                            priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at,
                            node_id, archived_at
                     FROM cmx_job_hi WHERE id = $1"#;
        self.query(sql, dv![job_id], "job_get_history")
            .await
            .first()
            .and_then(row_to_job)
    }

    async fn count_history(&self, kind: Option<&str>, status: Option<JobStatus>) -> u64 {
        // 与 list_history 同过滤，保证 total 与 items 一致（否则前端「N 条却列表空」）。
        let mut wheres = Vec::new();
        let mut params: Vec<DataValue> = Vec::new();
        if let Some(k) = kind {
            params.push(DataValue::String(k.to_string()));
            wheres.push(format!("kind = ${}", params.len()));
        }
        if let Some(s) = status {
            params.push(DataValue::String(s.as_str().to_string()));
            wheres.push(format!("status = ${}", params.len()));
        }
        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };
        let sql = format!("SELECT COUNT(*) AS n FROM cmx_job_hi {where_sql}");
        self.query(&sql, params, "job_count_history")
            .await
            .first()
            .and_then(|r| r.get("n"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0)
    }

    async fn list(&self, kind: Option<&str>, status: Option<JobStatus>, limit: usize) -> Vec<Job> {
        let mut wheres = Vec::new();
        let mut params: Vec<DataValue> = Vec::new();
        if let Some(k) = kind {
            params.push(DataValue::String(k.to_string()));
            wheres.push(format!("kind = ${}", params.len()));
        }
        if let Some(s) = status {
            params.push(DataValue::String(s.as_str().to_string()));
            wheres.push(format!("status = ${}", params.len()));
        }
        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };
        let sql = format!(
            r#"SELECT id, kind, title, status, params, progress, result, error,
                      priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at
               FROM cmx_job {where_sql}
               ORDER BY created_at DESC, id DESC LIMIT {}"#,
            limit.max(1)
        );
        self.query(&sql, params, "job_list")
            .await
            .iter()
            .filter_map(row_to_job)
            .collect()
    }

    async fn get(&self, job_id: i64) -> Option<Job> {
        let sql = r#"SELECT id, kind, title, status, params, progress, result, error,
                            priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at
                     FROM cmx_job WHERE id=$1"#;
        self.query(sql, dv![job_id], "job_get")
            .await
            .first()
            .and_then(row_to_job)
    }

    async fn load_active(&self) -> Vec<Job> {
        // 非终态：pending/running/paused/cancelling（终态不恢复）。
        let sql = r#"SELECT id, kind, title, status, params, progress, result, error,
                            priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at
                     FROM cmx_job
                     WHERE status IN ('pending','running','paused','cancelling')
                     ORDER BY created_at ASC"#;
        self.query(sql, dv![], "job_load_active")
            .await
            .iter()
            .filter_map(row_to_job)
            .collect()
    }

    // ───────────────────────── M3 分布式 ─────────────────────────

    async fn claim_pending(&self, node_id: &str, limit: usize, now: i64) -> Vec<Job> {
        // 原子抢占：子查询 FOR UPDATE SKIP LOCKED 锁住待领 pending 行（跳过被其它节点锁住的），
        // 外层 UPDATE 置 running+node_id+claimed_at+heartbeat_at，RETURNING 拿回本节点领到的作业。
        // 多实例并发调用各领不相交子集（SKIP LOCKED 保证不重领）——分布式不重跑的核心。
        let sql = format!(
            r#"UPDATE cmx_job SET
                 status='running', node_id=$1, claimed_at=$2, heartbeat_at=$2,
                 started_at=COALESCE(started_at,$2)
               WHERE id IN (
                 SELECT id FROM cmx_job
                 WHERE status='pending'
                 ORDER BY priority DESC, created_at ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT {}
               )
               RETURNING id, kind, title, status, params, progress, result, error,
                         priority, origin, trigger, org_id, created_by, created_at, started_at, finished_at"#,
            limit.max(1)
        );
        self.query(&sql, dv![node_id, now], "job_claim")
            .await
            .iter()
            .filter_map(row_to_job)
            .collect()
    }

    async fn heartbeat(&self, node_id: &str, job_ids: &[i64], now: i64) {
        if job_ids.is_empty() {
            return;
        }
        // id 列表拼进 IN（均为本进程产生的 i64，无注入风险）。
        let ids = job_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE cmx_job SET heartbeat_at=$2 WHERE node_id=$1 AND id IN ({ids}) AND status IN ('running','paused','cancelling')"
        );
        self.exec(&sql, dv![node_id, now], "job_heartbeat").await;
    }

    async fn reap_dead_owners(&self, timeout_ms: i64, now: i64) -> Vec<i64> {
        // 回收失联属主：heartbeat_at 早于 now-timeout 的活跃作业 → 重置 pending（清 node_id/心跳）。
        // 其它节点的 claim 循环随后重领。RETURNING id 供日志。
        let cutoff = now - timeout_ms;
        let sql = r#"UPDATE cmx_job SET
                       status='pending', node_id=NULL, heartbeat_at=NULL, claimed_at=NULL
                     WHERE status IN ('running','paused','cancelling')
                       AND heartbeat_at IS NOT NULL AND heartbeat_at < $1
                     RETURNING id"#;
        self.query(sql, dv![cutoff], "job_reap")
            .await
            .iter()
            .filter_map(|r| row_i64(r, "id"))
            .collect()
    }

    async fn set_control_intent(&self, job_id: i64, intent: &str) {
        // 排队中(pending)的 cancel：直接终态（无属主消费意图）；其余写 control_intent 列。
        if intent == "cancel" {
            let sql = r#"UPDATE cmx_job SET status='cancelled', finished_at=$2,
                         error='{"code":499,"message":"作业已被停止"}'::jsonb
                         WHERE id=$1 AND status='pending'"#;
            self.exec(sql, dv![job_id, now_ms_pub()], "job_cancel_pending").await;
        }
        let sql = "UPDATE cmx_job SET control_intent=$2 WHERE id=$1 AND status IN ('running','paused','cancelling')";
        self.exec(sql, dv![job_id, intent], "job_set_intent").await;
    }

    async fn take_control_intents(&self, node_id: &str) -> Vec<(i64, String)> {
        // 读本节点属主作业的待处理意图，读后清空。
        // 坑：`UPDATE...SET control_intent=NULL...RETURNING control_intent` 返回的是更新后的值（NULL）！
        // 故用 CTE 先 SELECT 快照旧值，再 UPDATE 清空，RETURNING 取 CTE 里的旧 intent。
        let sql = r#"
            WITH pend AS (
                SELECT id, control_intent FROM cmx_job
                WHERE node_id=$1 AND control_intent IS NOT NULL
            ),
            cleared AS (
                UPDATE cmx_job SET control_intent=NULL
                WHERE id IN (SELECT id FROM pend)
            )
            SELECT id, control_intent FROM pend"#;
        self.query(sql, dv![node_id], "job_take_intents")
            .await
            .iter()
            .filter_map(|r| {
                let id = row_i64(r, "id")?;
                let intent = r.get("control_intent").and_then(|v| v.as_str())?.to_string();
                Some((id, intent))
            })
            .collect()
    }
}

/// 当前 epoch 毫秒（供 store 内部用）。
fn now_ms_pub() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// bigint id 铸号（日志行主键；复用 cmx-utils 的 next_pk_id）。
fn cmx_utils_next_id() -> i64 {
    // 直接调 cmx-utils，避免额外依赖别名。
    cmx_utils::next_pk_id()
}
