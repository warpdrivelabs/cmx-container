//! 台账系统表管理：DDL 常量、schema 检查、DB 读取辅助。
//!
//! 从 lib.rs 拆出：原"三、DB 读取辅助"段 + 台账 DDL 常量 + read_meta/read_modules 等。

use cmx_core::model::cell::DataValue;
use cmx_database::get_default_db_manager;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use cmx_api_types::{Error, Result};

use crate::{db_err, data_value_string, META_VERSION, LEDGER_TABLES};

// ════════════════════════════════════════════════════════════════════════
//  三、DB 读取辅助（针对任意 db_id）
// ════════════════════════════════════════════════════════════════════════

/// 表是否存在（information_schema，内省）。
pub(crate) async fn table_exists(db_id: &str, table: &str) -> Result<bool> {
    let mm = get_default_db_manager();
    let sql = "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1";
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            sql,
            vec![DataValue::String(table.to_string())],
            "mc_exists",
        )
        .await
        .map_err(db_err("查询表存在性失败"))?;
    let n = ds
        .iter()
        .next()
        .and_then(|r| r.get(0))
        .and_then(|v| match v {
            DataValue::Int(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(0);
    Ok(n > 0)
}

/// 批量查询多张表是否存在（单次 DB 往返，替代逐表 `table_exists`）。
///
/// 返回**存在的**表名集合。调用方用 `contains` 判定。
async fn tables_exist_batch(db_id: &str, tables: &[&str]) -> Result<HashSet<String>> {
    let mm = get_default_db_manager();
    let sql = "SELECT table_name FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = ANY($1)";
    let names: Vec<DataValue> = tables.iter().map(|t| DataValue::String(t.to_string())).collect();
    let ds = mm
        .query_sql_with_datavalues(db_id, None, sql, vec![DataValue::Array(names)], "mc_exists_batch")
        .await
        .map_err(db_err("批量查询表存在性失败"))?;
    let mut out = HashSet::new();
    for row in ds.iter() {
        if let Some(name) = row.get(0).and_then(data_value_string) {
            out.insert(name);
        }
    }
    Ok(out)
}

const LEDGER_META_UPGRADE_DDL: &[&str] = &[
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS db_id VARCHAR(100)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS meta_version INT4 NOT NULL DEFAULT 1",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS app_id VARCHAR(64) NOT NULL DEFAULT 'default'",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS engine_version VARCHAR(50)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS portal_version VARCHAR(50)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'ready'",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS initialized_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS initialized_by VARCHAR(100)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS initialized_name VARCHAR(100)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS last_upgraded_at TIMESTAMP",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS last_upgraded_by VARCHAR(100)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS remark VARCHAR(500)",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "ALTER TABLE cmx_model_meta ADD COLUMN IF NOT EXISTS update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id)",
];

const LEDGER_MODULE_UPGRADE_DDL: &[&str] = &[
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS db_id VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS app_id VARCHAR(64) NOT NULL DEFAULT 'default'",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS domain_code VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS application_code VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS module_code VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS module_name VARCHAR(200)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS overall_status VARCHAR(20) DEFAULT 'active'",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS table_count INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS def_source VARCHAR(300)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS def_checksum VARCHAR(64)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS first_deployed_at TIMESTAMP",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS current_deployed_at TIMESTAMP",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS deployed_by VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS deployed_name VARCHAR(100)",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS archived INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "ALTER TABLE cmx_model_module ADD COLUMN IF NOT EXISTS update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code)",
];

const LEDGER_MODULE_KIND_DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS cmx_model_module_kind (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), kind VARCHAR(20) NOT NULL, version VARCHAR(50), status VARCHAR(20) DEFAULT 'none', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), error_message TEXT, archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_kind_key ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind)",
    "CREATE INDEX IF NOT EXISTS idx_model_module_kind_module ON cmx_model_module_kind (db_id, domain_code, application_code, module_code)",
];

const LEDGER_HISTORY_UPGRADE_DDL: &[&str] = &[
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS batch_id VARCHAR(64)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS db_id VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS app_id VARCHAR(64) NOT NULL DEFAULT 'default'",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS domain_code VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS application_code VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS module_code VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS module_name VARCHAR(200)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS kind VARCHAR(20)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS action VARCHAR(20)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS from_version VARCHAR(50)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS to_version VARCHAR(50)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS status VARCHAR(20)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS ddl_summary JSONB",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS object_count INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS seed_rows INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS def_ref VARCHAR(300)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS def_version VARCHAR(50)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS engine_version VARCHAR(50)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS error_message TEXT",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS started_at TIMESTAMP",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS finished_at TIMESTAMP",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS duration_ms INT8",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS operator_id VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS operator_name VARCHAR(100)",
    "ALTER TABLE cmx_model_deploy_history ADD COLUMN IF NOT EXISTS create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code)",
    "CREATE INDEX IF NOT EXISTS idx_model_history_batch ON cmx_model_deploy_history (batch_id)",
    "CREATE INDEX IF NOT EXISTS idx_model_history_time ON cmx_model_deploy_history (create_time)",
];

const LEDGER_SOURCE_UPGRADE_DDL: &[&str] = &[
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS db_id VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS app_id VARCHAR(64) NOT NULL DEFAULT 'default'",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS domain_code VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS application_code VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS module_code VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS module_name VARCHAR(200)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS kind VARCHAR(20)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS version VARCHAR(50)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS source_file VARCHAR(300)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS source_json JSONB",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS compiled_json JSONB",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS checksum VARCHAR(64)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS table_count INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS seed_row_count INT4 DEFAULT 0",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS is_current INT4 DEFAULT 1",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS engine_version VARCHAR(50)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS imported_by VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS imported_name VARCHAR(100)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS remark VARCHAR(500)",
    "ALTER TABLE cmx_model_source ADD COLUMN IF NOT EXISTS create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version)",
    "CREATE INDEX IF NOT EXISTS idx_model_source_current ON cmx_model_source (db_id, domain_code, application_code, module_code, kind, is_current)",
];

/// 对已存在的模型中心台账做幂等 schema 补齐。
///
/// 模块主表只存模块身份与汇总，具体 DCT/DOC/RPT/SEED 状态按行写入
/// `cmx_model_module_kind`，以后增加新的模块类型只增加 kind 值，不再给主表加列。
/// 这里不创建缺失的主台账表，保持"未初始化库仍需初始化"的门闸语义；但已初始化
/// 的旧库会自动创建/补齐 kind 明细表，并从旧横向列迁移一次当前态。
pub(crate) async fn ensure_ledger_schema(db_id: &str) -> Result<()> {
    let mm = get_default_db_manager();
    let has_module = table_exists(db_id, "cmx_model_module").await?;
    for (table, ddl) in [
        ("cmx_model_meta", LEDGER_META_UPGRADE_DDL),
        ("cmx_model_module", LEDGER_MODULE_UPGRADE_DDL),
        ("cmx_model_deploy_history", LEDGER_HISTORY_UPGRADE_DDL),
        ("cmx_model_source", LEDGER_SOURCE_UPGRADE_DDL),
    ] {
        if !table_exists(db_id, table).await? {
            continue;
        }
        for sql in ddl {
            mm.execute_sql(db_id, None, sql)
                .await
                .map_err(db_err(&format!("升级模型中心台账结构失败 {table}")))?;
        }
    }
    if has_module {
        for sql in LEDGER_MODULE_KIND_DDL {
            mm.execute_sql(db_id, None, sql)
                .await
                .map_err(db_err("升级模型中心模块类型台账失败"))?;
        }
        migrate_legacy_module_kind_rows(db_id).await?;
    }
    Ok(())
}

async fn migrate_legacy_module_kind_rows(db_id: &str) -> Result<()> {
    let cols = table_columns(db_id, "cmx_model_module").await?;
    let mm = get_default_db_manager();
    for (kind, ver_col, st_col) in [
        ("DCT", "dct_version", "dct_status"),
        ("DOC", "doc_version", "doc_status"),
        ("RPT", "rpt_version", "rpt_status"),
        ("SEED", "seed_version", "seed_status"),
    ] {
        let has_ver = cols.contains_key(ver_col);
        let has_st = cols.contains_key(st_col);
        if !has_ver && !has_st {
            continue;
        }
        let version_expr = if has_ver { ver_col } else { "NULL" };
        let status_expr = if has_st { st_col } else { "'none'" };
        let sql = format!(
            "INSERT INTO cmx_model_module_kind \
             (id, db_id, app_id, domain_code, application_code, module_code, kind, version, status, table_count, def_source, def_checksum, deployed_at, deployed_by, deployed_name, archived, create_time, update_time) \
             SELECT md5(COALESCE(db_id,'') || ':' || COALESCE(app_id,'default') || ':' || COALESCE(domain_code,'') || ':' || COALESCE(application_code,'') || ':' || COALESCE(module_code,'') || ':{kind}'), \
                    db_id, COALESCE(app_id,'default'), domain_code, application_code, module_code, '{kind}', {version_expr}, COALESCE({status_expr}, 'none'), COALESCE(table_count,0), def_source, def_checksum, current_deployed_at, deployed_by, deployed_name, COALESCE(archived,0), COALESCE(create_time,CURRENT_TIMESTAMP), COALESCE(update_time,CURRENT_TIMESTAMP) \
               FROM cmx_model_module \
              WHERE COALESCE(archived,0) = 0 \
                AND ({version_expr} IS NOT NULL OR COALESCE({status_expr}, 'none') <> 'none') \
             ON CONFLICT (db_id, app_id, domain_code, application_code, module_code, kind) DO UPDATE SET \
                    version = COALESCE(EXCLUDED.version, cmx_model_module_kind.version), \
                    status = COALESCE(NULLIF(EXCLUDED.status, ''), cmx_model_module_kind.status), \
                    table_count = GREATEST(COALESCE(cmx_model_module_kind.table_count,0), COALESCE(EXCLUDED.table_count,0)), \
                    def_source = COALESCE(EXCLUDED.def_source, cmx_model_module_kind.def_source), \
                    def_checksum = COALESCE(EXCLUDED.def_checksum, cmx_model_module_kind.def_checksum), \
                    deployed_at = COALESCE(EXCLUDED.deployed_at, cmx_model_module_kind.deployed_at), \
                    deployed_by = COALESCE(EXCLUDED.deployed_by, cmx_model_module_kind.deployed_by), \
                    deployed_name = COALESCE(EXCLUDED.deployed_name, cmx_model_module_kind.deployed_name), \
                    update_time = CURRENT_TIMESTAMP"
        );
        mm.execute_sql(db_id, None, &sql)
            .await
            .map_err(db_err("迁移旧模块类型台账失败"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LedgerSchemaStatus {
    pub(crate) needs_upgrade: bool,
    pub(crate) module_table_exists: bool,
    pub(crate) module_kind_exists: bool,
    pub(crate) legacy_kind_columns: Vec<String>,
    pub(crate) missing_tables: Vec<&'static str>,
    pub(crate) reasons: Vec<String>,
}

pub(crate) async fn ledger_schema_status(db_id: &str) -> Result<LedgerSchemaStatus> {
    let mut st = LedgerSchemaStatus::default();
    // 单次批量查询替代 5 次逐表 table_exists（5 次 DB 往返 → 1 次）
    let existing = tables_exist_batch(db_id, LEDGER_TABLES).await?;
    for table in LEDGER_TABLES {
        if existing.contains(*table) {
            if *table == "cmx_model_module" {
                st.module_table_exists = true;
            } else if *table == "cmx_model_module_kind" {
                st.module_kind_exists = true;
            }
        } else {
            st.missing_tables.push(*table);
        }
    }
    if st.module_table_exists {
        let cols = table_columns(db_id, "cmx_model_module").await?;
        for col in [
            "dct_version",
            "dct_status",
            "doc_version",
            "doc_status",
            "rpt_version",
            "rpt_status",
            "seed_version",
            "seed_status",
        ] {
            if cols.contains_key(col) {
                st.legacy_kind_columns.push(col.to_string());
            }
        }
    }
    if st.module_table_exists && !st.module_kind_exists {
        st.needs_upgrade = true;
        st.reasons
            .push("缺少模块类型当前态表 cmx_model_module_kind".to_string());
    }
    if !st.missing_tables.is_empty() {
        st.needs_upgrade = true;
        st.reasons.push(format!(
            "缺少基础管理台账对象：{}",
            st.missing_tables.join(", ")
        ));
    }
    Ok(st)
}

pub(crate) async fn ensure_current_ledger_schema(db_id: &str) -> Result<()> {
    let meta = read_meta(db_id).await?;
    let meta_version = meta
        .as_ref()
        .and_then(|m| m.get("meta_version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let schema = ledger_schema_status(db_id).await?;
    if meta.is_none() {
        return Err(Error::BadRequest(
            "数据库尚未初始化，请先初始化模型中心".into(),
        ));
    }
    if meta_version < META_VERSION || schema.needs_upgrade {
        let reason = schema
            .reasons
            .first()
            .cloned()
            .unwrap_or_else(|| format!("台账版本 v{meta_version} 低于当前 v{META_VERSION}"));
        return Err(Error::BadRequest(format!(
            "基础管理需要升级后才能执行模块操作：{reason}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DbColumnSnapshot {
    pub(crate) data_type: String,
    pub(crate) length: Option<i64>,
    pub(crate) precision: Option<i64>,
    pub(crate) scale: Option<i64>,
    pub(crate) nullable: bool,
    pub(crate) default_value: Option<String>,
}

pub(crate) fn data_value_i64(v: &DataValue) -> Option<i64> {
    match v {
        DataValue::Int(i) => Some(*i),
        DataValue::Float(f) => Some(*f as i64),
        DataValue::Decimal(d) => d.to_string().parse::<i64>().ok(),
        DataValue::String(s) => s.trim().parse::<i64>().ok(),
        DataValue::ShortStr(s) => s.trim().parse::<i64>().ok(),
        DataValue::LongStr(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

pub(crate) async fn table_columns(db_id: &str, table: &str) -> Result<HashMap<String, DbColumnSnapshot>> {
    let mm = get_default_db_manager();
    let sql = "SELECT column_name, data_type, character_maximum_length, numeric_precision, numeric_scale, is_nullable, column_default \
               FROM information_schema.columns \
               WHERE table_schema = current_schema() AND table_name = $1 \
               ORDER BY ordinal_position";
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            sql,
            vec![DataValue::String(table.to_string())],
            "mc_columns",
        )
        .await
        .map_err(db_err("查询表列信息失败"))?;
    let mut out = HashMap::new();
    for row in ds.iter() {
        let name = row.get(0).and_then(data_value_string).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let data_type = row
            .get(1)
            .and_then(data_value_string)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let is_nullable = row
            .get(5)
            .and_then(data_value_string)
            .map(|s| s.eq_ignore_ascii_case("YES"))
            .unwrap_or(true);
        out.insert(
            name.clone(),
            DbColumnSnapshot {
                data_type,
                length: row.get(2).and_then(data_value_i64),
                precision: row.get(3).and_then(data_value_i64),
                scale: row.get(4).and_then(data_value_i64),
                nullable: is_nullable,
                default_value: row.get(6).and_then(data_value_string),
            },
        );
    }
    Ok(out)
}

/// 读 cmx_model_meta 单行（未初始化返回 None）。
pub(crate) async fn read_meta(db_id: &str) -> Result<Option<Value>> {
    if !table_exists(db_id, "cmx_model_meta").await? {
        return Ok(None);
    }
    let mm = get_default_db_manager();
    let cols = table_columns(db_id, "cmx_model_meta").await?;
    let meta_version_expr = if cols.contains_key("meta_version") {
        "meta_version"
    } else {
        "1 AS meta_version"
    };
    let status_expr = if cols.contains_key("status") {
        "status"
    } else {
        "'ready' AS status"
    };
    let initialized_at_expr = if cols.contains_key("initialized_at") {
        "initialized_at"
    } else {
        "NULL::timestamp AS initialized_at"
    };
    let sql = format!(
        "SELECT {meta_version_expr}, {status_expr}, {initialized_at_expr} FROM cmx_model_meta LIMIT 1"
    );
    let ds = mm
        .query_sql(db_id, None, &sql, "mc_meta")
        .await
        .map_err(db_err("读取 cmx_model_meta 失败"))?;
    let schema = ds.schema.clone();
    if let Some(row) = ds.iter().next() {
        let mv = row
            .get_by_name(schema.as_ref(), "meta_version")
            .and_then(|v| match v {
                DataValue::Int(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(1);
        let st = row
            .get_by_name(schema.as_ref(), "status")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                DataValue::ShortStr(s) => Some(s.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        Ok(Some(json!({ "meta_version": mv, "status": st })))
    } else {
        Ok(None)
    }
}

/// 读 cmx_model_module 全部行 → map: "domain/app/module" -> 已部署模块台账详情。
pub(crate) async fn read_modules(db_id: &str) -> Result<std::collections::HashMap<String, Value>> {
    let mut map = std::collections::HashMap::new();
    if !table_exists(db_id, "cmx_model_module").await? {
        return Ok(map);
    }
    let mm = get_default_db_manager();
    let sql = "SELECT domain_code, application_code, module_code, module_name, table_count, def_source, first_deployed_at, current_deployed_at, deployed_by, deployed_name, create_time, update_time FROM cmx_model_module WHERE archived = 0";
    let ds = mm
        .query_sql(db_id, None, sql, "mc_modules")
        .await
        .map_err(db_err("读取模块台账失败"))?;
    let schema = ds.schema.clone();
    let sv = |row: &cmx_core::model::data::dataset::Row, name: &str| -> Option<String> {
        row.get_by_name(schema.as_ref(), name)
            .and_then(data_value_string)
    };
    for row in ds.iter() {
        let d = sv(row, "domain_code").unwrap_or_default();
        let a = sv(row, "application_code").unwrap_or_default();
        let m = sv(row, "module_code").unwrap_or_default();
        let key = format!("{d}/{a}/{m}");
        map.insert(key.clone(), json!({
            "key": key,
            "domain": d,
            "application": a,
            "module": m,
            "module_name": sv(row, "module_name"),
            "table_count": sv(row, "table_count").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0),
            "def_source": sv(row, "def_source"),
            "first_deployed_at": sv(row, "first_deployed_at"),
            "current_deployed_at": sv(row, "current_deployed_at"),
            "deployed_by": sv(row, "deployed_by"),
            "deployed_name": sv(row, "deployed_name"),
            "create_time": sv(row, "create_time"),
            "update_time": sv(row, "update_time"),
        }));
    }
    if table_exists(db_id, "cmx_model_module_kind").await? {
        // 注意：def_checksum 仅 SEED/MENU 路径写入（DCT 路径传 None，COALESCE 保留旧值）。
        // 旧库可能存在 def_checksum=NULL 的行，读取时按 None 处理。
        let ksql = "SELECT domain_code, application_code, module_code, kind, version, status, table_count, def_source, def_checksum, deployed_at, deployed_by, deployed_name, create_time, update_time FROM cmx_model_module_kind WHERE archived = 0";
        let kds = mm
            .query_sql(db_id, None, ksql, "mc_module_kinds")
            .await
            .map_err(db_err("读取模块类型台账失败"))?;
        let kschema = kds.schema.clone();
        let kv = |row: &cmx_core::model::data::dataset::Row, name: &str| -> Option<String> {
            row.get_by_name(kschema.as_ref(), name)
                .and_then(data_value_string)
        };
        for row in kds.iter() {
            let d = kv(row, "domain_code").unwrap_or_default();
            let a = kv(row, "application_code").unwrap_or_default();
            let m = kv(row, "module_code").unwrap_or_default();
            let key = format!("{d}/{a}/{m}");
            let kind = kv(row, "kind").unwrap_or_default().to_ascii_lowercase();
            if kind.is_empty() {
                continue;
            }
            let entry = map.entry(key.clone()).or_insert_with(|| {
                json!({
                    "key": key,
                    "domain": d,
                    "application": a,
                    "module": m,
                    "module_name": m,
                    "table_count": 0,
                })
            });
            let kind_tables = kv(row, "table_count")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let total = entry
                .get("table_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            entry["table_count"] = json!(total.max(kind_tables));
            if entry.get("def_source").and_then(|v| v.as_str()).is_none() {
                entry["def_source"] = json!(kv(row, "def_source"));
            }
            if entry
                .get("current_deployed_at")
                .and_then(|v| v.as_str())
                .is_none()
            {
                entry["current_deployed_at"] = json!(kv(row, "deployed_at"));
            }
            if entry.get("deployed_by").and_then(|v| v.as_str()).is_none() {
                entry["deployed_by"] = json!(kv(row, "deployed_by"));
            }
            if entry
                .get("deployed_name")
                .and_then(|v| v.as_str())
                .is_none()
            {
                entry["deployed_name"] = json!(kv(row, "deployed_name"));
            }
            entry[&kind] = json!({
                "version": kv(row, "version"),
                "status": kv(row, "status").unwrap_or_else(|| "none".into()),
                "table_count": kind_tables,
                "def_source": kv(row, "def_source"),
                "def_checksum": kv(row, "def_checksum"),
                "deployed_at": kv(row, "deployed_at"),
                "deployed_by": kv(row, "deployed_by"),
                "deployed_name": kv(row, "deployed_name"),
                "create_time": kv(row, "create_time"),
                "update_time": kv(row, "update_time"),
            });
        }
    }
    Ok(map)
}

/// 从主库（defaultdb）的 `cmx_module` 表批量加载模块显示名。
///
/// module_name 的权威来源是主库 `cmx_module.name`，
/// **不能**取定义文件里的 `moduleName` / `title`（那是元数据标题，非模块名）。
///
/// 索引键用 `(domain_code, application_code, code)` 三段短 id 复合键，而**非** `resource_root`：
/// 实测主库 `resource_root`（如 SAP 为 `fi/sap/gl`）与定义文件目录（`fi/sap/sap_gl`）不一致，
/// 但 `code` 列与 db_state 的 module 段、定义文件 moduleCode 三者一致（均为 `sap_gl`）。
///
/// 返回 `"{domain}\x1f{app}\x1f{module}" → name` 的映射；主库无此表或查询失败时返回空 map。
pub(crate) async fn read_main_module_names() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mm = get_default_db_manager();
    let main_db = mm.get_default_db_id().await;
    // 主库未初始化时 cmx_module 可能不存在，table_exists 返回 false 即跳过
    if !table_exists(&main_db, "cmx_module").await.unwrap_or(false) {
        return map;
    }
    let sql = "SELECT domain_code, application_code, code, name FROM cmx_module WHERE archived = 0";
    let ds = match mm.query_sql(&main_db, None, sql, "mc_main_modules").await {
        Ok(ds) => ds,
        Err(e) => {
            tracing::warn!(error = %e, "主库 cmx_module 读取失败，module_name 将使用兜底值");
            return map;
        }
    };
    let schema = ds.schema.clone();
    for row in ds.iter() {
        let d = row
            .get_by_name(schema.as_ref(), "domain_code")
            .and_then(data_value_string)
            .unwrap_or_default();
        let a = row
            .get_by_name(schema.as_ref(), "application_code")
            .and_then(data_value_string)
            .unwrap_or_default();
        let c = row
            .get_by_name(schema.as_ref(), "code")
            .and_then(data_value_string)
            .unwrap_or_default();
        let name = row
            .get_by_name(schema.as_ref(), "name")
            .and_then(data_value_string);
        if let Some(n) = name {
            map.insert(format!("{d}\x1f{a}\x1f{c}"), n);
        }
    }
    map
}

/// 由 (domain, app, module) 三段短 id 拼复合查询键。
/// 与 `read_main_module_names` 的 map key 格式一致（`\x1f` 分隔，防歧义）。
pub(crate) fn main_module_key(domain: &str, app: &str, module: &str) -> String {
    format!("{domain}\x1f{app}\x1f{module}")
}

/// 台账系统表 DDL（初始化时执行；幂等 IF NOT EXISTS，与迁移文件同源）。
pub(crate) const INIT_DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS cmx_model_meta (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), meta_version INT4 NOT NULL DEFAULT 1, app_id VARCHAR(64) NOT NULL, engine_version VARCHAR(50), portal_version VARCHAR(50), status VARCHAR(20) NOT NULL DEFAULT 'ready', initialized_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, initialized_by VARCHAR(100), initialized_name VARCHAR(100), last_upgraded_at TIMESTAMP, last_upgraded_by VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id)",
    "CREATE TABLE IF NOT EXISTS cmx_model_module (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), overall_status VARCHAR(20) DEFAULT 'active', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), first_deployed_at TIMESTAMP, current_deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code)",
    "CREATE TABLE IF NOT EXISTS cmx_model_module_kind (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), kind VARCHAR(20) NOT NULL, version VARCHAR(50), status VARCHAR(20) DEFAULT 'none', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), error_message TEXT, archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_kind_key ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind)",
    "CREATE INDEX IF NOT EXISTS idx_model_module_kind_module ON cmx_model_module_kind (db_id, domain_code, application_code, module_code)",
    "CREATE TABLE IF NOT EXISTS cmx_model_deploy_history (id VARCHAR(64) NOT NULL, batch_id VARCHAR(64), db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), action VARCHAR(20), from_version VARCHAR(50), to_version VARCHAR(50), status VARCHAR(20), ddl_summary JSONB, object_count INT4 DEFAULT 0, seed_rows INT4 DEFAULT 0, def_ref VARCHAR(300), def_version VARCHAR(50), engine_version VARCHAR(50), error_message TEXT, started_at TIMESTAMP, finished_at TIMESTAMP, duration_ms INT8, operator_id VARCHAR(100), operator_name VARCHAR(100), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code)",
    "CREATE TABLE IF NOT EXISTS cmx_model_source (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), version VARCHAR(50), source_file VARCHAR(300), source_json JSONB, compiled_json JSONB, checksum VARCHAR(64), table_count INT4 DEFAULT 0, seed_row_count INT4 DEFAULT 0, is_current INT4 DEFAULT 1, engine_version VARCHAR(50), imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, imported_by VARCHAR(100), imported_name VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version)",
];
