//! 台账系统表管理：DDL 常量、schema 检查、DB 读取辅助。
//!
//! - [`LEDGER_INIT_DDL`]：台账全量最新 DDL（目标库初始化时执行；幂等非破坏，不含 DROP）。
//!   与 `docs/sql/init/init_ddl.sql` 台账部分（37~41 节）保持同步；后者含 DROP，仅供手工重建。
//! - [`LEDGER_UPGRADE_DDL`]：已初始化库的升级补丁，当前为空；以后新增字段/索引时在此追加。
//! - 读取辅助：表存在性 / 台账元信息 / 模块当前态 / 主库模块名。

use cmx_core::model::cell::DataValue;
use cmx_database::get_default_db_manager;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use cmx_api_types::{Error, Result};

use crate::{db_err, data_value_string, LEDGER_TABLES, META_VERSION};

// ════════════════════════════════════════════════════════════════════════
//  DB 读取辅助（针对任意 db_id）
// ════════════════════════════════════════════════════════════════════════

/// 表是否存在（information_schema，内省）。
pub(crate) async fn table_exists(db_id: &str, table: &str) -> Result<bool> {
    let mm = get_default_db_manager();
    let sql = "SELECT 1 FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1 LIMIT 1";
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
    Ok(ds.iter().next().is_some())
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

// ════════════════════════════════════════════════════════════════════════
//  台账 DDL 常量
// ════════════════════════════════════════════════════════════════════════

/// 台账升级 DDL（针对**已初始化**的库）：`(表名, 幂等 ALTER 语句列表)`。
///
/// 当前基线（2026-08，META_VERSION=2）即全量最新结构，故为空。
/// 以后新增字段/索引时：① 在 [`LEDGER_INIT_DDL`] 的建表语句里加列（新库直接建出）；
/// ② 在此追加对应表的 `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`（已初始化库 reinit 时补齐）。
/// 语句必须幂等；`ensure_ledger_schema` 只给**已存在**的表打补丁，不创建缺失的主台账表，
/// 保持"未初始化库仍需完整初始化"的门闸语义。
const LEDGER_UPGRADE_DDL: &[(&str, &[&str])] = &[];

/// 对已存在的模型中心台账做幂等 schema 补齐（升级补丁见 [`LEDGER_UPGRADE_DDL`]）。
///
/// 模块主表只存模块身份与汇总，具体 DCT/DOC/RPT/SEED 状态按行写入
/// `cmx_model_module_kind`，以后增加新的模块类型只增加 kind 值，不再给主表加列。
pub(crate) async fn ensure_ledger_schema(db_id: &str) -> Result<()> {
    if LEDGER_UPGRADE_DDL.is_empty() {
        return Ok(());
    }
    let mm = get_default_db_manager();
    let tables: Vec<&str> = LEDGER_UPGRADE_DDL.iter().map(|(t, _)| *t).collect();
    let existing = tables_exist_batch(db_id, &tables).await?;
    for &(table, ddl) in LEDGER_UPGRADE_DDL {
        if !existing.contains(table) {
            continue;
        }
        for sql in ddl {
            mm.execute_sql(db_id, None, sql)
                .await
                .map_err(db_err(&format!("升级模型中心台账结构失败 {table}")))?;
        }
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════
//  schema 检查与门闸
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default)]
pub(crate) struct LedgerSchemaStatus {
    pub(crate) needs_upgrade: bool,
    pub(crate) module_table_exists: bool,
    pub(crate) module_kind_exists: bool,
    pub(crate) missing_tables: Vec<&'static str>,
    pub(crate) reasons: Vec<String>,
}

/// 检查目标库台账结构完整性（表存在性单次批量查询）。
pub(crate) async fn ledger_schema_status(db_id: &str) -> Result<LedgerSchemaStatus> {
    let mut st = LedgerSchemaStatus::default();
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

/// 模块操作门闸：目标库必须已初始化且台账为当前版本，否则拒绝后续操作。
///
/// # Errors
///
/// - 未初始化 → [`Error::BadRequest`]
/// - `meta_version` 低于当前或台账对象缺失 → [`Error::Conflict`]（HTTP 409，提示先走升级/重新初始化）
pub(crate) async fn ensure_current_ledger_schema(db_id: &str) -> Result<()> {
    let Some(meta) = read_meta(db_id).await? else {
        return Err(Error::BadRequest(
            "数据库尚未初始化，请先初始化模型中心".into(),
        ));
    };
    if meta.meta_version < META_VERSION {
        return Err(Error::Conflict(format!(
            "基础管理需要升级后才能执行模块操作：台账版本 v{} 低于当前 v{META_VERSION}",
            meta.meta_version
        )));
    }
    let schema = ledger_schema_status(db_id).await?;
    if schema.needs_upgrade {
        let reason = schema
            .reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "台账结构不完整".to_string());
        return Err(Error::Conflict(format!(
            "基础管理需要升级后才能执行模块操作：{reason}"
        )));
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════
//  台账读取
// ════════════════════════════════════════════════════════════════════════

/// 台账元信息（cmx_model_meta 单行）。
#[derive(Debug, Clone)]
pub(crate) struct LedgerMeta {
    /// 台账 schema 版本，用于判定是否需要升级系统表。
    pub(crate) meta_version: i32,
}

/// 读 cmx_model_meta 单行（未初始化返回 None）。
pub(crate) async fn read_meta(db_id: &str) -> Result<Option<LedgerMeta>> {
    if !table_exists(db_id, "cmx_model_meta").await? {
        return Ok(None);
    }
    let mm = get_default_db_manager();
    let sql = "SELECT meta_version FROM cmx_model_meta LIMIT 1";
    let ds = mm
        .query_sql(db_id, None, sql, "mc_meta")
        .await
        .map_err(db_err("读取 cmx_model_meta 失败"))?;
    let schema = ds.schema.clone();
    let Some(row) = ds.iter().next() else {
        return Ok(None);
    };
    let meta_version = row
        .get_by_name(schema.as_ref(), "meta_version")
        .and_then(|v| match v {
            DataValue::Int(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(1) as i32;
    Ok(Some(LedgerMeta { meta_version }))
}

/// 读 cmx_model_module 全部行 → map: "domain/app/module" -> 已部署模块台账详情。
///
/// kind 明细表存在时，把每 kind 的当前态合并进模块条目（见 [`merge_module_kinds`]）。
pub(crate) async fn read_modules(db_id: &str) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::new();
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
    merge_module_kinds(db_id, &mut map).await?;
    Ok(map)
}

/// 读 cmx_model_module_kind，把每 kind 的当前态合并进模块 map。
///
/// 模块主行字段优先，kind 行仅在主行对应字段缺失时补位；
/// `table_count` 取主行与各 kind 的最大值（主行为模块汇总口径）。
async fn merge_module_kinds(db_id: &str, map: &mut HashMap<String, Value>) -> Result<()> {
    if !table_exists(db_id, "cmx_model_module_kind").await? {
        return Ok(());
    }
    let mm = get_default_db_manager();
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
    Ok(())
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
pub(crate) async fn read_main_module_names() -> HashMap<String, String> {
    let mut map = HashMap::new();
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

// ════════════════════════════════════════════════════════════════════════
//  台账全量 DDL（初始化执行）
// ════════════════════════════════════════════════════════════════════════

/// 台账系统表全量 DDL（初始化时执行；幂等 IF NOT EXISTS + COMMENT，无 DROP）。
///
/// 与 `docs/sql/init/init_ddl.sql` 37~41 节同构同注释；
/// `cmx_model_registry` 为主控库专属，不在此列（见迁移文件 20260702_001）。
pub(crate) const LEDGER_INIT_DDL: &[&str] = &[
    // ── cmx_model_meta：台账自描述（每库单例） ──
    "CREATE TABLE IF NOT EXISTS cmx_model_meta (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), meta_version INT4 NOT NULL DEFAULT 1, app_id VARCHAR(64) NOT NULL, engine_version VARCHAR(50), portal_version VARCHAR(50), status VARCHAR(20) NOT NULL DEFAULT 'ready', initialized_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, initialized_by VARCHAR(100), initialized_name VARCHAR(100), last_upgraded_at TIMESTAMP, last_upgraded_by VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id)",
    "COMMENT ON TABLE cmx_model_meta IS '模型中心台账自描述（每库单例）'",
    "COMMENT ON COLUMN cmx_model_meta.id IS '主键ID'",
    "COMMENT ON COLUMN cmx_model_meta.db_id IS '数据库ID'",
    "COMMENT ON COLUMN cmx_model_meta.meta_version IS '台账 schema 版本，用于判定是否需要升级系统表'",
    "COMMENT ON COLUMN cmx_model_meta.app_id IS '应用ID'",
    "COMMENT ON COLUMN cmx_model_meta.engine_version IS '引擎版本'",
    "COMMENT ON COLUMN cmx_model_meta.portal_version IS '门户版本'",
    "COMMENT ON COLUMN cmx_model_meta.status IS '台账状态: ready / upgrading / failed'",
    "COMMENT ON COLUMN cmx_model_meta.initialized_at IS '初始化时间'",
    "COMMENT ON COLUMN cmx_model_meta.initialized_by IS '初始化人ID'",
    "COMMENT ON COLUMN cmx_model_meta.initialized_name IS '初始化人姓名'",
    "COMMENT ON COLUMN cmx_model_meta.last_upgraded_at IS '最近升级时间'",
    "COMMENT ON COLUMN cmx_model_meta.last_upgraded_by IS '最近升级人'",
    "COMMENT ON COLUMN cmx_model_meta.remark IS '备注'",
    "COMMENT ON COLUMN cmx_model_meta.create_time IS '创建时间'",
    "COMMENT ON COLUMN cmx_model_meta.update_time IS '更新时间'",
    // ── cmx_model_module：模块部署当前态主表 ──
    "CREATE TABLE IF NOT EXISTS cmx_model_module (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), overall_status VARCHAR(20) DEFAULT 'active', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), first_deployed_at TIMESTAMP, current_deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code)",
    "COMMENT ON TABLE cmx_model_module IS '模型中心-模块部署当前态主表（每模块一行；类型状态见 cmx_model_module_kind）'",
    "COMMENT ON COLUMN cmx_model_module.id IS '主键ID'",
    "COMMENT ON COLUMN cmx_model_module.db_id IS '数据库ID'",
    "COMMENT ON COLUMN cmx_model_module.app_id IS '应用ID'",
    "COMMENT ON COLUMN cmx_model_module.domain_code IS '域编码'",
    "COMMENT ON COLUMN cmx_model_module.application_code IS '应用编码'",
    "COMMENT ON COLUMN cmx_model_module.module_code IS '模块编码'",
    "COMMENT ON COLUMN cmx_model_module.module_name IS '模块名称'",
    "COMMENT ON COLUMN cmx_model_module.overall_status IS '整体状态: active/failed'",
    "COMMENT ON COLUMN cmx_model_module.table_count IS '表数量'",
    "COMMENT ON COLUMN cmx_model_module.def_source IS '定义来源文件'",
    "COMMENT ON COLUMN cmx_model_module.def_checksum IS '定义文件校验和'",
    "COMMENT ON COLUMN cmx_model_module.first_deployed_at IS '首次部署时间'",
    "COMMENT ON COLUMN cmx_model_module.current_deployed_at IS '当前部署时间'",
    "COMMENT ON COLUMN cmx_model_module.deployed_by IS '部署人ID'",
    "COMMENT ON COLUMN cmx_model_module.deployed_name IS '部署人姓名'",
    "COMMENT ON COLUMN cmx_model_module.archived IS '归档标志：0-未归档，1-已归档'",
    "COMMENT ON COLUMN cmx_model_module.create_time IS '创建时间'",
    "COMMENT ON COLUMN cmx_model_module.update_time IS '更新时间'",
    // ── cmx_model_module_kind：模块类型当前态（新增类型不改表结构） ──
    "CREATE TABLE IF NOT EXISTS cmx_model_module_kind (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), kind VARCHAR(20) NOT NULL, version VARCHAR(50), status VARCHAR(20) DEFAULT 'none', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), error_message TEXT, archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_kind_key ON cmx_model_module_kind (db_id, app_id, domain_code, application_code, module_code, kind)",
    "CREATE INDEX IF NOT EXISTS idx_model_module_kind_module ON cmx_model_module_kind (db_id, domain_code, application_code, module_code)",
    "COMMENT ON TABLE cmx_model_module_kind IS '模型中心-模块类型当前态（每模块每 kind 一行；新增类型不改表结构）'",
    "COMMENT ON COLUMN cmx_model_module_kind.id IS '主键ID'",
    "COMMENT ON COLUMN cmx_model_module_kind.db_id IS '数据库ID'",
    "COMMENT ON COLUMN cmx_model_module_kind.app_id IS '应用ID'",
    "COMMENT ON COLUMN cmx_model_module_kind.domain_code IS '域编码'",
    "COMMENT ON COLUMN cmx_model_module_kind.application_code IS '应用编码'",
    "COMMENT ON COLUMN cmx_model_module_kind.module_code IS '模块编码'",
    "COMMENT ON COLUMN cmx_model_module_kind.kind IS '模块类型: DCT/DOC/RPT/SEED/...'",
    "COMMENT ON COLUMN cmx_model_module_kind.version IS '当前版本'",
    "COMMENT ON COLUMN cmx_model_module_kind.status IS '类型状态: none/current/failed/upgrading'",
    "COMMENT ON COLUMN cmx_model_module_kind.table_count IS '表数量'",
    "COMMENT ON COLUMN cmx_model_module_kind.def_source IS '定义来源文件'",
    "COMMENT ON COLUMN cmx_model_module_kind.def_checksum IS '定义文件校验和'",
    "COMMENT ON COLUMN cmx_model_module_kind.deployed_at IS '部署时间'",
    "COMMENT ON COLUMN cmx_model_module_kind.deployed_by IS '部署人ID'",
    "COMMENT ON COLUMN cmx_model_module_kind.deployed_name IS '部署人姓名'",
    "COMMENT ON COLUMN cmx_model_module_kind.error_message IS '错误信息'",
    "COMMENT ON COLUMN cmx_model_module_kind.archived IS '归档标志：0-未归档，1-已归档'",
    "COMMENT ON COLUMN cmx_model_module_kind.create_time IS '创建时间'",
    "COMMENT ON COLUMN cmx_model_module_kind.update_time IS '更新时间'",
    // ── cmx_model_deploy_history：部署/升级历史（追加式，永不改写） ──
    "CREATE TABLE IF NOT EXISTS cmx_model_deploy_history (id VARCHAR(64) NOT NULL, batch_id VARCHAR(64), db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), action VARCHAR(20), from_version VARCHAR(50), to_version VARCHAR(50), status VARCHAR(20), ddl_summary JSONB, object_count INT4 DEFAULT 0, seed_rows INT4 DEFAULT 0, def_ref VARCHAR(300), def_version VARCHAR(50), engine_version VARCHAR(50), error_message TEXT, started_at TIMESTAMP, finished_at TIMESTAMP, duration_ms INT8, operator_id VARCHAR(100), operator_name VARCHAR(100), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code)",
    "CREATE INDEX IF NOT EXISTS idx_model_history_batch ON cmx_model_deploy_history (batch_id)",
    "CREATE INDEX IF NOT EXISTS idx_model_history_time ON cmx_model_deploy_history (create_time)",
    "COMMENT ON TABLE cmx_model_deploy_history IS '模型中心-部署/升级历史（追加式，永不改写）'",
    "COMMENT ON COLUMN cmx_model_deploy_history.id IS '主键ID'",
    "COMMENT ON COLUMN cmx_model_deploy_history.batch_id IS '批次ID'",
    "COMMENT ON COLUMN cmx_model_deploy_history.db_id IS '数据库ID'",
    "COMMENT ON COLUMN cmx_model_deploy_history.app_id IS '应用ID'",
    "COMMENT ON COLUMN cmx_model_deploy_history.domain_code IS '域编码'",
    "COMMENT ON COLUMN cmx_model_deploy_history.application_code IS '应用编码'",
    "COMMENT ON COLUMN cmx_model_deploy_history.module_code IS '模块编码'",
    "COMMENT ON COLUMN cmx_model_deploy_history.module_name IS '模块名称'",
    "COMMENT ON COLUMN cmx_model_deploy_history.kind IS '操作类别: INIT/META_UPGRADE/DCT/DOC/SEED'",
    "COMMENT ON COLUMN cmx_model_deploy_history.action IS '动作: deploy/upgrade/rollback'",
    "COMMENT ON COLUMN cmx_model_deploy_history.from_version IS '原版本'",
    "COMMENT ON COLUMN cmx_model_deploy_history.to_version IS '目标版本'",
    "COMMENT ON COLUMN cmx_model_deploy_history.status IS '状态机: pending→executing→success/failed/skipped'",
    "COMMENT ON COLUMN cmx_model_deploy_history.ddl_summary IS 'DDL 摘要 JSON'",
    "COMMENT ON COLUMN cmx_model_deploy_history.object_count IS '对象数量'",
    "COMMENT ON COLUMN cmx_model_deploy_history.seed_rows IS '初始数据行数'",
    "COMMENT ON COLUMN cmx_model_deploy_history.def_ref IS '定义引用'",
    "COMMENT ON COLUMN cmx_model_deploy_history.def_version IS '定义版本'",
    "COMMENT ON COLUMN cmx_model_deploy_history.engine_version IS '引擎版本'",
    "COMMENT ON COLUMN cmx_model_deploy_history.error_message IS '错误信息'",
    "COMMENT ON COLUMN cmx_model_deploy_history.started_at IS '开始时间'",
    "COMMENT ON COLUMN cmx_model_deploy_history.finished_at IS '完成时间'",
    "COMMENT ON COLUMN cmx_model_deploy_history.duration_ms IS '耗时(毫秒)'",
    "COMMENT ON COLUMN cmx_model_deploy_history.operator_id IS '操作人ID'",
    "COMMENT ON COLUMN cmx_model_deploy_history.operator_name IS '操作人姓名'",
    "COMMENT ON COLUMN cmx_model_deploy_history.create_time IS '创建时间'",
    // ── cmx_model_source：源定义/初始数据 JSON 完整留档 ──
    "CREATE TABLE IF NOT EXISTS cmx_model_source (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL, domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), version VARCHAR(50), source_file VARCHAR(300), source_json JSONB, compiled_json JSONB, checksum VARCHAR(64), table_count INT4 DEFAULT 0, seed_row_count INT4 DEFAULT 0, is_current INT4 DEFAULT 1, engine_version VARCHAR(50), imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, imported_by VARCHAR(100), imported_name VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version)",
    "CREATE INDEX IF NOT EXISTS idx_model_source_current ON cmx_model_source (db_id, domain_code, application_code, module_code, kind, is_current)",
    "COMMENT ON TABLE cmx_model_source IS '模型中心-源定义/初始数据 JSON 完整留档'",
    "COMMENT ON COLUMN cmx_model_source.id IS '主键ID'",
    "COMMENT ON COLUMN cmx_model_source.db_id IS '数据库ID'",
    "COMMENT ON COLUMN cmx_model_source.app_id IS '应用ID'",
    "COMMENT ON COLUMN cmx_model_source.domain_code IS '域编码'",
    "COMMENT ON COLUMN cmx_model_source.application_code IS '应用编码'",
    "COMMENT ON COLUMN cmx_model_source.module_code IS '模块编码'",
    "COMMENT ON COLUMN cmx_model_source.module_name IS '模块名称'",
    "COMMENT ON COLUMN cmx_model_source.kind IS '类别: DCT/DOC/SEED'",
    "COMMENT ON COLUMN cmx_model_source.version IS '版本'",
    "COMMENT ON COLUMN cmx_model_source.source_file IS '源文件路径'",
    "COMMENT ON COLUMN cmx_model_source.source_json IS '源定义或初始数据 JSON 原文（完整保存，可复现/审计）'",
    "COMMENT ON COLUMN cmx_model_source.compiled_json IS '编译后 JSON'",
    "COMMENT ON COLUMN cmx_model_source.checksum IS '校验和'",
    "COMMENT ON COLUMN cmx_model_source.table_count IS '表数量'",
    "COMMENT ON COLUMN cmx_model_source.seed_row_count IS '初始数据行数'",
    "COMMENT ON COLUMN cmx_model_source.is_current IS '是否当前版本：1-是，0-否'",
    "COMMENT ON COLUMN cmx_model_source.engine_version IS '引擎版本'",
    "COMMENT ON COLUMN cmx_model_source.imported_at IS '导入时间'",
    "COMMENT ON COLUMN cmx_model_source.imported_by IS '导入人ID'",
    "COMMENT ON COLUMN cmx_model_source.imported_name IS '导入人姓名'",
    "COMMENT ON COLUMN cmx_model_source.remark IS '备注'",
    "COMMENT ON COLUMN cmx_model_source.create_time IS '创建时间'",
];
