//! 真机集成验证：DctHierService + CmxMasterSlave 对 fico 库的 `cf_gl_account`（自分级字典）
//! 端到端装载。验证「服务 impl 协调器 → 协调器装配 ZmcDataSet 成树」这条链在真实 PG 上成立。
//!
//! 运行（在 cmx-container 目录下，定义根 ./data 自动生效）：
//! ```
//! TEST_PG_URL=postgresql://postgres:postgres@127.0.0.1:5432/fico \
//!   cargo test -p cmx-dct-store-pg --test hier_service_pg -- --ignored --nocapture
//! ```
//! 缺 TEST_PG_URL 时跳过（不阻塞离线构建）。

use cmx_database_pg::{get_default_pg_db_manager, DbConfig, DbType};
use cmx_dct_store_pg::DctHierService;
use cmx_master_slave::{CmxMasterSlave, HierSchema, LoadQuery};
use serde_json::json;

/// 注册 fico 数据源（db_id = "fico-db"，对齐 dev-local.toml）。返回 db_id；无 env 则 None。
async fn setup() -> Option<String> {
    let url = std::env::var("TEST_PG_URL").ok()?;
    // 定义根：cargo test 的 cwd 是 crate 目录，故显式指向 cmx-container/data。
    // 允许 env 覆盖；缺省按本文件相对路径推导 workspace 根。
    if std::env::var("CMX_PORTAL_DATA_ROOT").is_err() {
        // crate 目录 = crates/libs/cmx-dct/cmx-dct-store-pg → 上溯 4 层到 cmx-container
        let data_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4) // .../cmx-container
            .map(|p| p.join("data"))
            .expect("推导 data 根失败");
        // SAFETY: 测试单线程 setup 阶段设置进程环境变量，早于任何定义读取。
        unsafe {
            std::env::set_var("CMX_PORTAL_DATA_ROOT", &data_root);
        }
        eprintln!("CMX_PORTAL_DATA_ROOT = {:?}", data_root);
    }
    let db_id = "fico-db".to_string();
    let cfg = DbConfig {
        db_type: DbType::Postgres,
        db_url: url,
        db_id: db_id.clone(),
        db_name: None,
        db_schema: Some("public".to_string()),
        default: true,
        pool_config: Default::default(),
        health_check_interval: 60,
        health_check_timeout: 5,
        domain_code: None,
        application_code: None,
        module_code: None,
        source_type: Some("biz".to_string()),
    };
    get_default_pg_db_manager()
        .register_data_source(cfg)
        .await
        .expect("注册 fico 数据源失败");
    Some(db_id)
}

/// gl_account 的中立 schema：形状 B（自引用），根层表名填 dictCode（DctHierService 据此 resolve）。
fn gl_account_schema() -> HierSchema {
    HierSchema::from_json(&json!({
        "shape": { "kind": "self_ref", "parent_field": "parent_id" },
        "layers": [{
            "path": "dict",
            "table": "gl_account",          // 根层 table = dictCode（适配器约定）
            "pk": "id",
            "child_key": "parent_id",
            "order_key": "sort_no",
            "derived": { "full_path": "full_path", "level_no": "level_no", "is_leaf": "is_leaf" }
        }]
    }))
    .unwrap()
}

#[tokio::test]
#[ignore = "需 TEST_PG_URL 指向真实 fico 库"]
async fn load_gl_account_tree_via_service() {
    let Some(db_id) = setup().await else {
        eprintln!("跳过：未设置 TEST_PG_URL");
        return;
    };

    // 诊断（定位「找不到定义」时用；正常时确认扫描到 3 个版本）
    if let Ok(items) = cmx_model_meta::definitions::store::list_definitions(
        Some("DCT"),
        Some("fi"),
        Some("cmxfico"),
        Some("gl"),
    )
    .await
    {
        eprintln!("扫描到 {} 个 DCT 定义文件（fi/cmxfico/gl）", items.len());
    }

    // 服务侧实现 HierService；协调器经它装载。坐标对齐 dev-local.toml（fi/cmxfico/gl）。
    let svc = DctHierService::new("fi", "cmxfico", "gl", &db_id);
    let mut ms = CmxMasterSlave::new(gl_account_schema()).unwrap();

    ms.load_via(&svc, &LoadQuery::default())
        .await
        .expect("经 DctHierService 装载 cf_gl_account 失败");

    let tree = ms.tree();
    let total = tree.nodes().len();
    let roots = tree.roots().len();
    eprintln!("装载完成：{total} 节点，{roots} 根");

    // 真机断言（基于当前 fico.cf_gl_account：29 行，5 个顶层类，24 行带 parent_id）
    assert_eq!(total, 29, "cf_gl_account 应装载 29 个节点");
    assert_eq!(roots, 5, "应有 5 个顶层科目类（资产/负债/权益/收入/成本费用）");

    // 层级正确性：根节点的 code 应是 1..5，且各自有子节点（is_leaf=0）
    let mut root_codes: Vec<String> = tree
        .roots()
        .iter()
        .map(|&r| {
            tree.node(r)
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    root_codes.sort();
    assert_eq!(root_codes, vec!["1", "2", "3", "4", "5"]);

    // 至少一个根有子（树确实连起来了，不是全平铺为根）
    let has_children = tree
        .roots()
        .iter()
        .any(|&r| !tree.node(r).children.is_empty());
    assert!(has_children, "顶层科目类应有子科目（树已装配）");

    // 全树节点数 = 根 + 所有后代（无孤儿丢失）：collect 各层累加应等于 total
    let reachable = count_reachable(tree);
    assert_eq!(reachable, total, "所有节点都应从根可达（无孤儿）");
}

/// 从根 BFS 数可达节点，验证树连通无孤儿。
fn count_reachable(tree: &cmx_master_slave::MsTree) -> usize {
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<_> = tree.roots().to_vec();
    while let Some(n) = stack.pop() {
        if seen.insert(n) {
            for &c in &tree.node(n).children {
                stack.push(c);
            }
        }
    }
    seen.len()
}

/// currency 字典的中立 schema（形状 B 但 selfHierarchy=false 退化为平表；pk=code，不铸号）。
fn currency_schema() -> HierSchema {
    HierSchema::from_json(&json!({
        "shape": { "kind": "self_ref", "parent_field": "parent_code" },
        "layers": [{
            "path": "dict",
            "table": "currency",   // dictCode
            "pk": "code"
        }]
    }))
    .unwrap()
}

/// 写路径真机验证：经 CmxMasterSlave.save_via → DctHierService.save → write::save 插入一条
/// 测试币种 ZZZ，回读确认落库，再清理。证明「协调器驱动服务落库」端到端成立（自清理，不污染）。
#[tokio::test]
#[ignore = "需 TEST_PG_URL 指向真实 fico 库"]
async fn save_currency_roundtrip_via_service() {
    let Some(db_id) = setup().await else {
        eprintln!("跳过：未设置 TEST_PG_URL");
        return;
    };
    let mm = get_default_pg_db_manager();

    // 前置清理（防上次残留）
    let _ = mm
        .execute_sql(&db_id, None, "DELETE FROM cf_currency WHERE code = 'ZZZ'")
        .await;

    let svc = DctHierService::new("fi", "cmxfico", "gl", &db_id);
    let ms = CmxMasterSlave::new(currency_schema()).unwrap();

    // 变更集：插入一条测试币种（fields 包裹，对齐前端 ChangeSetCollector 结构）
    let cs = cmx_master_slave::ChangeSet::from_json(&json!({
        "dict": { "inserted": [
            { "id": "ZZZ", "fields": { "code": "ZZZ", "name": "测试币种(集成验证)" } }
        ]}
    }))
    .unwrap();

    let out = ms
        .save_via(&svc, cs)
        .await
        .expect("经 DctHierService 保存 ZZZ 失败");
    eprintln!("保存回执：affected={}", out.affected);
    assert!(out.affected >= 1, "应至少影响 1 行");

    // 真机回读：ZZZ 确实落库
    let ds = mm
        .query_sql(&db_id, None, "SELECT code, name FROM cf_currency WHERE code = 'ZZZ'", "chk")
        .await
        .expect("回读 ZZZ 失败");
    let found = ds.row_count();
    eprintln!("回读命中 {found} 行");

    // 清理（无论断言结果都执行）
    let _ = mm
        .execute_sql(&db_id, None, "DELETE FROM cf_currency WHERE code = 'ZZZ'")
        .await;

    assert_eq!(found, 1, "ZZZ 应已落库（经协调器→服务→write::save）");
}
