//! doc_versioning_test —— 方案 B / B1（版本快照接线）真实库端到端测试。
//!
// 下方文档用中文顺序号（1./2./3.）陈述断言，非 Markdown 列表；放行 rustdoc 缩进 lint。
#![allow(clippy::doc_lazy_continuation)]
//!
//! **默认 `#[ignore]`**：需本机可达的 fico 库（含 cmxfico 单据表 + 种子数据）。
//! 手动运行：
//! ```bash
//! FICO_DB_URL=postgres://postgres:postgres@127.0.0.1:5432/fico \
//!   cargo test -p cmx-biz --test doc_versioning_test -- --ignored --nocapture
//! ```
//!
//! 验证链路：真实 `DocSaver::save`（事务）→ 事务内 `DocLoader::load_txn(Some(txn_id))` 重装载
//! → `DocRevision::record` 写 `cmx_doc_revision`。断言：
//!   1. 开启 versioning 的单据 save → 新增一版（rev_no 递增、is_current=1、snapshot 非空、actor 落值）。
//!   2. 再 save 同单 → rev_no+1，旧版 is_current 翻 0。
//!   3. 未开 versioning（同结构定义去掉 flag）save → **无新增版本**。
//! 全程在一个可回滚的清理边界内，测试结束删除自己写入的版本行，不污染库。

use std::path::PathBuf;

use serde_json::{Value, json};

use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig};
use cmx_doc_store_pg::{BatchItem, DocMetaView, DocSaver, SaveCtx, SaveMode};

const DB_ID: &str = "fico_ver_test";

fn fico_url() -> String {
    std::env::var("FICO_DB_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/fico".to_string())
}

/// 仓库根（crate 在 crates/libs/cmx-biz，上溯 3 级）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("定位仓库根失败")
}

fn read_json(rel: &str) -> Value {
    let p = repo_root().join(rel);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("解析 {p:?} 失败: {e}"))
}

/// 读真实 cmxfico v2 定义 + base 字段集，建 DocMetaView。`enable_versioning` 控制是否带 flag。
fn build_meta(enable_versioning: bool) -> DocMetaView {
    let mut doc = read_json("data/meta/definitions/fi/cmxfico/gl/cmxfico_doc_meta_v2.json");
    let base = read_json("data/meta/definitions/base/base_doc_meta_v1.json");
    // 按需覆盖 versioning flag（未开分支：显式关掉，验证「仅开启时记」）
    doc["docMeta"]["versioning"] = json!({ "enabled": enable_versioning });
    DocMetaView::parse(&doc, &base).expect("解析 cmxfico 定义失败")
}

async fn setup_manager() -> DatabaseManager {
    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: fico_url(),
        db_id: DB_ID.to_string(),
        db_name: None,
        db_schema: Some("public".to_string()),
        pool_config: PoolConfig {
            max_connections: 5,
            min_connections: 1,
            connect_timeout: 30,
            acquire_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        },
        health_check_interval: 60,
        health_check_timeout: 5,
        domain_code: None,
        application_code: None,
        module_code: None,
        default: true,
        source_type: None,
    };
    let manager = DatabaseManager::new(DatabaseManagerConfig::default());
    manager
        .register_data_source(db_config)
        .await
        .expect("连 fico 库失败（设 FICO_DB_URL 或确认本机 PG 可达）");
    manager
}

/// 取一个存在的根单据 id（cv_batch 首行）。
async fn pick_root_id(mm: &DatabaseManager) -> String {
    let ds = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            "SELECT id FROM cv_batch ORDER BY id LIMIT 1",
            vec![],
            "pick",
        )
        .await
        .expect("查 cv_batch 失败");
    let idx = ds.schema.get_index("id").expect("无 id 列");
    let dv = ds
        .rows
        .first()
        .and_then(|r| r.get(idx))
        .expect("cv_batch 无数据");
    match dv {
        cmx_core::model::cell::DataValue::Int(i) => i.to_string(),
        other => format!("{other:?}"),
    }
}

/// 统计某单据的版本行数 + 当前版号。
async fn rev_stats(mm: &DatabaseManager, doc_file: &str, root_id: &str) -> (i64, i64) {
    let sql = "SELECT COUNT(*) AS c, COALESCE(MAX(rev_no),0) AS m FROM cmx_doc_revision WHERE doc_file=$1 AND root_id=$2";
    let ds = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            sql,
            vec![
                cmx_core::model::cell::DataValue::String(doc_file.to_string()),
                cmx_core::model::cell::DataValue::String(root_id.to_string()),
            ],
            "rev_stats",
        )
        .await
        .expect("查版本统计失败");
    let g = |name: &str| -> i64 {
        ds.schema
            .get_index(name)
            .and_then(|i| ds.rows.first().and_then(|r| r.get(i)))
            .and_then(|dv| match dv {
                cmx_core::model::cell::DataValue::Int(n) => Some(*n),
                _ => None,
            })
            .unwrap_or(-1)
    };
    (g("c"), g("m"))
}

/// 清理本测试写入的版本行（不污染库）。
async fn cleanup(mm: &DatabaseManager, doc_file: &str, root_id: &str) {
    let _ = mm
        .execute_sql_with_datavalues(
            DB_ID,
            None,
            "DELETE FROM cmx_doc_revision WHERE doc_file=$1 AND root_id=$2",
            vec![
                cmx_core::model::cell::DataValue::String(doc_file.to_string()),
                cmx_core::model::cell::DataValue::String(root_id.to_string()),
            ],
        )
        .await;
}

fn ctx() -> SaveCtx {
    SaveCtx {
        actor_id: 42,
        actor_name: "集成测试".into(),
        doc_file: "cmxfico_doc_meta_v2.json".into(),
        op_override: None,
    }
}

/// merge changeset：更新根层某行的一个可空业务列（不改真实语义，仅触发 update）。
fn merge_update(root_id: &str) -> Value {
    json!({
        "cv_batch": {
            "updated": [ { "id": root_id, "fields": { "period_code": "2026" } } ]
        }
    })
}

#[tokio::test]
#[ignore = "需本机 fico 库，手动 --ignored 运行"]
async fn versioning_records_and_flips() {
    let mm = setup_manager().await;
    let root_id = pick_root_id(&mm).await;
    let doc_file = "cmxfico_doc_meta_v2.json";

    // 干净起点
    cleanup(&mm, doc_file, &root_id).await;
    let (c0, _) = rev_stats(&mm, doc_file, &root_id).await;
    assert_eq!(c0, 0, "清理后应无版本");

    // ── 1. 开启 versioning：save → 记一版 ──
    let meta = build_meta(true);
    assert!(meta.versioning_enabled(), "versioning 应开启");
    let changes = merge_update(&root_id);
    DocSaver::save(&mm, DB_ID, &meta, SaveMode::Merge, &changes, &ctx())
        .await
        .expect("save#1 失败");
    let (c1, m1) = rev_stats(&mm, doc_file, &root_id).await;
    assert_eq!(c1, 1, "第一次 save 应新增 1 版");
    assert_eq!(m1, 1, "首版 rev_no=1");

    // ── 2. 再 save → rev_no=2，旧版翻 is_current=0 ──
    DocSaver::save(&mm, DB_ID, &meta, SaveMode::Merge, &changes, &ctx())
        .await
        .expect("save#2 失败");
    let (c2, m2) = rev_stats(&mm, doc_file, &root_id).await;
    assert_eq!(c2, 2, "第二次 save 应累计 2 版");
    assert_eq!(m2, 2, "次版 rev_no=2");
    // 当前版唯一
    let cur = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            "SELECT COUNT(*) AS c FROM cmx_doc_revision WHERE doc_file=$1 AND root_id=$2 AND is_current=1",
            vec![
                cmx_core::model::cell::DataValue::String(doc_file.to_string()),
                cmx_core::model::cell::DataValue::String(root_id.clone()),
            ],
            "cur",
        )
        .await
        .expect("查当前版失败");
    let cur_c = cur
        .schema
        .get_index("c")
        .and_then(|i| cur.rows.first().and_then(|r| r.get(i)))
        .and_then(|dv| match dv {
            cmx_core::model::cell::DataValue::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(-1);
    assert_eq!(cur_c, 1, "同单只应有一个当前版（旧版已翻 0）");

    // ── 3. actor / snapshot 落值检查 ──
    let detail = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            "SELECT actor_id, actor_name, op, (snapshot IS NOT NULL) AS has_snap FROM cmx_doc_revision WHERE doc_file=$1 AND root_id=$2 AND rev_no=2",
            vec![
                cmx_core::model::cell::DataValue::String(doc_file.to_string()),
                cmx_core::model::cell::DataValue::String(root_id.clone()),
            ],
            "detail",
        )
        .await
        .expect("查版本明细失败");
    let row = detail.rows.first().expect("应有 rev_no=2 行");
    let get_str = |name: &str| -> String {
        detail
            .schema
            .get_index(name)
            .and_then(|i| row.get(i))
            .map(|dv| match dv {
                cmx_core::model::cell::DataValue::String(s) => s.clone(),
                other => format!("{other:?}"),
            })
            .unwrap_or_default()
    };
    assert_eq!(get_str("actor_id"), "42", "actor_id 应落 SaveCtx.actor_id");
    assert_eq!(get_str("actor_name"), "集成测试", "actor_name 落值");
    assert_eq!(get_str("op"), "update", "merge 更新根行 op=update");

    cleanup(&mm, doc_file, &root_id).await;
}

#[tokio::test]
#[ignore = "需本机 fico 库，手动 --ignored 运行"]
async fn versioning_disabled_records_nothing() {
    let mm = setup_manager().await;
    let root_id = pick_root_id(&mm).await;
    let doc_file = "cmxfico_doc_meta_v2.json";
    cleanup(&mm, doc_file, &root_id).await;

    // versioning 关闭：save 不应写任何版本
    let meta = build_meta(false);
    assert!(!meta.versioning_enabled());
    DocSaver::save(
        &mm,
        DB_ID,
        &meta,
        SaveMode::Merge,
        &merge_update(&root_id),
        &ctx(),
    )
    .await
    .expect("save 失败");
    let (c, _) = rev_stats(&mm, doc_file, &root_id).await;
    assert_eq!(c, 0, "versioning 关闭时不应记版本");
}

/// 读根行当前 update_time 的 RFC3339 字符串（乐观锁基线）。
async fn root_update_time(mm: &DatabaseManager, root_id: &str) -> Option<String> {
    let ds = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            "SELECT update_time FROM cv_batch WHERE id = $1",
            vec![cmx_core::model::cell::DataValue::Int(
                root_id.parse().unwrap(),
            )],
            "ut",
        )
        .await
        .expect("查 update_time 失败");
    let idx = ds.schema.get_index("update_time")?;
    match ds.rows.first().and_then(|r| r.get(idx)) {
        Some(cmx_core::model::cell::DataValue::DateTime(dt)) => Some(dt.to_rfc3339()),
        _ => None,
    }
}

/// merge changeset：更新根行一个字段，带乐观锁 baseline。
fn merge_update_with_baseline(root_id: &str, baseline: Option<&str>) -> Value {
    json!({
        "cv_batch": {
            "updated": [ {
                "id": root_id,
                "fields": { "period_code": "2026" },
                "baseline": baseline
            } ]
        }
    })
}

#[tokio::test]
#[ignore = "需本机 fico 库，手动 --ignored 运行"]
async fn optimistic_lock_conflict_and_refresh() {
    use cmx_biz::BizError;

    let mm = setup_manager().await;
    let root_id = pick_root_id(&mm).await;
    let doc_file = "cmxfico_doc_meta_v2.json";
    cleanup(&mm, doc_file, &root_id).await;
    let meta = build_meta(true);

    // 起点：先跑一次无基线 save（退化为不加锁），确保根行有一个已知 update_time。
    DocSaver::save(
        &mm,
        DB_ID,
        &meta,
        SaveMode::Merge,
        &merge_update(&root_id),
        &ctx(),
    )
    .await
    .expect("初始化 save 失败");
    let base_now = root_update_time(&mm, &root_id)
        .await
        .expect("应有 update_time");

    // ── 1. 正确基线 → 成功，且 SaveResult 回传新基线（供前端刷新）──
    let r = DocSaver::save(
        &mm,
        DB_ID,
        &meta,
        SaveMode::Merge,
        &merge_update_with_baseline(&root_id, Some(&base_now)),
        &ctx(),
    )
    .await
    .expect("正确基线 save 应成功");
    assert!(r.affected >= 1, "根行应被更新");
    assert!(!r.updated_at.is_empty(), "应回传新基线 updatedAt");
    assert_eq!(r.updated_at[0].id, root_id);
    let refreshed = &r.updated_at[0].update_time;
    assert_ne!(refreshed, &base_now, "新基线应不同于旧基线");

    // ── 2. 陈旧基线（用第一次的 base_now，但库里已被步骤1改新）→ 冲突 409 ──
    let err = DocSaver::save(
        &mm,
        DB_ID,
        &meta,
        SaveMode::Merge,
        &merge_update_with_baseline(&root_id, Some(&base_now)),
        &ctx(),
    )
    .await
    .expect_err("陈旧基线应冲突");
    assert!(
        matches!(err, BizError::Conflict(_)),
        "应为乐观锁冲突 BizError::Conflict，实得 {err:?}"
    );

    // ── 3. 用刷新后的基线 → 再次成功（证明基线刷新闭环）──
    DocSaver::save(
        &mm,
        DB_ID,
        &meta,
        SaveMode::Merge,
        &merge_update_with_baseline(&root_id, Some(refreshed)),
        &ctx(),
    )
    .await
    .expect("刷新后基线 save 应成功");

    cleanup(&mm, doc_file, &root_id).await;
}

/// 取前 N 个根 id。
async fn pick_root_ids(mm: &DatabaseManager, n: i64) -> Vec<String> {
    let ds = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            &format!("SELECT id FROM cv_batch ORDER BY id LIMIT {n}"),
            vec![],
            "pickn",
        )
        .await
        .expect("查 cv_batch 失败");
    let idx = ds.schema.get_index("id").expect("无 id 列");
    ds.rows
        .iter()
        .filter_map(|r| match r.get(idx) {
            Some(cmx_core::model::cell::DataValue::Int(i)) => Some(i.to_string()),
            _ => None,
        })
        .collect()
}

/// 读某根行 period_code 当前值（验证 atomic 回滚未落地用）。
async fn root_period_code(mm: &DatabaseManager, root_id: &str) -> Option<String> {
    let ds = mm
        .query_sql_with_datavalues(
            DB_ID,
            None,
            "SELECT period_code FROM cv_batch WHERE id = $1",
            vec![cmx_core::model::cell::DataValue::Int(
                root_id.parse().unwrap(),
            )],
            "pc",
        )
        .await
        .expect("查 period_code 失败");
    let idx = ds.schema.get_index("period_code")?;
    match ds.rows.first().and_then(|r| r.get(idx)) {
        Some(cmx_core::model::cell::DataValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

#[tokio::test]
#[ignore = "需本机 fico 库，手动 --ignored 运行"]
async fn batch_atomic_all_or_nothing_and_non_atomic_isolation() {
    use cmx_biz::BizError;

    let mm = setup_manager().await;
    let ids = pick_root_ids(&mm, 2).await;
    assert!(ids.len() >= 2, "需至少 2 个根单据");
    let (id_a, id_b) = (ids[0].clone(), ids[1].clone());
    let meta = build_meta(true);
    let sctx = ctx();

    // ── atomic：第 1 单正常、第 2 单陈旧基线冲突 → 整批回滚，第 1 单也不落地 ──
    // 捕获 id_a 原值，用与之不同的合法哨兵；测试末尾复原，保证可重复运行（幂等）。
    let orig_a = root_period_code(&mm, &id_a).await;
    let sentinel = if orig_a.as_deref() == Some("209901") {
        "209902"
    } else {
        "209901"
    };
    let ch_a_ok = json!({ "cv_batch": { "updated": [ { "id": id_a, "fields": { "period_code": sentinel } } ] } });
    let ch_b_conflict = merge_update_with_baseline(&id_b, Some("2000-01-01T00:00:00Z"));
    let items = vec![
        BatchItem {
            meta: &meta,
            mode: SaveMode::Merge,
            changes: &ch_a_ok,
            sctx: &sctx,
        },
        BatchItem {
            meta: &meta,
            mode: SaveMode::Merge,
            changes: &ch_b_conflict,
            sctx: &sctx,
        },
    ];
    let err = DocSaver::save_batch(&mm, DB_ID, &items, true)
        .await
        .expect_err("atomic 批含冲突单应整体失败");
    assert!(
        matches!(err, BizError::Conflict(_)),
        "应为冲突，实得 {err:?}"
    );
    let after_a = root_period_code(&mm, &id_a).await;
    assert_eq!(orig_a, after_a, "atomic 回滚：第 1 单的改动不应落地");
    assert_ne!(after_a.as_deref(), Some(sentinel), "哨兵值不应写入");

    // ── 非 atomic：第 1 单正常、第 2 单冲突 → 第 1 单提交、第 2 单标记失败 ──
    let items2 = vec![
        BatchItem {
            meta: &meta,
            mode: SaveMode::Merge,
            changes: &ch_a_ok,
            sctx: &sctx,
        },
        BatchItem {
            meta: &meta,
            mode: SaveMode::Merge,
            changes: &ch_b_conflict,
            sctx: &sctx,
        },
    ];
    let results = DocSaver::save_batch(&mm, DB_ID, &items2, false)
        .await
        .expect("非 atomic 批本身不应整体 Err");
    assert_eq!(results.len(), 2);
    assert!(results[0].ok, "第 1 单应成功");
    assert!(!results[1].ok, "第 2 单应失败");
    assert!(results[1].error.is_some(), "失败单应带 error");
    assert_eq!(
        root_period_code(&mm, &id_a).await.as_deref(),
        Some(sentinel),
        "非 atomic：第 1 单已提交"
    );

    // 复原 id_a 的 period_code（幂等）+ 清版本行。
    if let Some(orig) = &orig_a {
        let restore = json!({ "cv_batch": { "updated": [ { "id": id_a, "fields": { "period_code": orig } } ] } });
        let _ = DocSaver::save(&mm, DB_ID, &meta, SaveMode::Merge, &restore, &sctx).await;
    }
    cleanup(&mm, "cmxfico_doc_meta_v2.json", &id_a).await;
    cleanup(&mm, "cmxfico_doc_meta_v2.json", &id_b).await;
}
