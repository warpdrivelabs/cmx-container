//! 开发者工作区注册（W5·M0）—— 给孤儿端点 `POST /api/vscode/register` 一个后端归属。
//!
//! 扩展启动时上报 `{workspace_id, workspace_path, port}`；本 crate 落表 `cmx_dev_workspace`，为
//! 按开发者/租户隔离（M1 Nginx 路径路由 `/codeserver/{dev}/`、配额）打底。当前单 code-server 沿用，
//! 先补"谁的工作区在哪个端口/路径 + 认证归属 + 配额位"的登记。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_database::{execute_sql, execute_sql_with_params, query_sql_with_params, SqlParams};
use serde::{Deserialize, Serialize};

/// 工作区注册请求（扩展 `POST /api/vscode/register` 载荷）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RegisterRequest {
    pub workspace_id: String,
    pub workspace_path: String,
    pub port: i64,
    /// 开发者身份（认证注入；扩展可选带）。
    #[serde(default)]
    pub dev_id: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
}

/// 工作区记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevWorkspace {
    pub workspace_id: String,
    pub workspace_path: String,
    pub port: i64,
    pub dev_id: Option<String>,
    pub tenant_id: Option<String>,
    /// active | stale（心跳过期）。
    pub status: String,
    /// 磁盘配额（字节；0=不限）。M1 起用。
    pub disk_quota_bytes: i64,
    pub registered_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 工作区存储契约。
#[async_trait]
pub trait DevWorkspaceStore: Send + Sync {
    async fn register(&self, req: &RegisterRequest) -> Result<DevWorkspace, String>;
    async fn get(&self, workspace_id: &str) -> Result<Option<DevWorkspace>, String>;
    async fn list(&self) -> Result<Vec<DevWorkspace>, String>;
    async fn set_status(&self, workspace_id: &str, status: &str) -> Result<(), String>;
}

/// 建表 DDL（幂等）。
pub const DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS cmx_dev_workspace (
        workspace_id     VARCHAR(128) PRIMARY KEY,
        workspace_path   TEXT         NOT NULL,
        port             BIGINT       NOT NULL,
        dev_id           VARCHAR(128),
        tenant_id        VARCHAR(128),
        status           VARCHAR(16)  NOT NULL DEFAULT 'active',
        disk_quota_bytes BIGINT       NOT NULL DEFAULT 0,
        registered_at    TIMESTAMPTZ  NOT NULL,
        updated_at       TIMESTAMPTZ  NOT NULL
    )"#,
    "CREATE INDEX IF NOT EXISTS idx_cmx_dev_workspace_dev ON cmx_dev_workspace (dev_id)",
    "CREATE INDEX IF NOT EXISTS idx_cmx_dev_workspace_tenant ON cmx_dev_workspace (tenant_id)",
];

/// PG 工作区存储。
#[derive(Clone)]
pub struct PgDevWorkspaceStore {
    db_id: String,
}

impl PgDevWorkspaceStore {
    pub fn new(db_id: impl Into<String>) -> Self {
        Self { db_id: db_id.into() }
    }

    pub async fn ensure_schema(&self) -> Result<(), String> {
        for stmt in DDL {
            execute_sql(&self.db_id, None, stmt)
                .await
                .map_err(|e| format!("建工作区表失败: {e}"))?;
        }
        Ok(())
    }
}

#[async_trait]
impl DevWorkspaceStore for PgDevWorkspaceStore {
    async fn register(&self, req: &RegisterRequest) -> Result<DevWorkspace, String> {
        let now = Utc::now();
        // upsert：同 workspace_id 更新端口/路径/身份，registered_at 首登不变。
        execute_sql_with_params(
            &self.db_id,
            None,
            "INSERT INTO cmx_dev_workspace \
             (workspace_id, workspace_path, port, dev_id, tenant_id, status, registered_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,'active',$6,$6) \
             ON CONFLICT (workspace_id) DO UPDATE SET workspace_path=EXCLUDED.workspace_path, \
             port=EXCLUDED.port, dev_id=EXCLUDED.dev_id, tenant_id=EXCLUDED.tenant_id, \
             status='active', updated_at=EXCLUDED.updated_at",
            SqlParams::DataValues(vec![
                DataValue::String(req.workspace_id.clone()),
                DataValue::String(req.workspace_path.clone()),
                DataValue::Int(req.port),
                opt_str(&req.dev_id),
                opt_str(&req.tenant_id),
                DataValue::DateTime(now),
            ]),
        )
        .await
        .map_err(|e| format!("注册工作区失败: {e}"))?;
        self.get(&req.workspace_id)
            .await?
            .ok_or_else(|| "注册后回读失败".to_string())
    }

    async fn get(&self, workspace_id: &str) -> Result<Option<DevWorkspace>, String> {
        let ds = query_sql_with_params(
            &self.db_id,
            None,
            "SELECT workspace_id, workspace_path, port, dev_id, tenant_id, status, disk_quota_bytes, registered_at, updated_at \
             FROM cmx_dev_workspace WHERE workspace_id = $1",
            SqlParams::DataValues(vec![DataValue::String(workspace_id.into())]),
            "dev_workspace_one",
        )
        .await
        .map_err(|e| format!("查工作区失败: {e}"))?;
        Ok(rows(&ds).into_iter().next())
    }

    async fn list(&self) -> Result<Vec<DevWorkspace>, String> {
        let ds = query_sql_with_params(
            &self.db_id,
            None,
            "SELECT workspace_id, workspace_path, port, dev_id, tenant_id, status, disk_quota_bytes, registered_at, updated_at \
             FROM cmx_dev_workspace ORDER BY updated_at DESC",
            SqlParams::DataValues(vec![]),
            "dev_workspace_list",
        )
        .await
        .map_err(|e| format!("列工作区失败: {e}"))?;
        Ok(rows(&ds))
    }

    async fn set_status(&self, workspace_id: &str, status: &str) -> Result<(), String> {
        execute_sql_with_params(
            &self.db_id,
            None,
            "UPDATE cmx_dev_workspace SET status=$2, updated_at=$3 WHERE workspace_id=$1",
            SqlParams::DataValues(vec![
                DataValue::String(workspace_id.into()),
                DataValue::String(status.into()),
                DataValue::DateTime(Utc::now()),
            ]),
        )
        .await
        .map_err(|e| format!("更新工作区状态失败: {e}"))?;
        Ok(())
    }
}

fn opt_str(v: &Option<String>) -> DataValue {
    match v {
        Some(s) => DataValue::String(s.clone()),
        None => DataValue::Null,
    }
}

fn rows(ds: &DataSet) -> Vec<DevWorkspace> {
    let schema = ds.schema.as_ref();
    let mut out = Vec::new();
    for r in ds.iter() {
        out.push(DevWorkspace {
            workspace_id: gs(r, schema, "workspace_id"),
            workspace_path: gs(r, schema, "workspace_path"),
            port: gi(r, schema, "port"),
            dev_id: gos(r, schema, "dev_id"),
            tenant_id: gos(r, schema, "tenant_id"),
            status: gs(r, schema, "status"),
            disk_quota_bytes: gi(r, schema, "disk_quota_bytes"),
            registered_at: gt(r, schema, "registered_at"),
            updated_at: gt(r, schema, "updated_at"),
        });
    }
    out
}

fn gs(row: &Row, schema: &Schema, col: &str) -> String {
    match row.get_by_name(schema, col) {
        Some(DataValue::String(s)) => s.clone(),
        Some(DataValue::ShortStr(s)) | Some(DataValue::LongStr(s)) => s.to_string(),
        _ => String::new(),
    }
}
fn gos(row: &Row, schema: &Schema, col: &str) -> Option<String> {
    match row.get_by_name(schema, col) {
        Some(DataValue::String(s)) => Some(s.clone()),
        Some(DataValue::ShortStr(s)) | Some(DataValue::LongStr(s)) => Some(s.to_string()),
        _ => None,
    }
}
fn gi(row: &Row, schema: &Schema, col: &str) -> i64 {
    match row.get_by_name(schema, col) {
        Some(DataValue::Int(v)) => *v,
        _ => 0,
    }
}
fn gt(row: &Row, schema: &Schema, col: &str) -> DateTime<Utc> {
    match row.get_by_name(schema, col) {
        Some(DataValue::DateTime(dt)) => *dt,
        _ => Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_request_serde_snake_case() {
        let j = serde_json::json!({
            "workspace_id": "ws1", "workspace_path": "/data/workspace/ws1", "port": 18443
        });
        let r: RegisterRequest = serde_json::from_value(j).unwrap();
        assert_eq!(r.workspace_id, "ws1");
        assert_eq!(r.port, 18443);
        assert!(r.dev_id.is_none());
    }

    #[test]
    fn ddl_has_workspace_table() {
        assert!(DDL[0].contains("cmx_dev_workspace"));
        assert!(DDL[0].contains("disk_quota_bytes"));
    }
}

#[cfg(test)]
mod it {
    //! 真机集成（需本机 PG + fico）。`CMX_IT_PG=1 cargo test -p cmx-dev-workspace -- --ignored --nocapture`
    use super::*;
    use cmx_database::get_default_db_manager;
    use cmx_database::{DbConfig, DbType};

    async fn setup() -> String {
        let db_id = "cmx_it_ws".to_string();
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
    async fn register_upsert_roundtrip() {
        if std::env::var("CMX_IT_PG").is_err() {
            eprintln!("跳过：设 CMX_IT_PG=1 启用");
            return;
        }
        let store = PgDevWorkspaceStore::new(setup().await);
        store.ensure_schema().await.unwrap();

        let wsid = format!("ws-it-{}", Utc::now().timestamp_micros());
        let req = RegisterRequest {
            workspace_id: wsid.clone(),
            workspace_path: "/data/workspace/dev1".into(),
            port: 18443,
            dev_id: Some("dev1".into()),
            tenant_id: None,
        };
        let ws = store.register(&req).await.unwrap();
        assert_eq!(ws.status, "active");
        assert_eq!(ws.port, 18443);

        // upsert：同 id 改端口。
        let req2 = RegisterRequest { port: 18999, ..req.clone() };
        let ws2 = store.register(&req2).await.unwrap();
        assert_eq!(ws2.port, 18999, "upsert 应更新端口");

        // set_status。
        store.set_status(&wsid, "stale").await.unwrap();
        let got = store.get(&wsid).await.unwrap().unwrap();
        assert_eq!(got.status, "stale");
        assert!(store.list().await.unwrap().iter().any(|w| w.workspace_id == wsid));
        eprintln!("✅ dev_workspace register/upsert/status roundtrip 通过（真机 PG）");
    }
}
