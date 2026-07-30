//! 数据库初始化流程：init_db / init_plan_stream / init_db_stream。
//!
//! 从 lib.rs 拆出：原 init 相关函数。

use cmx_core::model::cell::DataValue;
use cmx_database::get_default_db_manager;
use cmx_utils::snowflake_id_str;
use serde_json::{Value, json};

use cmx_api_types::Result;

use crate::db_err;
use crate::ledger::{
    ensure_ledger_schema, ledger_schema_status, read_meta, INIT_DDL,
};
use crate::{ENGINE_VERSION, LEDGER_TABLES, META_VERSION};

/// 初始化目标库：建 5 张台账系统表 + 写 cmx_model_meta + 记 INIT 历史。
///
/// # 流程
///
/// 1. 检测 reinit（已初始化时升级，否则新建）
/// 2. 跑 `INIT_DDL` 逐条建系统表（DDL 自动提交，txn_id=None）
/// 3. 调 `ensure_ledger_schema` 补齐 schema + 迁移旧横向列
/// 4. 开启事务 → `write_meta_and_history` 写 meta + 历史 → 提交
/// 5. 调 `db_state` 返回最新状态（前端可立即刷新工作台）
pub async fn init_db(db_id: &str, operator_id: &str, operator_name: &str) -> Result<Value> {
    let mm = get_default_db_manager();
    // reinit = 已存在 cmx_model_meta 行（首次初始化时为 false）
    let reinit = read_meta(db_id).await?.is_some();
    // 1) 建系统表（DDL 自动提交，txn_id=None）
    for ddl in INIT_DDL {
        mm.execute_sql(db_id, None, ddl)
            .await
            .map_err(db_err("建台账系统表失败"))?;
    }
    ensure_ledger_schema(db_id).await?;
    // 2) 写 meta + 历史（同一事务，保证原子性）
    let ctx = mm.get_transaction_context();
    let guard = ctx
        .begin_with_guard(db_id)
        .await
        .map_err(db_err("开启事务失败"))?;
    let txn = guard.txn_id().to_string();

    write_meta_and_history(db_id, &txn, reinit, operator_id, operator_name).await?;

    guard.commit().await.map_err(db_err("提交事务失败"))?;
    // 3) 返回最新 db_state（前端可立即拿到全量数据）
    crate::db_state::db_state(db_id).await
}

/// 写 cmx_model_meta（UPSERT）+ 记 INIT 历史。在事务内执行。
///
/// `reinit=true` 时 action="upgrade"，否则 action="create"。
async fn write_meta_and_history(
    db_id: &str,
    txn: &str,
    reinit: bool,
    operator_id: &str,
    operator_name: &str,
) -> Result<()> {
    let mm = get_default_db_manager();
    let current_app_id = cmx_utils::ConfigManager::global().get_app_id();

    let meta_sql = "INSERT INTO cmx_model_meta (id, db_id, app_id, meta_version, engine_version, status, initialized_by, initialized_name) VALUES ($1,$2,$3,$4,$5,'ready',$6,$7) \
        ON CONFLICT (db_id, app_id) DO UPDATE SET meta_version = EXCLUDED.meta_version, engine_version = EXCLUDED.engine_version, status = 'ready', last_upgraded_at = CURRENT_TIMESTAMP, last_upgraded_by = EXCLUDED.initialized_by, update_time = CURRENT_TIMESTAMP";
    mm.execute_sql_with_datavalues(
        db_id,
        Some(txn),
        meta_sql,
        vec![
            DataValue::String(snowflake_id_str()),
            DataValue::String(db_id.to_string()),
            DataValue::String(current_app_id.clone()),
            DataValue::Int(META_VERSION as i64),
            DataValue::String(ENGINE_VERSION.to_string()),
            DataValue::String(operator_id.to_string()),
            DataValue::String(operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写 cmx_model_meta 失败"))?;

    let action = if reinit { "upgrade" } else { "create" };
    let hist_sql = "INSERT INTO cmx_model_deploy_history (id, db_id, app_id, kind, action, to_version, status, object_count, engine_version, operator_id, operator_name, finished_at) VALUES ($1,$2,$3,'INIT',$4,$5,'success',$6,$7,$8,$9,CURRENT_TIMESTAMP)";
    mm.execute_sql_with_datavalues(
        db_id,
        Some(txn),
        hist_sql,
        vec![
            DataValue::String(snowflake_id_str()),
            DataValue::String(db_id.to_string()),
            DataValue::String(current_app_id.clone()),
            DataValue::String(action.to_string()),
            DataValue::String(META_VERSION.to_string()),
            DataValue::Int(LEDGER_TABLES.len() as i64),
            DataValue::String(ENGINE_VERSION.to_string()),
            DataValue::String(operator_id.to_string()),
            DataValue::String(operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写 INIT 历史失败"))?;

    Ok(())
}

/// 初始化进度事件（推给 SSE 通道）。kind: connect/step/progress/done/error。
#[derive(Clone)]
pub struct InitEvent {
    /// 事件类型：`"connect"` / `"step"` / `"progress"` / `"done"` / `"error"`
    /// 前端按 kind 决定展示位置（标题/步骤列表/进度条/完成态/错误态）
    pub kind: String,
    /// 事件数据：JSON 对象（具体字段随 kind 变化，参见各 send(ev(...)) 调用点）
    pub data: Value,
}
/// 简写构造器：内部用，比直接 `InitEvent { kind, data }` 更紧凑。
pub(crate) fn ev(kind: &str, data: Value) -> InitEvent {
    InitEvent {
        kind: kind.to_string(),
        data,
    }
}

/// 流式生成初始化/系统表升级计划：只读探测，不执行 DDL、不写台账。
pub async fn init_plan_stream(db_id: &str, tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>) {
    let send = |e: InitEvent| {
        let _ = tx.send(e);
    };
    send(ev(
        "connect",
        json!({ "message": format!("准备生成目标数据库 {db_id} 的系统表执行计划 …"), "db_id": db_id }),
    ));

    let mm = get_default_db_manager();
    match mm.query_sql(db_id, None, "SELECT 1", "mc_plan_ping").await {
        Ok(_) => send(ev(
            "connect",
            json!({ "ok": true, "message": "数据库连接成功，开始只读检查" }),
        )),
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "connect", "message": format!("数据库连接失败: {e}") }),
            ));
            return;
        }
    }

    let meta = match read_meta(db_id).await {
        Ok(v) => v,
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "state", "message": format!("读取模型中心状态失败: {e}") }),
            ));
            return;
        }
    };
    let reinit = meta.is_some();
    let meta_version = meta
        .as_ref()
        .and_then(|m| m.get("meta_version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let schema = match ledger_schema_status(db_id).await {
        Ok(v) => v,
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "state", "message": format!("检查基础管理结构失败: {e}") }),
            ));
            return;
        }
    };
    send(ev(
        "step",
        json!({
            "message": if reinit {
                if meta_version < META_VERSION as i64 || schema.needs_upgrade {
                    "检测到旧版基础管理结构：将生成可审核升级计划"
                } else {
                    "该库已初始化：计划执行基础管理加性校验，不删除任何数据"
                }
            } else {
                "该库尚未初始化：计划创建模型中心基础管理台账"
            },
            "reinit": reinit,
            "meta_version": meta_version,
            "expected_meta_version": META_VERSION,
            "upgrade_reasons": schema.reasons,
        }),
    ));

    let total = INIT_DDL.len();
    send(ev(
        "step",
        json!({ "message": format!("计划检查/执行 {total} 条幂等 DDL（CREATE IF NOT EXISTS / INDEX IF NOT EXISTS）"), "total": total }),
    ));
    for (i, ddl) in INIT_DDL.iter().enumerate() {
        let obj = ddl
            .split_whitespace()
            .skip_while(|w| !w.eq_ignore_ascii_case("EXISTS"))
            .nth(1)
            .unwrap_or("")
            .to_string();
        send(ev(
            "progress",
            json!({
                "index": i + 1,
                "total": total,
                "object": obj,
                "message": format!("[{}/{}] {}", i + 1, total, if obj.is_empty() { "执行幂等 DDL" } else { obj.as_str() })
            }),
        ));
    }
    if reinit {
        send(ev(
            "step",
            json!({ "message": "升级影响说明：本次为加性升级，只创建缺失台账/索引、补齐缺失列并迁移当前态；不会删除业务表、不会删除台账数据、不会删除旧列。" }),
        ));
        send(ev(
            "progress",
            json!({
                "message": "将把 DCT/DOC/RPT/SEED 等类型版本状态迁移到 cmx_model_module_kind；以后新增类型只新增 kind 行，不再修改 cmx_model_module 结构。",
                "legacy_columns": schema.legacy_kind_columns,
            }),
        ));
        send(ev(
            "progress",
            json!({ "message": "升级完成前将锁定模块创建/安装/升级，避免用旧台账结构写入新类型状态。" }),
        ));
    }
    send(ev(
        "step",
        json!({ "message": "计划写入/更新 cmx_model_meta，并记录 INIT 执行历史" }),
    ));
    let mut results = vec![
        json!({
            "module": "基础管理台账",
            "kind": "SYS",
            "status": "planned",
            "version": META_VERSION,
            "tables": LEDGER_TABLES.len(),
            "table_names": LEDGER_TABLES,
            "note": if reinit { "校验/补齐基础管理台账表与索引" } else { "创建基础管理台账表与索引" },
        }),
        json!({
            "module": "模块类型当前态",
            "kind": "SYS",
            "status": "planned",
            "version": META_VERSION,
            "tables": 1,
            "table_names": ["cmx_model_module_kind"],
            "note": "按行保存 DCT/DOC/RPT/SEED/... 的版本与状态，新增类型不再改主表",
        }),
    ];
    if reinit && !schema.legacy_kind_columns.is_empty() {
        results.push(json!({
            "module": "旧台账数据迁移",
            "kind": "SYS",
            "status": "planned",
            "version": META_VERSION,
            "tables": 1,
            "table_names": ["cmx_model_module", "cmx_model_module_kind"],
            "note": format!("从旧列迁移当前态：{}", schema.legacy_kind_columns.join(", ")),
        }));
    }
    results.push(json!({
        "module": "版本标记",
        "kind": "SYS",
        "status": "planned",
        "version": META_VERSION,
        "tables": 1,
        "table_names": ["cmx_model_meta"],
        "note": format!("升级 meta_version：v{meta_version} -> v{META_VERSION}"),
    }));
    send(ev(
        "done",
        json!({
            "message": "执行计划已生成，请审核后确认是否执行",
            "action": if reinit { "upgrade" } else { "create" },
            "tables": LEDGER_TABLES,
            "ddl_count": total,
            "results": results,
            "impacts": [
                "仅执行加性 DDL：CREATE TABLE IF NOT EXISTS、CREATE INDEX IF NOT EXISTS、ADD COLUMN IF NOT EXISTS。",
                "不删除业务表、不删除台账数据、不删除旧横向列；旧列只作为迁移来源保留。",
                "升级完成后模块类型状态写入 cmx_model_module_kind，新增模块类型不再需要改 cmx_model_module 表结构。",
                "升级未完成前，模块创建/安装/升级会被阻止，以避免新旧台账混写。",
            ],
            "db_id": db_id,
        }),
    ));
}

/// 流式初始化：逐步把「连接建立 / 建表进度 / 写台账 / 完成 / 错误」推给 tx。
/// 每一步都发事件，前端实时展示；失败发 error 事件后中止（不 panic）。
pub async fn init_db_stream(
    db_id: &str,
    operator_id: &str,
    operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    let started = std::time::Instant::now();
    let send = |e: InitEvent| {
        let _ = tx.send(e);
    };

    send(ev(
        "connect",
        json!({ "message": format!("连接目标数据库 {db_id} …"), "db_id": db_id }),
    ));

    let mm = get_default_db_manager();

    // 0) 探测连接（一次轻量查询即建立/复用连接池）——把连接失败与建表失败区分开。
    match mm.query_sql(db_id, None, "SELECT 1", "mc_ping").await {
        Ok(_) => send(ev(
            "connect",
            json!({ "ok": true, "message": "数据库连接成功" }),
        )),
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "connect", "message": format!("数据库连接失败: {e}") }),
            ));
            return;
        }
    }

    // 幂等 + 可重复：已初始化不再跳过，而是重跑「加性 DDL」(CREATE TABLE IF NOT EXISTS，绝不删表/删列)，
    // 并把 meta 标记为「已校验/升级」。满足"允许多次初始化，但不删数据，只做变化性升级"。
    let reinit = matches!(read_meta(db_id).await, Ok(Some(_)));
    if reinit {
        send(ev(
            "step",
            json!({ "message": "检测到该库已初始化 → 执行加性校验/升级（不删除任何数据）" }),
        ));
    }

    // 1) 逐条建系统表（DDL 自动提交，每条发进度）。CREATE TABLE IF NOT EXISTS 天然加性、可重复。
    let total = INIT_DDL.len();
    send(ev(
        "step",
        json!({ "message": format!("{}台账系统表（{total} 条 DDL）", if reinit { "校验" } else { "创建" }), "total": total }),
    ));
    for (i, ddl) in INIT_DDL.iter().enumerate() {
        // 从 DDL 里粗提对象名用于展示
        let obj = ddl
            .split_whitespace()
            .skip_while(|w| !w.eq_ignore_ascii_case("EXISTS"))
            .nth(1)
            .unwrap_or("")
            .to_string();
        match mm.execute_sql(db_id, None, ddl).await {
            Ok(_) => send(ev(
                "progress",
                json!({
                    "index": i + 1, "total": total, "object": obj,
                    "message": format!("[{}/{}] {}", i + 1, total, if obj.is_empty() { "执行 DDL" } else { obj.as_str() })
                }),
            )),
            Err(e) => {
                send(ev(
                    "error",
                    json!({ "stage": "ddl", "index": i + 1, "object": obj, "message": format!("建表失败: {e}") }),
                ));
                return;
            }
        }
    }
    send(ev(
        "step",
        json!({ "message": "补齐/升级基础管理结构，并迁移旧版模块类型当前态 …" }),
    ));
    if let Err(e) = ensure_ledger_schema(db_id).await {
        send(ev(
            "error",
            json!({ "stage": "ledger_upgrade", "message": format!("升级基础管理结构失败: {e}") }),
        ));
        return;
    }
    send(ev(
        "progress",
        json!({ "message": "基础管理结构已补齐；旧横向类型列如存在，已迁移到 cmx_model_module_kind" }),
    ));

    // 2) 写/更新 meta + 记历史（事务）。UPSERT：首次插入，重复初始化则更新 last_upgraded_at（不动 initialized_*）。
    send(ev("step", json!({ "message": "写入台账元信息与历史 …" })));
    let ctx = mm.get_transaction_context();
    let guard = match ctx.begin_with_guard(db_id).await {
        Ok(g) => g,
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "txn", "message": format!("开启事务失败: {e}") }),
            ));
            return;
        }
    };
    let txn = guard.txn_id().to_string();

    if let Err(e) = write_meta_and_history(db_id, &txn, reinit, operator_id, operator_name).await {
        send(ev(
            "error",
            json!({ "stage": "meta", "message": format!("写元信息/历史失败: {e}") }),
        ));
        return;
    }
    if let Err(e) = guard.commit().await {
        send(ev(
            "error",
            json!({ "stage": "commit", "message": format!("提交事务失败: {e}") }),
        ));
        return;
    }

    // 3) 完成：附最新 db-state，前端直接刷新工作台。
    match crate::db_state::db_state(db_id).await {
        Ok(st) => send(ev(
            "done",
            json!({
                "message": if reinit { "校验/升级完成（未删除任何数据）" } else { "初始化完成" },
                "reinit": reinit,
                "tables": total,
                "db_state": st,
                "duration_ms": started.elapsed().as_millis() as i64,
            }),
        )),
        Err(e) => send(ev(
            "error",
            json!({ "stage": "state", "message": e.to_string() }),
        )),
    }
}
