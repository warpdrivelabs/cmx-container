//! 部署流程：deploy / deploy_stream / deploy_plan_stream / deploy_with_events。
//!
//! 从 lib.rs 拆出：原 deploy 相关函数 + SEED/MENU 部署辅助函数。

use cmx_core::model::cell::{DataValue, TableDefine};
use cmx_database::get_default_db_manager;
use cmx_metadata::{PgTableDefineExecutor, TableDefineDbExecutor};
use cmx_utils::snowflake_id_str;
use serde_json::{Value, json};

use cmx_api_types::{Error, Result};

use crate::compile::compile_definition;
use crate::db_state::db_state;
use crate::diff_report::{table_action_label, table_change_plan};
use crate::init::{ev, InitEvent};
use crate::ledger::{
    ensure_current_ledger_schema, main_module_key, read_main_module_names, read_meta, table_exists,
};
use crate::seed_scanner;
use crate::{db_err, deploy_seed_menu, ENGINE_VERSION};

/// 部署 kind 优先级：DCT(0) → DOC(1) → RPT(2) → SEED(3) → MENU(4)。
///
/// 保证 SEED 在 DCT 建表之后执行（SEED 依赖目标表已建），MENU 最后同步。
/// `deploy_with_events` 与 `deploy_plan_stream` 共用此函数，确保「预览顺序 == 执行顺序」。
///
/// 未知 kind 返回 99（兜底：排到最后，避免误插队）。
fn kind_order(k: &str) -> u8 {
    match k {
        "DCT" => 0,  // 先建业务表
        "DOC" => 1,  // 再建单据表
        "RPT" => 2,  // 再建报表落地表
        "SEED" => 3, // 写入种子（DCT/DOC 须先建好）
        "MENU" => 4, // 最后同步菜单
        _ => 99,     // 未知 kind 排到最后（防御性兜底）
    }
}

/// 部署项：从前端 JSON item 解析出的结构化字段。
#[derive(Debug, Clone)]
struct DeployItem {
    /// 资源类型：`DCT` / `DOC` / `RPT` / `SEED` / `MENU`
    kind: String,
    /// 领域编码（如 "fi"）
    domain: String,
    /// 应用编码（如 "cmxfico"）
    app: String,
    /// 模块编码（如 "report"）
    module: String,
    /// 定义文件名（DCT/DOC/RPT 必填；SEED/MENU 路径由扫描器决定）
    file: String,
}

/// 解析前端传入的 items JSON 数组 → 按 kind 优先级稳定排序的 DeployItem 列表。
///
/// 排序规则：DCT(0) → DOC(1) → RPT(2) → SEED(3) → MENU(4)，同 kind 内保持原始顺序。
/// `deploy_plan_stream` 与 `deploy_with_events` 共用此函数，保证「预览顺序 == 执行顺序」。
///
/// 容错：
/// - `kind` 缺失 / 非字符串 → 默认 "DCT"
/// - `application` / `app` 缺失或非字符串 → 空串（`domain` 缺失同理）
/// - `module` / `file` 缺失或非字符串 → 空串
fn parse_deploy_items(items: &[Value]) -> Vec<(usize, DeployItem)> {
    let mut parsed: Vec<(usize, DeployItem)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            // kind 默认 DCT（兼容老前端）
            let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("DCT").to_uppercase();
            let domain = it.get("domain").and_then(|v| v.as_str()).unwrap_or("").to_string();
            // app 与 application 兼容（老前端可能用 app）
            let app = it
                .get("application")
                .or_else(|| it.get("app"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let module = it.get("module").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
            (i, DeployItem { kind, domain, app, module, file })
        })
        .collect();
    // 排序：先按 kind 优先级，再按原下标（同 kind 内稳定）
    parsed.sort_by_key(|(orig_idx, item)| (kind_order(&item.kind), *orig_idx));
    parsed
}

/// 部署一批定义（create/upgrade）到目标库：编译→建表→写台账+源JSON+历史。
///
/// items: [{ kind, domain, application, module, file }]
/// operator_id / operator_name 写入台账（cmx_model_deploy_history / cmx_model_module 等）。
pub async fn deploy(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
) -> Result<Value> {
    // 转发到 deploy_with_events（tx=None → 走非流式，无 SSE 推送）
    deploy_with_events(db_id, items, operator_id, operator_name, None).await
}

/// 流式部署：把后端每个阶段的提示、进度、错误、最终结果推给 SSE。
///
/// 与 `deploy` 同源（都走 `deploy_with_events`），仅多一个 SSE 通道参数；
/// 调用方（如 cmx-api）只需关注"我想要流还是非流"。
pub async fn deploy_stream(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    match deploy_with_events(db_id, items, operator_id, operator_name, Some(tx)).await {
        // 成功：推 done 事件（含 results / batch_id / db_state）
        Ok(v) => {
            let _ = tx.send(ev(
                "done",
                json!({
                    "message": "部署执行完成",
                    "results": v.get("results").cloned().unwrap_or_else(|| json!([])),
                    "batch_id": v.get("batch_id").cloned().unwrap_or(Value::Null),
                    "db_state": v.get("db_state").cloned().unwrap_or(Value::Null),
                }),
            ));
        }
        // 失败：推 error 事件（不 panic；让前端按事件展示错误并继续）
        Err(e) => {
            let _ = tx.send(ev(
                "error",
                json!({ "stage": "deploy", "message": e.to_string() }),
            ));
        }
    }
}

/// 流式生成部署计划：编译定义并汇总将处理的表，不执行 DDL、不写台账。
pub async fn deploy_plan_stream(
    db_id: &str,
    items: &[Value],
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    let send = |kind: &str, data: Value| {
        let _ = tx.send(ev(kind, data));
    };
    send(
        "connect",
        json!({ "message": format!("检查目标数据库 {db_id} 的模型中心状态 …"), "db_id": db_id }),
    );
    if let Err(e) = ensure_current_ledger_schema(db_id).await {
        send(
            "error",
            json!({ "stage": "state", "message": e.to_string() }),
        );
        return;
    }
    match read_meta(db_id).await {
        Ok(Some(_)) => send(
            "connect",
            json!({ "ok": true, "message": "模型中心已初始化，可生成部署计划" }),
        ),
        Ok(None) => {
            send(
                "error",
                json!({ "stage": "state", "message": "数据库尚未初始化，请先初始化模型中心" }),
            );
            return;
        }
        Err(e) => {
            send(
                "error",
                json!({ "stage": "state", "message": format!("读取模型中心状态失败: {e}") }),
            );
            return;
        }
    }

    send(
        "step",
        json!({ "message": format!("开始生成 {} 个模块定义的部署计划", items.len()), "total": items.len() }),
    );
    let deploy_items = parse_deploy_items(items);
    let mut results = Vec::new();
    let mut total_tables = 0usize;
    for (idx, (_orig_idx, item)) in deploy_items.iter().enumerate() {
        let kind = &item.kind;
        let domain = &item.domain;
        let app = &item.app;
        let module = &item.module;
        let file = &item.file;
        send(
            "step",
            json!({
                "message": format!("[{}/{}] 分析 {}/{}/{} · {} · {}", idx + 1, items.len(), domain, app, module, kind, file),
                "index": idx + 1,
                "total": items.len(),
                "module": module,
                "kind": kind,
                "file": file,
            }),
        );
        if kind == "SEED" {
            // 只读预览：扫描 seed 文件 + 检查目标表是否已建（不写库）
            let seed_files = seed_scanner::scan_seed_files(domain, app, module);
            let total_rows: usize = seed_files.iter().map(|f| f.row_count).sum();
            let mut detail = Vec::with_capacity(seed_files.len());
            for f in &seed_files {
                let exists = table_exists(db_id, &f.table_name).await.unwrap_or(false);
                detail.push(json!({
                    "table": f.table_name,
                    "rows": f.row_count,
                    "table_exists": exists,
                }));
            }
            send(
                "progress",
                json!({
                    "message": if detail.is_empty() {
                        format!("{module} · SEED 无种子文件，计划中将跳过")
                    } else {
                        format!("{module} · SEED 将写入 {} 张表 / {} 行种子数据", detail.len(), total_rows)
                    },
                    "module": module,
                    "kind": "SEED",
                    "tables": detail.len(),
                    "rows": total_rows,
                }),
            );
            results.push(json!({
                "module": module,
                "kind": "SEED",
                "tables": detail.len(),
                "rows": total_rows,
                "detail": detail,
                "note": if detail.is_empty() { "无种子文件".to_string() }
                        else { format!("将写入 {} 张表 / {} 行种子数据", detail.len(), total_rows) },
            }));
            continue;
        }
        if kind == "MENU" {
            // 只读预览：扫描 menu 文件 + 统计节点数（不写库）
            let menu_files = seed_scanner::scan_menu_files(domain, app, module);
            let total_nodes: usize = menu_files.iter().map(|f| f.row_count).sum();
            let detail: Vec<Value> = menu_files
                .iter()
                .map(|f| json!({ "file": f.rel_path, "nodes": f.row_count }))
                .collect();
            send(
                "progress",
                json!({
                    "message": if menu_files.is_empty() {
                        format!("{module} · MENU 无菜单文件，计划中将跳过")
                    } else {
                        format!("{module} · MENU 将同步 {} 个菜单文件 / {} 个节点（先删后插 module={}）", menu_files.len(), total_nodes, module)
                    },
                    "module": module,
                    "kind": "MENU",
                    "files": menu_files.len(),
                    "nodes": total_nodes,
                }),
            );
            results.push(json!({
                "module": module,
                "kind": "MENU",
                "files": menu_files.len(),
                "nodes": total_nodes,
                "detail": detail,
                "note": if menu_files.is_empty() { "无菜单文件".to_string() }
                        else { format!("将同步 {} 个菜单文件 / {} 个节点（先删后插 module={}）", menu_files.len(), total_nodes, module) },
            }));
            continue;
        }
        let (defs, src) = match compile_definition(kind, domain, app, module, file).await {
            Ok(x) => x,
            Err(e) => {
                send(
                    "error",
                    json!({ "stage": "compile", "module": module, "kind": kind, "message": e.to_string() }),
                );
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e.to_string() }));
                continue;
            }
        };
        let version = src
            .get("moduleMeta")
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        total_tables += defs.len();
        let mut changes = Vec::new();
        let mut inspect_error: Option<String> = None;
        for def in &defs {
            match table_change_plan(db_id, def).await {
                Ok(ch) => changes.push(ch),
                Err(e) => {
                    let msg = e.to_string();
                    send(
                        "error",
                        json!({ "stage": "inspect", "module": module, "kind": kind, "table": def.table_name, "message": msg }),
                    );
                    inspect_error = Some(msg);
                    break;
                }
            }
        }
        if let Some(e) = inspect_error {
            results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e }));
            continue;
        }
        send(
            "progress",
            json!({
                "message": format!("{module} · {kind} 计划建表/升级 {} 张表，定义版本 v{}", defs.len(), version),
                "module": module,
                "kind": kind,
                "version": version,
                "tables": defs.len(),
                "changes": changes,
            }),
        );
        results.push(json!({
            "module": module,
            "kind": kind,
            "status": "planned",
            "version": version,
            "tables": defs.len(),
            "table_names": defs.iter().map(|d| d.table_name.clone()).collect::<Vec<String>>(),
            "changes": changes,
        }));
    }

    let failed = results
        .iter()
        .filter(|r| r.get("status").and_then(|v| v.as_str()) == Some("failed"))
        .count();
    if failed > 0 {
        send(
            "error",
            json!({ "stage": "plan", "message": format!("计划生成完成，但有 {failed} 项编译失败，请返回调整选择") }),
        );
        return;
    }
    send(
        "done",
        json!({
            "message": "部署执行计划已生成，请审核后确认是否执行",
            "results": results,
            "total": items.len(),
            "tables": total_tables,
            "db_id": db_id,
        }),
    );
}

async fn deploy_with_events(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
    tx: Option<&tokio::sync::mpsc::UnboundedSender<InitEvent>>,
) -> Result<Value> {
    let current_app_id = cmx_utils::ConfigManager::global().get_app_id();
    let send = |kind: &str, data: Value| {
        if let Some(tx) = tx {
            let _ = tx.send(ev(kind, data));
        }
    };

    // 前置：库必须已初始化
    send(
        "connect",
        json!({ "message": format!("检查目标数据库 {db_id} 的模型中心状态 …"), "db_id": db_id }),
    );
    ensure_current_ledger_schema(db_id).await?;
    if read_meta(db_id).await?.is_none() {
        return Err(Error::BadRequest(
            "数据库尚未初始化，请先初始化模型中心".into(),
        ));
    }
    let mm = get_default_db_manager();
    let batch_id = snowflake_id_str();
    let mut results = Vec::new();
    send(
        "step",
        json!({ "message": format!("开始部署 {} 个模块定义", items.len()), "batch_id": batch_id, "total": items.len() }),
    );

    let deploy_items = parse_deploy_items(items);

    // 预加载主库 cmx_module.name（按 code 匹配）；部署写台账时 module_name 用此权威值，
    // 不再取定义文件 moduleMeta.metaName（那是元数据标题，非模块名）
    let main_names = read_main_module_names().await;

    for (idx, (_orig_idx, item)) in deploy_items.iter().enumerate() {
        let kind = &item.kind;
        let domain = &item.domain;
        let app = &item.app;
        let module = &item.module;
        let file = &item.file;
        send(
            "step",
            json!({
                "message": format!("[{}/{}] 准备部署 {}/{}/{} · {} · {}", idx + 1, items.len(), domain, app, module, kind, file),
                "index": idx + 1,
                "total": items.len(),
                "module": module,
                "kind": kind,
                "file": file,
            }),
        );
        if kind == "SEED" {
            // SEED 走完整部署流程（依赖目标表已由 DCT 路径建好）
            let result = deploy_seed_menu::deploy_seed_with_events(
                db_id,
                domain,
                app,
                module,
                operator_id,
                operator_name,
                tx,
            )
            .await?;
            results.push(result);
            continue;
        }
        if kind == "MENU" {
            // MENU 走完整部署流程（菜单数据写平台库，台账/历史写目标库）
            let result = deploy_seed_menu::deploy_menu_with_events(
                db_id,
                domain,
                app,
                module,
                operator_id,
                operator_name,
                tx,
            )
            .await?;
            results.push(result);
            continue;
        }
        let started = std::time::Instant::now();
        let hist_id = snowflake_id_str();
        // 历史：pending→executing（对账锚点，失败不阻断部署）
        send(
            "progress",
            json!({ "message": "写入执行历史锚点 …", "module": module, "kind": kind }),
        );
        let _ = insert_history_executing(
            LedgerCtx { db_id, txn_id: None, operator_id, operator_name },
            ModuleId { domain, app, module, kind },
            &hist_id, &batch_id, "create", file,
        ).await;

        // 编译
        send(
            "progress",
            json!({ "message": "编译定义 JSON 为数据库表结构 …", "module": module, "kind": kind, "file": file }),
        );
        let (defs, src) = match compile_definition(kind, domain, app, module, file).await {
            Ok(x) => x,
            Err(e) => {
                let _ = fail_history(
                    &hist_id,
                    db_id,
                    &e.to_string(),
                    started.elapsed().as_millis() as i64,
                )
                .await;
                send(
                    "error",
                    json!({ "stage": "compile", "module": module, "kind": kind, "message": e.to_string() }),
                );
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e.to_string() }));
                continue;
            }
        };
        let version = src
            .get("moduleMeta")
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        // module_name 权威来源：主库 cmx_module.name；缺失时回退到 module 短 id。
        // 不取定义文件 moduleMeta.metaName（那是元数据标题，非模块名，会污染台账）。
        let module_name = main_names
            .get(&main_module_key(domain, app, module))
            .cloned()
            .unwrap_or_else(|| module.to_string());
        send(
            "progress",
            json!({ "message": format!("编译完成：{} 张表，定义版本 v{}", defs.len(), version), "module": module, "kind": kind, "tables": defs.len(), "version": version }),
        );

        // 建表（DDL 自动提交，txn_id=None；内省 diff，additive-only）
        let executor = PgTableDefineExecutor::new(db_id.to_string(), None);
        let mut created = 0i64;
        let mut changes = Vec::new();
        let mut ddl_err: Option<String> = None;
        for (table_idx, def) in defs.iter().enumerate() {
            let change = match table_change_plan(db_id, def).await {
                Ok(ch) => ch,
                Err(e) => {
                    ddl_err = Some(format!("内省表 {} 失败: {e}", def.table_name));
                    break;
                }
            };
            send(
                "progress",
                json!({
                    "message": format!("[{}/{}] {} {}", table_idx + 1, defs.len(), table_action_label(&change), def.table_name),
                    "module": module,
                    "kind": kind,
                    "table": def.table_name,
                    "index": table_idx + 1,
                    "total": defs.len(),
                    "change": change,
                }),
            );
            if let Err(e) = executor.create_or_upgrade_table(def).await {
                ddl_err = Some(format!("建表 {} 失败: {e}", def.table_name));
                break;
            }
            changes.push(change);
            created += 1;
        }
        if let Some(e) = ddl_err {
            let _ = fail_history(&hist_id, db_id, &e, started.elapsed().as_millis() as i64).await;
            send(
                "error",
                json!({ "stage": "ddl", "module": module, "kind": kind, "message": e }),
            );
            results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e, "changes": changes }));
            continue;
        }

        // 台账 DML（事务）：源 JSON 留档 + 模块态 UPSERT + 对象台账 + 历史 success。
        // 台账 schema 已在入口统一 ensure，避免浏览、计划、执行三条链路各自补列。
        send(
            "progress",
            json!({ "message": "写入台账事务：源 JSON、模块版本、部署历史 …", "module": module, "kind": kind }),
        );
        let ctx = mm.get_transaction_context();
        let guard = match ctx.begin_with_guard(db_id).await {
            Ok(g) => g,
            Err(e) => {
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": format!("事务失败: {e}") }));
                continue;
            }
        };
        let txn = guard.txn_id().to_string();

        let tx_result: Result<()> = async {
            // 4-a 源 JSON 留档：翻旧版 is_current=0，插新版
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
            "UPDATE cmx_model_source SET is_current = 0 WHERE db_id=$1 AND app_id=$2 AND domain_code=$3 AND application_code=$4 AND module_code=$5 AND kind=$6",
            vec![DataValue::String(db_id.into()), DataValue::String(current_app_id.clone()), DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(kind.clone())])
                .await.map_err(db_err("更新源 JSON 当前标记失败"))?;

            let compiled = serde_json::to_value(&defs).unwrap_or(Value::Null);
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
            "INSERT INTO cmx_model_source (id, db_id, app_id, domain_code, application_code, module_code, module_name, kind, version, source_file, source_json, compiled_json, table_count, is_current, engine_version, imported_by, imported_name) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,$12::jsonb,$13,1,$14,$15,$16) ON CONFLICT (db_id, app_id, domain_code, application_code, module_code, kind, version) DO UPDATE SET source_json=EXCLUDED.source_json, compiled_json=EXCLUDED.compiled_json, is_current=1, imported_at=CURRENT_TIMESTAMP",
            vec![
                DataValue::String(snowflake_id_str()), DataValue::String(db_id.into()), DataValue::String(current_app_id.clone()),
                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(module_name.clone()),
                DataValue::String(kind.clone()), DataValue::String(version.to_string()), DataValue::String(file.into()),
                DataValue::Json(serde_json::to_string(&src).unwrap_or_default()), DataValue::Json(serde_json::to_string(&compiled).unwrap_or_default()),
                DataValue::Int(created), DataValue::String(ENGINE_VERSION.into()), DataValue::String(operator_id.into()), DataValue::String(operator_name.into()),
            ]).await.map_err(db_err("写源 JSON 失败"))?;

            // 4-b 模块态 UPSERT：主表只存模块身份；kind 明细按行存版本/状态。
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
            "INSERT INTO cmx_model_module (id, db_id, app_id, domain_code, application_code, module_code, module_name, table_count, first_deployed_at, current_deployed_at, deployed_by, deployed_name) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,$9,$10) \
             ON CONFLICT (db_id, app_id, domain_code, application_code, module_code) DO UPDATE SET module_name=EXCLUDED.module_name, table_count=GREATEST(COALESCE(cmx_model_module.table_count,0), COALESCE(EXCLUDED.table_count,0)), current_deployed_at=CURRENT_TIMESTAMP, deployed_by=EXCLUDED.deployed_by, deployed_name=EXCLUDED.deployed_name, update_time=CURRENT_TIMESTAMP",
            vec![
                DataValue::String(snowflake_id_str()), DataValue::String(db_id.into()), DataValue::String(current_app_id.clone()),
                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(module_name.clone()),
                DataValue::Int(created), DataValue::String(operator_id.into()), DataValue::String(operator_name.into()),
            ]).await.map_err(db_err("写模块台账失败"))?;

            upsert_module_kind(
                LedgerCtx { db_id, txn_id: Some(&txn), operator_id, operator_name },
                ModuleId { domain, app, module, kind },
                &version.to_string(), created, file,
                // 内容 checksum：版本未变但内容已改时，矩阵靠它检出 drift
                Some(&crate::checksum::normalized_def_checksum(&src)),
            ).await.map_err(db_err("写模块类型台账失败"))?;

            // 4-c 对象台账
            register_table_defines(
                db_id, &txn, &defs,
                domain, app, module,
                &version.to_string(), &current_app_id,
            ).await?;

            // 4-d 历史 → success
            update_history_success(
                LedgerCtx { db_id, txn_id: Some(&txn), operator_id, operator_name },
                &hist_id,
                Some(&version.to_string()), created, 0,
                &serde_json::to_string(&changes).unwrap_or_else(|_| "[]".to_string()),
                started.elapsed().as_millis() as i64,
            ).await.map_err(db_err("更新历史失败"))?;
            Ok(())
        }.await;

        if let Err(e) = tx_result {
            let msg = e.to_string();
            let _ = guard.rollback().await;
            let _ = fail_history(&hist_id, db_id, &msg, started.elapsed().as_millis() as i64).await;
            send(
                "error",
                json!({ "stage": "ledger", "module": module, "kind": kind, "message": msg }),
            );
            results
                .push(json!({ "module": module, "kind": kind, "status": "failed", "error": msg }));
            continue;
        }

        guard.commit().await.map_err(db_err("提交部署事务失败"))?;
        send(
            "progress",
            json!({ "message": format!("{module} · {kind} 部署成功：v{version}，{} 张表", created), "module": module, "kind": kind, "status": "success", "version": version, "tables": created, "changes": changes }),
        );
        results.push(json!({ "module": module, "kind": kind, "status": "success", "version": version, "tables": created, "changes": changes }));
    }

    Ok(
        json!({ "ok": true, "batch_id": batch_id, "results": results, "db_state": db_state(db_id).await? }),
    )
}

/// 对象台账登记：把编译产出的 TableDefine 写入 cmx_meta_table_define（主表）
/// 和 cmx_meta_table_define_version（版本快照），check-then-upsert 避免重复行。
///
/// 仅在目标库存在这两张元数据表时执行（模型中心库只初始化自身台账，不强依赖）。
#[allow(clippy::too_many_arguments)]
async fn register_table_defines(
    db_id: &str,
    txn: &str,
    defs: &[TableDefine],
    domain: &str,
    app: &str,
    module: &str,
    version_str: &str,
    current_app_id: &str,
) -> Result<()> {
    let mm = get_default_db_manager();
    let has_meta_table = table_exists(db_id, "cmx_meta_table_define")
        .await
        .unwrap_or(false);
    let has_table_define_version = table_exists(db_id, "cmx_meta_table_define_version")
        .await
        .unwrap_or(false);
    if !has_meta_table || !has_table_define_version {
        return Ok(());
    }
    for def in defs {
        let meta_json = serde_json::to_string(def).unwrap_or_default();

        // (1) 主表 cmx_meta_table_define:check-then-upsert(无 table_name 唯一约束)
        let main_check = mm.query_sql_with_datavalues(db_id, Some(txn),
            "SELECT id FROM cmx_meta_table_define WHERE table_name = $1 AND archived = 0",
            vec![DataValue::String(def.table_name.clone())],
            "deploy_meta_check")
            .await
            .map_err(db_err("查询表定义主表失败"))?;
        let existing_main_id = serde_json::to_value(&main_check).ok()
            .and_then(|j| j.get("rows").and_then(|r| r.as_array()).and_then(|rows| rows.first()).cloned())
            .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(String::from));
        if let Some(eid) = existing_main_id {
            // 已存在 → UPDATE
            mm.execute_sql_with_datavalues(db_id, Some(txn),
                "UPDATE cmx_meta_table_define SET display_name = $1, domain_code = $2, application_code = $3, module_code = $4, db_id = $5, version = $6, ddl_status = 'completed', update_time = CURRENT_TIMESTAMP WHERE id = $7",
                vec![
                    DataValue::String(def.display_name.clone()),
                    DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                    DataValue::String(db_id.into()), DataValue::String(version_str.to_string()),
                    DataValue::String(eid),
                ]).await.map_err(db_err("更新表定义主表失败"))?;
        } else {
            // 不存在 → INSERT(完整 12 列,含 app_id/plugin_id=NULL/archived=0)
            let main_id = snowflake_id_str();
            mm.execute_sql_with_datavalues(db_id, Some(txn),
                "INSERT INTO cmx_meta_table_define (id, table_name, display_name, db_id, plugin_id, version, app_id, ddl_status, domain_code, application_code, module_code, archived) VALUES ($1,$2,$3,$4,NULL,$5,$6,'completed',$7,$8,$9,0)",
                vec![
                    DataValue::String(main_id), DataValue::String(def.table_name.clone()), DataValue::String(def.display_name.clone()),
                    DataValue::String(db_id.into()), DataValue::String(version_str.to_string()), DataValue::String(current_app_id.to_string()),
                    DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                ]).await.map_err(db_err("写表定义主表失败"))?;
        }

        // (2) 版本表 cmx_meta_table_define_version:check-then-upsert
        //     按 (table_name, version, app_id) 去重,避免重复部署累积重复行
        let ver_check = mm.query_sql_with_datavalues(db_id, Some(txn),
            "SELECT id FROM cmx_meta_table_define_version WHERE table_name = $1 AND version = $2 AND app_id = $3 AND archived = 0",
            vec![DataValue::String(def.table_name.clone()), DataValue::String(version_str.to_string()), DataValue::String(current_app_id.to_string())],
            "deploy_meta_ver_check")
            .await
            .map_err(db_err("查询表定义版本表失败"))?;
        let existing_ver_id = serde_json::to_value(&ver_check).ok()
            .and_then(|j| j.get("rows").and_then(|r| r.as_array()).and_then(|rows| rows.first()).cloned())
            .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(String::from));
        if let Some(vid) = existing_ver_id {
            // 已存在 → UPDATE metadata + 基础字段
            mm.execute_sql_with_datavalues(db_id, Some(txn),
                "UPDATE cmx_meta_table_define_version SET display_name = $1, db_id = $2, domain_code = $3, application_code = $4, module_code = $5, metadata = $6::jsonb, update_time = CURRENT_TIMESTAMP WHERE id = $7",
                vec![
                    DataValue::String(def.display_name.clone()),
                    DataValue::String(db_id.into()),
                    DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                    DataValue::Json(meta_json),
                    DataValue::String(vid),
                ]).await.map_err(db_err("更新表定义版本快照失败"))?;
        } else {
            // 不存在 → INSERT 完整 12 列
            let vid = snowflake_id_str();
            mm.execute_sql_with_datavalues(db_id, Some(txn),
                "INSERT INTO cmx_meta_table_define_version (id, table_name, display_name, db_id, plugin_id, version, app_id, domain_code, application_code, module_code, metadata, archived) VALUES ($1,$2,$3,$4,NULL,$5,$6,$7,$8,$9,$10::jsonb,0)",
                vec![
                    DataValue::String(vid), DataValue::String(def.table_name.clone()), DataValue::String(def.display_name.clone()),
                    DataValue::String(db_id.into()), DataValue::String(version_str.to_string()), DataValue::String(current_app_id.to_string()),
                    DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                    DataValue::Json(meta_json),
                ]).await.map_err(db_err("写表定义版本快照失败"))?;
        }
    }
    Ok(())
}

/// 把某历史行标记为 failed（DDL 失败等）。
pub(crate) async fn fail_history(hist_id: &str, db_id: &str, err: &str, dur_ms: i64) -> Result<()> {
    let mm = get_default_db_manager();
    mm.execute_sql_with_datavalues(db_id, None,
        "UPDATE cmx_model_deploy_history SET status='failed', error_message=$2, finished_at=CURRENT_TIMESTAMP, duration_ms=$3 WHERE id=$1",
        vec![DataValue::String(hist_id.into()), DataValue::String(err.into()), DataValue::Int(dur_ms)]
    ).await.map_err(db_err("写失败历史失败"))?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════
//  数据库辅助函数：DCT / SEED / MENU 部署路径共用
//
//  说明：deploy_with_events（DCT 路径）与 deploy_seed_menu（SEED/MENU 路径）
//  统一调用本组函数写台账，消除双实现并存与列集漂移风险。
// ════════════════════════════════════════════════════════════════════════

/// 台账写入上下文：目标库 + 事务 + 操作人。
///
/// 多个台账辅助函数（`insert_history_executing` / `update_history_success` /
/// `upsert_module_kind`）共享这组参数，避免 12 参数平铺。
#[derive(Clone, Copy)]
pub(crate) struct LedgerCtx<'a> {
    /// 目标数据库标识（多库环境下指向具体 db）
    pub db_id: &'a str,
    /// 事务 id；DDL 自动提交时为 `None`（如历史锚点、失败历史），事务内为 `Some(txn)`
    pub txn_id: Option<&'a str>,
    /// 操作人 ID（写入 deploy_history.operator_id / module_kind.deployed_by）
    pub operator_id: &'a str,
    /// 操作人姓名（写入 deploy_history.operator_name / module_kind.deployed_name）
    pub operator_name: &'a str,
}

/// 模块标识：三段式 + kind。
///
/// 配合 `LedgerCtx` 使用，作为 deploy_history / module_kind 的模块身份四元组。
#[derive(Clone, Copy)]
pub(crate) struct ModuleId<'a> {
    /// 领域编码（如 "fi"）
    pub domain: &'a str,
    /// 应用编码（如 "cmxfico"）
    pub app: &'a str,
    /// 模块编码（如 "report"）
    pub module: &'a str,
    /// 资源类型（`"DCT"` / `"DOC"` / `"RPT"` / `"SEED"` / `"MENU"`）
    pub kind: &'a str,
}

/// 写入 deploy_history 行：status='executing'，作为部署对账锚点。
///
/// 通常 `txn_id=None`（PG DDL 自动提交前的锚点），失败不阻断主流程由调用方决定。
///
/// 参数：
/// - `txn_id`：事务 id；锚点写一般在事务外，传 `None`。
/// - `action`：动作类型（DCT 用 `'create'`；SEED 用 `'seed'`；MENU 用 `'menu'`）。
/// - `def_ref`：定义来源（DCT 用 file 名；SEED 用 `'seed/'`；MENU 用 `'menu-pages/'`）。
pub(crate) async fn insert_history_executing(
    ctx: LedgerCtx<'_>,
    mid: ModuleId<'_>,
    hist_id: &str,
    batch_id: &str,
    action: &str,
    def_ref: &str,
) -> Result<()> {
    let mm = get_default_db_manager();
    let current_app_id = cmx_utils::ConfigManager::global().get_app_id();
    mm.execute_sql_with_datavalues(
        ctx.db_id,
        ctx.txn_id,
        "INSERT INTO cmx_model_deploy_history (id, batch_id, db_id, app_id, domain_code, application_code, module_code, kind, action, status, def_ref, engine_version, operator_id, operator_name, started_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'executing',$10,$11,$12,$13,CURRENT_TIMESTAMP)",
        vec![
            DataValue::String(hist_id.to_string()),
            DataValue::String(batch_id.to_string()),
            DataValue::String(ctx.db_id.to_string()),
            DataValue::String(current_app_id.clone()),
            DataValue::String(mid.domain.to_string()),
            DataValue::String(mid.app.to_string()),
            DataValue::String(mid.module.to_string()),
            DataValue::String(mid.kind.to_string()),
            DataValue::String(action.to_string()),
            DataValue::String(def_ref.to_string()),
            DataValue::String(ENGINE_VERSION.to_string()),
            DataValue::String(ctx.operator_id.to_string()),
            DataValue::String(ctx.operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写执行历史锚点失败"))?;
    Ok(())
}

/// 把某历史行更新为 success：补 to_version / object_count / seed_rows / ddl_summary / finished_at / duration_ms。
///
/// 在部署事务内执行，`txn_id` 传事务 id。
///
/// 参数：
/// - `to_version`：目标版本（SEED/MENU 无版本概念，传 `None`；DCT 传 `Some("3")` 等）。
/// - `object_count`：对象计数（SEED 用表数；MENU 用节点数；DCT 用建表数）。
/// - `seed_rows`：SEED/MENU 写入行数（DCT 传 `0`）。
/// - `ddl_summary_json`：DDL 摘要 JSON 字符串（写入 jsonb 列）。
/// - `dur_ms`：本次部署耗时毫秒。
pub(crate) async fn update_history_success(
    ctx: LedgerCtx<'_>,
    hist_id: &str,
    to_version: Option<&str>,
    object_count: i64,
    seed_rows: i64,
    ddl_summary_json: &str,
    dur_ms: i64,
) -> Result<()> {
    let mm = get_default_db_manager();
    mm.execute_sql_with_datavalues(
        ctx.db_id,
        ctx.txn_id,
        "UPDATE cmx_model_deploy_history SET status='success', to_version=$2, object_count=$3, seed_rows=$4, ddl_summary=$5::jsonb, finished_at=CURRENT_TIMESTAMP, duration_ms=$6 WHERE id=$1",
        vec![
            DataValue::String(hist_id.to_string()),
            DataValue::String(to_version.map(|s| s.to_string()).unwrap_or_default()),
            DataValue::Int(object_count),
            DataValue::Int(seed_rows),
            DataValue::Json(ddl_summary_json.to_string()),
            DataValue::Int(dur_ms),
        ],
    )
    .await
    .map_err(db_err("更新历史为 success 失败"))?;
    Ok(())
}

/// UPSERT cmx_model_module_kind：记录某模块某 kind 的当前态版本/状态。
///
/// 在部署事务内执行，`txn_id` 传事务 id。
/// `id` 由本函数内部用 `snowflake_id_str()` 生成（INSERT 时使用，UPSERT 命中冲突时被忽略）。
///
/// 参数：
/// - `version`：定义版本（字符串形式，DCT 传 "3"；SEED/MENU 无版本概念传 `""`）。
/// - `table_count`：本模块本 kind 部署的对象数（表数 / 节点数）。
/// - `def_source`：定义来源（DCT 用 file 名；SEED 用 `'seed/'`；MENU 用 `'menu-pages/'`）。
/// - `def_checksum`：内容 checksum（SEED/MENU 用文件聚合 SHA256；DCT/DOC/RPT 用
///   `checksum::normalized_def_checksum`——剔除顶层 `updatedAt` 的规范化 SHA256，
///   供矩阵 drift 检测；`None` 时 COALESCE 保留旧值）。
pub(crate) async fn upsert_module_kind(
    ctx: LedgerCtx<'_>,
    mid: ModuleId<'_>,
    version: &str,
    table_count: i64,
    def_source: &str,
    def_checksum: Option<&str>,
) -> Result<()> {
    let mm = get_default_db_manager();
    let current_app_id = cmx_utils::ConfigManager::global().get_app_id();
    mm.execute_sql_with_datavalues(
        ctx.db_id,
        ctx.txn_id,
        "INSERT INTO cmx_model_module_kind (id, db_id, app_id, domain_code, application_code, module_code, kind, version, status, table_count, def_source, def_checksum, deployed_at, deployed_by, deployed_name) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'current',$9,$10,$11,CURRENT_TIMESTAMP,$12,$13) \
         ON CONFLICT (db_id, app_id, domain_code, application_code, module_code, kind) DO UPDATE SET version=EXCLUDED.version, status='current', table_count=EXCLUDED.table_count, def_source=EXCLUDED.def_source, def_checksum=COALESCE(EXCLUDED.def_checksum, cmx_model_module_kind.def_checksum), deployed_at=CURRENT_TIMESTAMP, deployed_by=EXCLUDED.deployed_by, deployed_name=EXCLUDED.deployed_name, error_message=NULL, update_time=CURRENT_TIMESTAMP",
        vec![
            DataValue::String(snowflake_id_str()),
            DataValue::String(ctx.db_id.to_string()),
            DataValue::String(current_app_id.clone()),
            DataValue::String(mid.domain.to_string()),
            DataValue::String(mid.app.to_string()),
            DataValue::String(mid.module.to_string()),
            DataValue::String(mid.kind.to_string()),
            DataValue::String(version.to_string()),
            DataValue::Int(table_count),
            DataValue::String(def_source.to_string()),
            match def_checksum {
                Some(c) => DataValue::String(c.to_string()),
                None => DataValue::Null,
            },
            DataValue::String(ctx.operator_id.to_string()),
            DataValue::String(ctx.operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写模块类型台账失败"))?;
    Ok(())
}
