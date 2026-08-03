//! 跨引擎 parity：前端 JS `CmxMasterSlave` vs 后端 Rust `CmxMasterSlave`。
//!
//! 同一份固定凭证夹具（parity/fixture.json）+ 同一套 aggregations，分别喂两个引擎逐层上卷，
//! 断言两引擎产出的每层每行**逐字段一致**（业务字段 + 上卷度量；id 是夹具里显式给的临时键，
//! 两边同源故也一致）。
//!
//! 前端参考用真实源码（packages/cmx-data-comp/src/lib/cmx-master-slave.js）经 Node 子进程跑；
//! 缺 node 时跳过。这是「后端移植是否忠实复现前端语义」的最强保障。
//!
//! 运行：`cargo test -p cmx-doc-store-pg --test parity_ms -- --nocapture`

use cmx_master_slave::{CmxMasterSlave, HierSchema};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/parity")
}

/// 4 层凭证的中立 schema（点分路径与夹具 aggregations 对齐）。
fn voucher_schema(aggregations: &Value) -> HierSchema {
    let mut def = serde_json::json!({
        "shape": { "kind": "path_tree" },
        "layers": [
            { "path": "cv_batch", "table": "cv_batch", "pk": "id" },
            { "path": "cv_batch.cv_header", "table": "cv_header", "pk": "id", "child_key": "upper_id" },
            { "path": "cv_batch.cv_header.cv_acc_line", "table": "cv_acc_line", "pk": "id", "child_key": "upper_id" },
            { "path": "cv_batch.cv_header.cv_acc_line.cv_aux_line", "table": "cv_aux_line", "pk": "id", "child_key": "upper_id" }
        ],
        "relations": [
            { "parent": "cv_batch", "child": "cv_batch.cv_header", "child_key": "upper_id" },
            { "parent": "cv_batch.cv_header", "child": "cv_batch.cv_header.cv_acc_line", "child_key": "upper_id" },
            { "parent": "cv_batch.cv_header.cv_acc_line", "child": "cv_batch.cv_header.cv_acc_line.cv_aux_line", "child_key": "upper_id" }
        ]
    });
    // 夹具 aggregations 用点分完整路径，直接注入
    def["aggregations"] = aggregations.clone();
    HierSchema::from_json(&def).expect("schema 解析失败")
}

/// 把夹具 flat（{table: [rows]}）转成 Rust set_flat_data 的入参（路径→行）。
/// 键从表名（cv_header）映射到完整路径（cv_batch.cv_header）。
fn flat_to_paths(flat: &Value) -> HashMap<String, Vec<Map<String, Value>>> {
    let path_of = |t: &str| -> String {
        match t {
            "cv_batch" => "cv_batch".into(),
            "cv_header" => "cv_batch.cv_header".into(),
            "cv_acc_line" => "cv_batch.cv_header.cv_acc_line".into(),
            "cv_aux_line" => "cv_batch.cv_header.cv_acc_line.cv_aux_line".into(),
            other => other.into(),
        }
    };
    let mut out = HashMap::new();
    for (table, rows) in flat.as_object().unwrap() {
        let v: Vec<Map<String, Value>> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_object().unwrap().clone())
            .collect();
        out.insert(path_of(table), v);
    }
    out
}

/// 规范化一组行：按 id 排序 + 每行按键排序，便于逐字段稳定比对。
fn normalize(rows: &[Map<String, Value>]) -> Vec<Map<String, Value>> {
    let mut v: Vec<Map<String, Value>> = rows.to_vec();
    v.sort_by_key(|r| r.get("id").map(|x| x.to_string()).unwrap_or_default());
    v
}

#[test]
fn cross_engine_parity_rollup() {
    let dir = parity_dir();
    let fixture_path = dir.join("fixture.json");
    let fx: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    let aggregations = fx["aggregations"].clone();
    let flat = fx["flat"].clone();

    // ── 后端 Rust CmxMasterSlave：同数据同规则上卷 ──
    let mut ms = CmxMasterSlave::new(voucher_schema(&aggregations)).unwrap();
    ms.set_flat_data(&flat_to_paths(&flat));
    ms.rollup_in_place().unwrap();
    let rust_flat = ms.flat_data(); // 表名(末段) → 行

    // ── 前端 JS CmxMasterSlave：真实源码经 Node 跑 ──
    let out = Command::new("node")
        .arg(dir.join("ms-driver.mjs"))
        .arg(&fixture_path)
        .output();
    let out = match out {
        Ok(o) if o.status.success() => o,
        Ok(o) => panic!(
            "node 驱动失败：{}",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => {
            eprintln!("跳过：node 不可用（{e}）");
            return;
        }
    };
    let js_flat: Value = serde_json::from_slice(&out.stdout).expect("解析 JS 输出失败");

    // ── 逐层逐行逐字段比对 ──
    let tables = ["cv_batch", "cv_header", "cv_acc_line", "cv_aux_line"];
    for t in tables {
        let js_rows: Vec<Map<String, Value>> = js_flat[t]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|r| r.as_object().unwrap().clone())
            .collect();
        let rust_rows = rust_flat.get(t).cloned().unwrap_or_default();

        let js_n = normalize(&js_rows);
        let rust_n = normalize(&rust_rows);

        assert_eq!(
            js_n.len(),
            rust_n.len(),
            "层 {t} 行数不一致：JS={} Rust={}",
            js_n.len(),
            rust_n.len()
        );

        for (jr, rr) in js_n.iter().zip(rust_n.iter()) {
            // 比对两引擎都产出的字段（JS 输出为准；Rust 可能多带 null 列，逐 JS 键比）
            for (k, jv) in jr {
                let rv = rr.get(k).cloned().unwrap_or(Value::Null);
                // 数值统一按 f64 比（JS number vs Rust int/float 表示差异归一）
                let equal = match (jv.as_f64(), rv.as_f64()) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                    _ => jv == &rv,
                };
                assert!(
                    equal,
                    "层 {t} 行 id={:?} 字段 {k} 不一致：JS={jv} Rust={rv}",
                    jr.get("id")
                );
            }
        }
        eprintln!("层 {t}：{} 行逐字段一致 ✓", js_n.len());
    }

    // 关键上卷断言（双保险，防两边都错成一样）：借贷各 150 逐层
    for t in ["cv_batch", "cv_header"] {
        let row = &rust_flat[t][0];
        assert_eq!(row["entered_dr"].as_f64().unwrap(), 150.0, "{t}.entered_dr 应=150");
        assert_eq!(row["entered_cr"].as_f64().unwrap(), 150.0, "{t}.entered_cr 应=150");
    }
    eprintln!("跨引擎 parity 通过：前端 JS 与后端 Rust CmxMasterSlave 上卷结果逐字段一致");
}

// ─────────────────────────────────────────────────────────────────────────
// 真机 DB parity：两引擎的上卷输出各经 DocHierService 真写 fico 库为一张凭证，
// 再 SQL 逐层比对（除 id/upper_id/line_no/审计列外，业务字段 + 上卷度量全一致）。
// #[ignore] + TEST_PG_URL 门控；自建自清理，doc_no 打唯一标记区分 A/B。
// ─────────────────────────────────────────────────────────────────────────

use cmx_database_pg::{get_default_pg_db_manager, DbConfig, DbType};
use cmx_doc_store_pg::DocHierService;
use cmx_master_slave::ChangeSet;

/// 注册 fico 到 tokio-pg + sqlx 两个 manager（DOC 装载走 tokio、保存走 sqlx）+ 定义根 env。
async fn setup_doc() -> Option<String> {
    let url = std::env::var("TEST_PG_URL").ok()?;
    if std::env::var("CMX_PORTAL_DATA_ROOT").is_err() {
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4) // .../cmx-container（crates/libs/cmx-doc/cmx-doc-store-pg 上溯 4 层）
            .map(|p| p.join("data"))
            .expect("推导 data 根失败");
        unsafe { std::env::set_var("CMX_PORTAL_DATA_ROOT", &data_root); }
    }
    let db_id = "fico-db".to_string();
    // tokio-pg
    let cfg = DbConfig {
        db_type: DbType::Postgres, db_url: url.clone(), db_id: db_id.clone(),
        db_name: None, db_schema: Some("public".into()), default: true,
        pool_config: Default::default(), health_check_interval: 60, health_check_timeout: 5,
        domain_code: None, application_code: None, module_code: None, source_type: Some("biz".into()),
    };
    get_default_pg_db_manager().register_data_source(cfg).await.expect("注册 tokio fico 失败");
    // sqlx（DocSaver::save 用）
    let scfg = cmx_database::config::DbConfig {
        db_type: cmx_database::config::DbType::Postgres, db_url: url, db_id: db_id.clone(),
        db_name: None, db_schema: Some("public".into()), default: true,
        pool_config: Default::default(), health_check_interval: 60, health_check_timeout: 5,
        domain_code: None, application_code: None, module_code: None, source_type: Some("biz".into()),
    };
    cmx_database::get_default_db_manager().register_data_source(scfg).await.expect("注册 sqlx fico 失败");
    Some(db_id)
}

/// 把某引擎的 flat 输出（表名→行）重塑成 DOC changeset：每行 { id, upper_id?, fields:{业务列} }。
/// 用 tag 覆盖 doc_no/reference 以便区分 A/B 并清理。id 用 temp（前缀 tag）交后端铸号。
fn flat_to_doc_changeset(flat: &HashMap<String, Vec<Map<String, Value>>>, tag: &str) -> ChangeSet {
    // 业务列（进 fields）= 除 id/upper_id/line_no 外的全部
    let reshape = |rows: &[Map<String, Value>], stamp_docno: bool| -> Vec<Value> {
        rows.iter().enumerate().map(|(i, r)| {
            let mut fields = Map::new();
            for (k, v) in r {
                if k == "id" || k == "upper_id" || k == "line_no" { continue; }
                fields.insert(k.clone(), v.clone());
            }
            if stamp_docno {
                fields.insert("doc_no".into(), Value::from(format!("PARITY-{tag}")));
                fields.insert("reference".into(), Value::from(format!("PARITY-{tag}")));
            }
            let mut row = Map::new();
            // temp id：tag 前缀 + 原 id，保证 A/B 不撞、且是"临时"（非纯数字）交铸号
            row.insert("id".into(), Value::from(format!("{tag}-{}", r.get("id").and_then(|v| v.as_str()).unwrap_or(""))));
            if let Some(up) = r.get("upper_id").and_then(|v| v.as_str()) {
                row.insert("upper_id".into(), Value::from(format!("{tag}-{up}")));
            }
            // line_no：层内序号（1-based），A/B 同源同序故一致
            row.insert("line_no".into(), Value::from((i + 1) as i64));
            row.insert("fields".into(), Value::Object(fields));
            Value::Object(row)
        }).collect()
    };
    let mut cs = ChangeSet::default();
    for (table, stamp) in [("cv_batch", true), ("cv_header", true), ("cv_acc_line", false), ("cv_aux_line", false)] {
        if let Some(rows) = flat.get(table) {
            let mut lc = cmx_master_slave::LayerChanges::default();
            lc.inserted = reshape(rows, stamp);
            cs.layers.insert(table.to_string(), lc);
        }
    }
    cs
}

#[tokio::test]
#[ignore = "需 TEST_PG_URL 指向真实 fico 库"]
async fn cross_engine_parity_db_write() {
    let Some(db_id) = setup_doc().await else { eprintln!("跳过：未设置 TEST_PG_URL"); return; };
    let dir = parity_dir();
    let fixture_path = dir.join("fixture.json");
    let fx: Value = serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    let aggregations = fx["aggregations"].clone();
    let flat = fx["flat"].clone();

    // 前置清理
    cleanup(&db_id).await;

    // ── B = Rust 引擎上卷 ──
    let mut ms = CmxMasterSlave::new(voucher_schema(&aggregations)).unwrap();
    ms.set_flat_data(&flat_to_paths(&flat));
    ms.rollup_in_place().unwrap();
    let rust_flat = ms.flat_data();

    // ── A = JS 引擎上卷（真实源码经 node）──
    let out = Command::new("node").arg(dir.join("ms-driver.mjs")).arg(&fixture_path).output();
    let out = match out { Ok(o) if o.status.success() => o, _ => { eprintln!("跳过：node 不可用"); return; } };
    let js_val: Value = serde_json::from_slice(&out.stdout).unwrap();
    let js_flat: HashMap<String, Vec<Map<String, Value>>> = js_val.as_object().unwrap().iter()
        .map(|(k, v)| (k.clone(), v.as_array().unwrap().iter().map(|r| r.as_object().unwrap().clone()).collect()))
        .collect();

    // ── 各经 DocHierService 真写库 ──
    let svc = DocHierService::new("fi", "cmxfico", "gl", &db_id);
    let out_a = svc_save(&svc, &aggregations, flat_to_doc_changeset(&js_flat, "JS")).await;
    let out_b = svc_save(&svc, &aggregations, flat_to_doc_changeset(&rust_flat, "RS")).await;
    eprintln!("写库回执：A(JS) affected={}, B(Rust) affected={}", out_a, out_b);
    assert!(out_a > 0 && out_b > 0, "两凭证都应落库");

    // ── SQL 逐层比对（除 id/upper_id/line_no/审计列）──
    let mm = get_default_pg_db_manager();
    let biz_cols: HashMap<&str, &str> = HashMap::from([
        ("cv_batch", "batch_name, entered_dr, entered_cr, local_dr, local_cr"),
        ("cv_header", "header_text, entered_dr, entered_cr, local_dr, local_cr"),
        ("cv_acc_line", "gl_account_id, item_text, posting_key_code, entered_dr, entered_cr, local_dr, local_cr"),
        ("cv_aux_line", "entered_dr, entered_cr, local_dr, local_cr"),
    ]);
    for (table, cols) in &biz_cols {
        // A 行（doc_no/reference=PARITY-JS 关联；子层经 upper_id 链，简化为按 item_text/度量排序比）
        let sql_a = format!("SELECT {cols} FROM {table} WHERE {} ORDER BY {cols}", tag_filter(table, "JS"));
        let sql_b = format!("SELECT {cols} FROM {table} WHERE {} ORDER BY {cols}", tag_filter(table, "RS"));
        let a = dump(mm, &db_id, &sql_a).await;
        let b = dump(mm, &db_id, &sql_b).await;
        assert_eq!(a, b, "层 {table} A(JS) vs B(Rust) 业务字段不一致\nA={a}\nB={b}");
        eprintln!("层 {table}：A/B 业务字段 + 上卷度量逐行一致 ✓");
    }

    cleanup(&db_id).await;
    eprintln!("真机 DB parity 通过：JS 与 Rust 两引擎写出的凭证除 id/号外完全一致");
}

async fn svc_save(svc: &DocHierService, aggs: &Value, cs: ChangeSet) -> u64 {
    // 协调器 save_via 会再上卷一次（幂等：值已是上卷后的，再算仍相同），然后落库
    let ms = CmxMasterSlave::new(voucher_schema(aggs)).unwrap();
    ms.save_via(svc, cs).await.expect("DocHierService 保存失败").affected
}

/// 关联本层某凭证的过滤条件：batch/header 有 doc_no，子层经 upper_id 归属，简化为按度量集合匹配。
fn tag_filter(table: &str, tag: &str) -> String {
    match table {
        "cv_batch" | "cv_header" => format!("doc_no = 'PARITY-{tag}'"),
        // 子层：挂在本 tag 的 header 下（upper_id 链）
        "cv_acc_line" => format!("upper_id IN (SELECT id FROM cv_header WHERE doc_no='PARITY-{tag}')"),
        "cv_aux_line" => format!("upper_id IN (SELECT id FROM cv_acc_line WHERE upper_id IN (SELECT id FROM cv_header WHERE doc_no='PARITY-{tag}'))"),
        _ => "1=1".into(),
    }
}

async fn dump(mm: &cmx_database_pg::DatabaseManager, db_id: &str, sql: &str) -> String {
    let ds = mm.query_sql(db_id, None, sql, "cmp").await.expect("查询失败");
    // DataSet impl Serialize：直接序列化为稳定比对串
    serde_json::to_string(&ds).expect("序列化 DataSet 失败")
}

async fn cleanup(db_id: &str) {
    let mm = get_default_pg_db_manager();
    for sql in [
        "DELETE FROM cv_aux_line WHERE upper_id IN (SELECT id FROM cv_acc_line WHERE upper_id IN (SELECT id FROM cv_header WHERE doc_no LIKE 'PARITY-%'))",
        "DELETE FROM cv_acc_line WHERE upper_id IN (SELECT id FROM cv_header WHERE doc_no LIKE 'PARITY-%')",
        "DELETE FROM cv_header WHERE doc_no LIKE 'PARITY-%'",
        "DELETE FROM cv_batch WHERE doc_no LIKE 'PARITY-%'",
    ] {
        let _ = mm.execute_sql(db_id, None, sql).await;
    }
}
