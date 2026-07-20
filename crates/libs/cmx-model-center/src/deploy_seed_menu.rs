//! SEED / MENU 部署的业务编排。
//!
//! 本模块对外暴露：
//! - [`deploy_seed_with_events`]: SEED 部署主流程
//! - [`deploy_menu_with_events`]: MENU 部署主流程
//! - [`compile_all_definitions_for_module`]: 聚合编译某模块所有 DCT/DOC/RPT 定义
//! - [`infer_conflict_columns`]: 从 TableDefine 推断 UPSERT 冲突列

use crate::compile_definition;
use crate::{
    insert_history_executing, table_exists, update_history_success, upsert_module_kind, InitEvent,
};
use cmx_api_types::{Error, Result};
use cmx_core::model::cell::{IndexKind, TableDefine};
use cmx_core::model::meta::plugin::SeedDataConfig;
use cmx_database::get_default_db_manager;
use cmx_metadata::seed::PgSeedDataExecutor;
use cmx_model::definitions::store::list_definitions;
use cmx_utils::snowflake_id_str;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::menu_pages_adapter::parse_menu_pages_file;
use crate::seed_scanner::{aggregate_sha256, scan_menu_files, scan_seed_files};
use cmx_biz::menu::LocalMenuDefinitionImporter;
// 引入 trait 到作用域，才能调用 importer.apply_menu_definitions（trait method）
use cmx_traits::resource::MenuDefinitionImporter;

/// 推断 UPSERT 冲突列。
///
/// 优先级：
/// 1. 单列唯一索引（业务编码字段最常见的去重约束）
/// 2. 复合唯一索引（联合唯一，如 (client_id, coa_code)）
/// 3. 主键列
/// 4. 兜底：`["code"]`（cmxfico 数据集永远走这条 —— 文件命名约定为业务编码）
///
/// 返回的列名列表会直接作为 PostgreSQL `INSERT ... ON CONFLICT (...)` 的目标列。
pub fn infer_conflict_columns(def: &TableDefine) -> Vec<String> {
    // 1. 优先：单列唯一索引
    for idx in &def.indexes {
        if matches!(idx.kind, IndexKind::Unique) && idx.columns.len() == 1 {
            return idx.columns.clone();
        }
    }
    // 2. 次：复合唯一索引
    for idx in &def.indexes {
        if matches!(idx.kind, IndexKind::Unique) && !idx.columns.is_empty() {
            return idx.columns.clone();
        }
    }
    // 3. 主键
    if !def.primary_keys.is_empty() {
        return def.primary_keys.clone();
    }
    // 4. 兜底（cmxfico 数据集永远走这条）
    vec!["code".to_string()]
}

/// 聚合编译某模块下所有 DCT/DOC/RPT 定义 → `Vec<TableDefine>`。
///
/// 流程：
/// 1. 调 `list_definitions(None, domain, app, module)` 列出该模块所有定义文件（不限 kind）。
/// 2. 逐个调 `compile_definition` 编译成 `TableDefine` 列表。
/// 3. 顺序合并所有定义的表，返回聚合结果。
///
/// 文件项中 `kind` / `file` 缺失或为空时跳过（容错），避免脏数据中断整个模块编译。
pub async fn compile_all_definitions_for_module(
    domain: &str,
    app: &str,
    module: &str,
) -> Result<Vec<TableDefine>> {
    let mut all = Vec::new();
    // 列出该模块所有定义文件（不限 kind，DCT/DOC/RPT 全要）
    let files = list_definitions(None, Some(domain), Some(app), Some(module))
        .await
        .map_err(|e| Error::InternalError(format!("list_definitions 失败: {e}")))?;

    for f in files {
        let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if kind.is_empty() || file.is_empty() {
            continue;
        }
        let (defs, _src) = compile_definition(kind, domain, app, module, file).await?;
        all.extend(defs);
    }
    Ok(all)
}

// ════════════════════════════════════════════════════════════════════════
//  SEED / MENU 部署主流程（Task 6）
//
//  复用 lib.rs 的 3 个辅助函数（Task 5 抽取，Task 6 已修正签名）：
//  - insert_history_executing（已加 action 参数）
//  - update_history_success（已加 seed_rows 参数）
//  - upsert_module_kind（已加 def_checksum 参数）
//  以及 lib.rs 的私有辅助：table_exists / fail_history / ev。
// ════════════════════════════════════════════════════════════════════════

/// SSE 事件推送的简写：`tx=None` 时静默。
fn send(tx: Option<&UnboundedSender<InitEvent>>, kind: &str, data: Value) {
    if let Some(tx) = tx {
        let _ = tx.send(crate::ev(kind, data));
    }
}

/// SEED 部署主流程：扫描 seed/*.json → 编译元定义 → 校验表已建 → 写业务库 → 写台账。
///
/// 关键阶段：
/// 1. 写历史锚点（`status='executing'`，`action='seed'`，`def_ref='seed/'`）
/// 2. 扫描 `data/meta/definitions/<domain>/<app>/<module>/seed/*.json`
/// 3. 重读 DCT/DOC 元定义 JSON，编译为 `TableDefine` 列表
/// 4. 构造 `SeedDataConfig`（冲突列从 `TableDefine.indexes`/`primary_keys` 推断）
/// 5. 前置校验：目标表必须已建（否则失败并标历史 failed）
/// 6. 事务内执行 `PgSeedDataExecutor`；写 `cmx_model_module_kind` 台账
/// 7. 更新历史为 success（`object_count`=表数，`seed_rows`=成功行数）
///
/// 无 seed 文件时返回 `status='skipped'`，不算错误。
pub async fn deploy_seed_with_events(
    db_id: &str,
    domain: &str,
    app: &str,
    module: &str,
    operator_id: &str,
    operator_name: &str,
    tx: Option<&UnboundedSender<InitEvent>>,
) -> Result<Value> {
    let started = std::time::Instant::now();
    let mm = get_default_db_manager();
    let batch_id = snowflake_id_str();
    let hist_id = snowflake_id_str();

    // 1. 写历史锚点 executing（事务外，txn_id=None；action='seed'，def_ref='seed/'）
    insert_history_executing(
        db_id,
        None,
        &hist_id,
        &batch_id,
        domain,
        app,
        module,
        "SEED",
        "seed",
        "seed/",
        operator_id,
        operator_name,
    )
    .await?;

    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在加载种子数据文件清单…") }),
    );

    // 2. 扫描 seed 文件
    let seed_files = scan_seed_files(domain, app, module);
    if seed_files.is_empty() {
        // 无种子文件 → 标记 success（seed_rows=0），不算错误
        let _ = update_history_success(
            db_id,
            None,
            &hist_id,
            None,
            0,
            0,
            "{}",
            started.elapsed().as_millis() as i64,
        )
        .await;
        return Ok(json!({
            "module": module, "kind": "SEED", "status": "skipped", "note": "无种子文件",
            "tables": 0, "rows": 0
        }));
    }

    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在编译表元定义…") }),
    );

    // 3. 重读 DCT/DOC 元定义 JSON 编译（用于冲突列推断 + PgSeedDataExecutor 内部表结构）
    let table_defines = compile_all_definitions_for_module(domain, app, module).await?;
    let table_map: std::collections::HashMap<&str, &TableDefine> = table_defines
        .iter()
        .map(|td| (td.table_name.as_str(), td))
        .collect();

    // 4. 构造 SeedDataConfig（冲突列从 TableDefine 推断）
    // base_path 走 data_root 解析（portal.data_root → CMX_PORTAL_DATA_ROOT → ./data），
    // 与 cmx-model::definitions::store 保持一致，避免硬编码相对路径在非默认 cwd 失效。
    let base_path = cmx_model::config::data_path(["meta", "definitions"]);
    let seed_configs: Vec<SeedDataConfig> = seed_files
        .iter()
        .map(|f| {
            let conflict = table_map
                .get(f.table_name.as_str())
                .map(|td| infer_conflict_columns(td))
                .unwrap_or_default();
            SeedDataConfig {
                table_name: f.table_name.clone(),
                file: format!("{domain}/{app}/{module}/seed/{}.json", f.table_name),
                conflict_columns: conflict,
                enabled: true,
            }
        })
        .collect();

    // 5. 前置校验：目标表必须已建（DDL 走 DCT 路径，SEED 不负责建表）
    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在校验目标表是否已建…") }),
    );
    for cfg in &seed_configs {
        if !table_exists(db_id, &cfg.table_name).await? {
            let err_msg = format!("表 {} 不存在，请先部署 DCT 元定义", cfg.table_name);
            send(tx, "error", json!({ "message": err_msg }));
            let _ = crate::fail_history(
                &hist_id,
                db_id,
                &err_msg,
                started.elapsed().as_millis() as i64,
            )
            .await;
            return Err(Error::InternalError(err_msg));
        }
    }

    // 6. 执行 PgSeedDataExecutor（事务内）；写台账
    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在写入种子数据…") }),
    );
    let guard = mm
        .get_transaction_context()
        .begin_with_guard(db_id)
        .await
        .map_err(|e| Error::InternalError(format!("开启事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();
    let executor = PgSeedDataExecutor::new(db_id.to_string(), Some(txn_id.clone()));
    let summary = executor
        .execute_all_seed_data(&table_defines, &seed_configs, &base_path)
        .await;
    let total_success = summary.total_success() as i64;
    let total_failed = summary.total_failed() as i64;

    // 写台账（事务内）：SEED 无版本概念，version 传空串
    let module_checksum = aggregate_sha256(&seed_files);
    upsert_module_kind(
        db_id,
        Some(&txn_id),
        domain,
        app,
        module,
        "SEED",
        "",
        seed_files.len() as i64,
        "seed/",
        Some(&module_checksum),
        operator_id,
        operator_name,
    )
    .await?;

    guard
        .commit()
        .await
        .map_err(|e| Error::InternalError(format!("提交事务失败: {e}")))?;

    // 7. 更新历史 success（object_count=表数，seed_rows=成功行数）
    let ddl_summary = json!({
        "tables": seed_files.len(),
        "rows_success": total_success,
        "rows_failed": total_failed,
        "checksum": module_checksum,
    });
    let _ = update_history_success(
        db_id,
        None,
        &hist_id,
        None,
        seed_files.len() as i64,
        total_success,
        &ddl_summary.to_string(),
        started.elapsed().as_millis() as i64,
    )
    .await;

    send(
        tx,
        "done",
        json!({
            "module": module, "kind": "SEED", "status": "success",
            "tables": seed_files.len(), "rows": total_success, "failed": total_failed
        }),
    );

    Ok(json!({
        "module": module, "kind": "SEED", "status": "success",
        "tables": seed_files.len(), "rows": total_success, "failed": total_failed,
        "checksum": module_checksum
    }))
}

/// MENU 部署主流程：扫描 menu-pages JSON → 适配转换 → 调 LocalMenuDefinitionImporter → 写台账。
///
/// 关键阶段：
/// 1. 写历史锚点到**平台库**（`action='menu'`，`def_ref='menu-pages/'`）
/// 2. 扫描 `data/menu-pages/<domain>/<app>/<module>/*.json`
/// 3. 解析 + 扁平化为 `MenuDefinition` 列表
/// 4. 调 `LocalMenuDefinitionImporter::apply_menu_definitions`（内部按 module_code 先删后插，自管事务）
/// 5. 写 `cmx_model_module_kind` 台账（平台库，无外部事务包裹，复用 importer 已提交的事务）
/// 6. 更新历史为 success（`object_count`=文件数，`seed_rows`=已应用节点数）
///
/// 无 menu 文件时返回 `status='skipped'`，不算错误。
pub async fn deploy_menu_with_events(
    domain: &str,
    app: &str,
    module: &str,
    operator_id: &str,
    operator_name: &str,
    tx: Option<&UnboundedSender<InitEvent>>,
) -> Result<Value> {
    let started = std::time::Instant::now();
    let mm = get_default_db_manager();
    let platform_db_id = mm.get_default_db_id().await;
    let batch_id = snowflake_id_str();
    let hist_id = snowflake_id_str();

    // 1. 写历史锚点（平台库，事务外，action='menu'，def_ref='menu-pages/'）
    insert_history_executing(
        &platform_db_id,
        None,
        &hist_id,
        &batch_id,
        domain,
        app,
        module,
        "MENU",
        "menu",
        "menu-pages/",
        operator_id,
        operator_name,
    )
    .await?;

    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在加载菜单 JSON…") }),
    );

    // 2. 扫描 menu 文件
    let menu_files = scan_menu_files(domain, app, module);
    if menu_files.is_empty() {
        let _ = update_history_success(
            &platform_db_id,
            None,
            &hist_id,
            None,
            0,
            0,
            "{}",
            started.elapsed().as_millis() as i64,
        )
        .await;
        return Ok(json!({
            "module": module, "kind": "MENU", "status": "skipped", "note": "无菜单文件",
            "files": 0, "nodes": 0
        }));
    }

    // 3. 解析 + 适配转换
    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在解析菜单结构…") }),
    );
    let mut all_defs = Vec::new();
    let mut total_nodes = 0usize;
    for mf in &menu_files {
        let defs = parse_menu_pages_file(&mf.content, domain, app, module)
            .map_err(|e| Error::InternalError(format!("菜单解析失败: {e}")))?;
        total_nodes += defs.len();
        all_defs.extend(defs);
    }
    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 已解析 {total_nodes} 个菜单节点") }),
    );

    // 4. 调 LocalMenuDefinitionImporter（内部按 module_code 先删后插，自管事务）
    send(
        tx,
        "step",
        json!({ "message": format!("{module} · 正在同步菜单到平台库…") }),
    );
    let importer = LocalMenuDefinitionImporter::new(mm.clone(), platform_db_id.clone());
    let applied = importer
        .apply_menu_definitions(domain, app, module, &all_defs)
        .await
        .map_err(|e| Error::InternalError(format!("菜单同步失败: {e:?}")))?;

    // 5. 写台账（平台库，无外部事务包裹，复用 importer 已提交的事务）
    let menu_checksum = aggregate_sha256(&menu_files);
    upsert_module_kind(
        &platform_db_id,
        None,
        domain,
        app,
        module,
        "MENU",
        "",
        menu_files.len() as i64,
        "menu-pages/",
        Some(&menu_checksum),
        operator_id,
        operator_name,
    )
    .await?;

    // 6. 更新历史 success（object_count=文件数，seed_rows=已应用节点数）
    let ddl_summary = json!({
        "files": menu_files.len(),
        "nodes_total": total_nodes,
        "nodes_applied": applied,
        "checksum": menu_checksum,
    });
    let _ = update_history_success(
        &platform_db_id,
        None,
        &hist_id,
        None,
        menu_files.len() as i64,
        applied as i64,
        &ddl_summary.to_string(),
        started.elapsed().as_millis() as i64,
    )
    .await;

    send(
        tx,
        "done",
        json!({
            "module": module, "kind": "MENU", "status": "success",
            "files": menu_files.len(), "nodes": applied
        }),
    );

    Ok(json!({
        "module": module, "kind": "MENU", "status": "success",
        "files": menu_files.len(), "nodes": applied,
        "checksum": menu_checksum
    }))
}
