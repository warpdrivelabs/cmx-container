//! 模型中心 · 数据库初始化与模块部署（真实落库）。
//!
//! 职责：
//! - `db_state`：读目标库台账（cmx_model_meta / cmx_model_module），组合出每模块每 kind 的 scenario。
//! - `init_db`：在目标库建 5 张台账系统表 + 写 cmx_model_meta + 历史（真实建表）。
//! - `deploy`：把选中的 DCT/DOC 定义编译成 TableDefine，用 PgTableDefineExecutor 建到目标库，
//!   写对象台账 + cmx_model_module + cmx_model_source（源 JSON 留档）+ 历史。
//!
//! 关键约束（见 docs/模型中心-…设计.md）：
//! - 建表现状对比只用数据库内省（PgTableDefineExecutor 内部走 information_schema，不读台账）。
//! - DDL 用 txn_id=None（PG DDL 自动提交）；台账 DML 在事务内；失败经 deploy_history 状态可对账。
//! - additive-only：create_or_upgrade_table 只加列/加索引，不 DROP。
//!
//! # 模块结构
//!
//! | 子模块 | 职责 |
//! |---|---|
//! | `compile` | DCT/DOC/RPT 定义 JSON → TableDefine 编译器 |
//! | `ledger` | 台账系统表 DDL、schema 检查、DB 读取辅助 |
//! | `diff_report` | 表差异报告（基于 DdlDiff 引擎） |
//! | `db_state` | db_state API + 模块发现 + cell 计算 + 记录组装 |
//! | `init` | 数据库初始化流程 |
//! | `deploy` | 部署流程 |
//! | `seed_scanner` | SEED/MENU 文件扫描 |
//! | `menu_pages_adapter` | 菜单页面适配 |
//! | `deploy_seed_menu` | SEED/MENU 部署编排 |

use cmx_core::model::cell::DataValue;
use cmx_api_types::Error;

// ── 子模块声明 ──────────────────────────────────────────────────────────
pub mod seed_scanner;
pub mod menu_pages_adapter;
pub mod deploy_seed_menu;

mod compile;
mod ledger;
mod diff_report;
mod db_state;
mod init;
mod deploy;

// ── 公共 API 再导出（cmx-api 通过 `cmx_model_deploy::xxx` 调用）────────
pub use db_state::db_state;
pub use deploy::{deploy, deploy_plan_stream, deploy_stream};
pub use init::{init_db, init_db_stream, init_plan_stream, InitEvent};

// ── 共享常量（pub(crate)：子模块通过 `crate::XXX` 引用）────────────────
pub(crate) const META_VERSION: i32 = 2;
pub(crate) const ENGINE_VERSION: &str = "1.0.0";
/// VARCHAR 字段未指定 fieldLength 时的默认长度。
/// 避免无长度 VARCHAR 被当成 TEXT 建表，导致与设计期望不一致时无法 ALTER 修正。
pub(crate) const VARCHAR_DEFAULT_LENGTH: u32 = 255;
/// 台账系统表清单（初始化时建入目标库）。
pub(crate) const LEDGER_TABLES: &[&str] = &[
    "cmx_model_meta",
    "cmx_model_module",
    "cmx_model_module_kind",
    "cmx_model_deploy_history",
    "cmx_model_source",
];

// ── 共享工具函数（pub(crate)：子模块通过 `crate::xxx` 引用）────────────
/// 构造统一错误：`"{ctx}: {e}"` 形式，返回闭包给 `.map_err()` 用。
pub(crate) fn db_err<E: std::fmt::Display>(ctx: &str) -> impl Fn(E) -> Error + '_ {
    move |e| Error::InternalError(format!("{ctx}: {e}"))
}
/// DataValue → 字符串。各种数值/时间/布尔形态都尝试转成可读字符串；
/// 不可转换（数组/二进制等）返回 `None`，由调用方决定走空字符串还是默认值。
pub(crate) fn data_value_string(v: &DataValue) -> Option<String> {
    match v {
        // 字符串家族（长短都直接 clone / to_string）
        DataValue::String(s) => Some(s.clone()),
        DataValue::ShortStr(s) => Some(s.to_string()),
        DataValue::LongStr(s) => Some(s.to_string()),
        // 数值家族（统一用其 Display 实现）
        DataValue::Int(i) => Some(i.to_string()),
        DataValue::Float(f) => Some(f.to_string()),
        DataValue::Decimal(d) => Some(d.to_string()),
        // 时间家族：DateTime 走 RFC3339（带时区，跨时区可读），Date 走 Display（YYYY-MM-DD）
        DataValue::DateTime(dt) => Some(dt.to_rfc3339()),
        DataValue::Date(d) => Some(d.to_string()),
        // 布尔："true"/"false"
        DataValue::Bool(b) => Some(b.to_string()),
        // 数组/二进制/null 等不可读形态 → None（调用方按需处理）
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
//  单元测试：编译器对真实 DCT JSON 的正确性（不依赖数据库）
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use crate::compile::{compile_dct, compile_doc, compile_rpt, map_field_type};
    use crate::diff_report::diff_table_to_report;
    use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, IndexKind, TableDefine};
    use cmx_metadata::{DdlDiff, PostgresDdlDialect};
    use serde_json::Value;

    fn load(p: &str) -> Value {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../data/meta/definitions");
        serde_json::from_str(&std::fs::read_to_string(root.join(p)).unwrap()).unwrap()
    }

    #[test]
    fn field_type_mapping() {
        assert!(matches!(map_field_type("VARCHAR"), FieldType::String));
        assert!(matches!(map_field_type("varchar"), FieldType::String));
        assert!(matches!(map_field_type("BIGINT"), FieldType::Int));
        assert!(matches!(map_field_type("TINYINT"), FieldType::Int));
        assert!(matches!(map_field_type("DECIMAL"), FieldType::Decimal));
        assert!(matches!(map_field_type("DATE"), FieldType::Date));
    }

    #[test]
    fn compile_real_dct_cf_client() {
        let doc = load("fi/cmxfico/gl/cmxfico_dct_meta_v1.json");
        let base = load("base/base_dct_meta_v1.json");
        let defs = compile_dct(&doc, &base);
        assert!(!defs.is_empty(), "应编译出多张字典表");

        // 找 cf_client（第一张：3 本表字段 + 4 base + 4 audit + 1 system）
        let t = defs
            .iter()
            .find(|d| d.table_name == "cf_client")
            .expect("应有 cf_client");
        let cols: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        // 本表字段
        for c in ["logical_system", "system_id", "client_role"] {
            assert!(cols.contains(&c), "缺列 {c}: {:?}", cols);
        }
        // base 字段集展开
        for c in ["code", "name", "sort_no", "status"] {
            assert!(cols.contains(&c), "缺 base 列 {c}: {:?}", cols);
        }
        // audit + system 字段集
        assert!(
            cols.contains(&"create_by") && cols.contains(&"is_system"),
            "缺 audit/system 列: {:?}",
            cols
        );
        // 唯一键 uniqueKeys=[["code"]]
        assert!(
            t.indexes
                .iter()
                .any(|i| i.columns == vec!["code".to_string()]
                    && matches!(i.kind, IndexKind::Unique)),
            "应有 code 唯一索引"
        );
        // 无重复列
        let mut seen = std::collections::HashSet::new();
        for c in &t.columns {
            assert!(seen.insert(c.name.clone()), "列 {} 重复", c.name);
        }
    }

    #[test]
    fn compile_all_tables_have_name_and_columns() {
        let doc = load("fi/cmxfico/gl/cmxfico_dct_meta_v1.json");
        let base = load("base/base_dct_meta_v1.json");
        let defs = compile_dct(&doc, &base);
        for d in &defs {
            assert!(!d.table_name.is_empty(), "表名不能为空");
            assert!(!d.columns.is_empty(), "表 {} 应有列", d.table_name);
        }
        // DOC 也能编译
        let doc2 = load("fi/cmxfico/gl/cmxfico_doc_meta_v1.json");
        let base2 = load("base/base_doc_meta_v1.json");
        let vdefs = compile_doc(&doc2, &base2);
        assert!(!vdefs.is_empty(), "DOC 应编译出单据表");
        for d in &vdefs {
            assert!(!d.columns.is_empty(), "DOC 表 {} 应有列", d.table_name);
        }
    }

    #[test]
    fn compile_doc_includes_summary_tables() {
        let doc = load("fi/cmxfico/gl/cmxfico_doc_meta_v1.json");
        let base = load("base/base_doc_meta_v1.json");
        let defs = compile_doc(&doc, &base);
        let main_count = doc
            .get("voucherTables")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(
            defs.len() > main_count,
            "DOC 编译应包含 voucherTables 下的 summaries 汇总表"
        );
        let sum = defs
            .iter()
            .find(|d| d.table_name == "cv_aux_line_sum")
            .expect("应编译出凭证辅助核算汇总表 cv_aux_line_sum");
        let cols: Vec<&str> = sum.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(cols.contains(&"id"), "汇总表应包含主键 id");
        assert!(cols.contains(&"upper_id"), "汇总表应包含上级主键 upper_id");
        assert!(
            sum.primary_keys == vec!["id".to_string()],
            "汇总表 id 应作为主键"
        );
    }

    // ── diff_table_to_report：复用 DdlDiff 引擎后的假阳性修复 ──────────────

    /// 构造最小可用的 ColumnDefine（仅 name/field_type/nullable，其余默认）。
    fn mk_col(name: &str, ft: FieldType, nullable: bool) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: name.to_string(),
            field_type: ft,
            is_primary_key: false,
            is_nullable: nullable,
            default_value: None,
            i18n: false,
            length: None,
            precision: None,
            scale: None,
            db_type: None,
            ordinal: None,
            create_time: None,
            update_time: None,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: std::collections::HashMap::new(),
        }
    }

    /// 构造最小可用的 TableDefine。
    fn mk_table(name: &str, cols: Vec<ColumnDefine>) -> TableDefine {
        TableDefine {
            table_name: name.to_string(),
            display_name: name.to_string(),
            columns: cols,
            primary_keys: vec![],
            indexes: vec![],
            version: 1,
            create_time: None,
            update_time: None,
            i18n: false,
            comment: None,
            schema: None,
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: std::collections::HashMap::new(),
        }
    }

    /// 核心场景：PG 内省还原的 bigint 列（带派生 precision=64/scale=0、db_type=BIGINT）
    /// 与设计期 Int 列（precision/scale=None）对比，不应再误报「升级表/修改列」。
    /// 这是用户报告的 `cf_client` sort_no/status/create_by 等字段的典型情况。
    #[test]
    fn diff_table_report_no_false_positive_for_bigint() {
        // 模拟 query_current_table_define 还原出的 bigint 列
        let mut db_sort_no = mk_col("sort_no", FieldType::Int, true);
        db_sort_no.precision = Some(64);
        db_sort_no.scale = Some(0);
        db_sort_no.db_type = Some("BIGINT".to_string());
        let mut db_create_time = mk_col("create_time", FieldType::DateTime, true);
        db_create_time.db_type = Some("TIMESTAMP WITH TIME ZONE".to_string());

        let current = mk_table("cf_client", vec![db_sort_no, db_create_time]);
        // 设计期定义：同类型，但 precision/scale/db_type 未设置
        let desired = mk_table(
            "cf_client",
            vec![
                mk_col("sort_no", FieldType::Int, true),
                mk_col("create_time", FieldType::DateTime, true),
            ],
        );

        let report = diff_table_to_report(&current, &desired);
        assert_eq!(
            report["action"].as_str(),
            Some("no_change"),
            "bigint/timestamptz 与设计期 Int/DateTime 不应报变更: {report}"
        );
        assert!(
            report["modifiedColumns"].as_array().unwrap().is_empty(),
            "modifiedColumns 应为空: {report}"
        );
        assert_eq!(
            report["unchangedColumns"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            2,
            "两列都应归入 unchangedColumns: {report}"
        );
    }

    /// 真实差异：设计期新增一列，应报 upgrade_table + addedColumns。
    #[test]
    fn diff_table_report_detects_added_column() {
        let current = mk_table("t", vec![mk_col("id", FieldType::Int, false)]);
        let desired = mk_table(
            "t",
            vec![
                mk_col("id", FieldType::Int, false),
                mk_col("name", FieldType::String, true),
            ],
        );
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        let added = report["addedColumns"].as_array().unwrap();
        assert_eq!(added.len(), 1, "应报 1 个新增列: {report}");
        assert_eq!(added[0]["name"].as_str(), Some("name"));
        // dataType 应使用建表基准（String → VARCHAR/TEXT），不再是旧的 "character varying"
        assert!(
            added[0]["dataType"]
                .as_str()
                .map(|s| s != "character varying" && !s.is_empty())
                .unwrap_or(false),
            "新增列 dataType 应为建表基准类型而非 information_schema 小写名: {report}"
        );
    }

    /// 真实差异：列类型 Int → Decimal，应报 upgrade_table + modifiedColumns。
    #[test]
    fn diff_table_report_detects_type_change() {
        let current = mk_table("t", vec![mk_col("amount", FieldType::Int, true)]);
        let desired = mk_table("t", vec![mk_col("amount", FieldType::Decimal, true)]);
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        let modified = report["modifiedColumns"].as_array().unwrap();
        assert_eq!(modified.len(), 1, "应报 1 个修改列: {report}");
        let changes = modified[0]["changes"].as_array().unwrap();
        assert!(
            changes
                .iter()
                .any(|c| c["field"].as_str() == Some("dataType")),
            "修改明细应含 dataType 变更: {report}"
        );
    }

    /// nullable 变更应被检出（如 NOT NULL → NULL）。
    #[test]
    fn diff_table_report_detects_nullable_change() {
        let current = mk_table("t", vec![mk_col("code", FieldType::String, false)]);
        let desired = mk_table("t", vec![mk_col("code", FieldType::String, true)]);
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        assert!(
            !report["modifiedColumns"].as_array().unwrap().is_empty(),
            "nullable 变更应报修改列: {report}"
        );
    }

    /// cf_fs_version 场景：列全部一致，但设计期有表注释（remark）、DB 无表注释。
    /// 应报 upgrade_table，且 commentChange 透出 from=∅→to=注释，让用户看到「为什么要升级」。
    #[test]
    fn diff_table_report_shows_comment_change() {
        let current = mk_table(
            "cf_fs_version",
            vec![mk_col("code", FieldType::String, false)],
        );
        let mut desired = mk_table(
            "cf_fs_version",
            vec![mk_col("code", FieldType::String, false)],
        );
        desired.comment = Some("报表上滚结构".to_string());
        let report = diff_table_to_report(&current, &desired);
        // 列一致但因注释差异仍报升级表（保留同步语义）
        assert_eq!(
            report["action"].as_str(),
            Some("upgrade_table"),
            "表注释差异应触发 upgrade_table: {report}"
        );
        // 列级无变更
        assert!(
            report["modifiedColumns"].as_array().unwrap().is_empty(),
            "列应无变更: {report}"
        );
        // 但 commentChange 必须透出，让用户看到原因
        let cmt = &report["commentChange"];
        assert!(!cmt.is_null(), "commentChange 不应为 null: {report}");
        assert_eq!(cmt["from"].as_str(), Some(""), "from 应为空(DB无注释)");
        assert_eq!(cmt["to"].as_str(), Some("报表上滚结构"));
    }

    /// 索引名错配场景：设计期 uk_t_1(Unique,code) vs DB cf_t_code_key(Unique,code)。
    /// 列+类型相同 → addedIndexes/droppedIndexes 都为空，且列也一致 → action 应为 no_change。
    #[test]
    fn diff_table_report_no_false_positive_for_index_name() {
        let mut current = mk_table("cf_t", vec![mk_col("code", FieldType::String, false)]);
        let mut desired = mk_table("cf_t", vec![mk_col("code", FieldType::String, false)]);
        current.indexes = vec![IndexDefine {
            name: "cf_t_code_key".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
        }];
        desired.indexes = vec![IndexDefine {
            name: "uk_cf_t_1".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
        }];
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(
            report["action"].as_str(),
            Some("no_change"),
            "索引名不同但列+类型相同，应判无变化: {report}"
        );
        assert!(
            report["addedIndexes"].as_array().unwrap().is_empty(),
            "addedIndexes 应为空: {report}"
        );
        assert!(
            report["droppedIndexes"].as_array().unwrap().is_empty(),
            "droppedIndexes 应为空: {report}"
        );
    }

    /// 列注释透出：DB 列无注释（label=""）、设计期有 caption，结构相同。
    /// 报告应透出 modifiedColumnComments 且 action=upgrade_table，让用户看到「列注释要同步」。
    #[test]
    fn diff_table_report_shows_column_comment_change() {
        // current：DB 还原，列 label 为空（DB 缺 COMMENT ON COLUMN）
        let mut cur_col = mk_col("id", FieldType::Int, false);
        cur_col.label = String::new();
        let current = mk_table("cf_t", vec![cur_col]);
        // desired：设计期 label="字典项主键"
        let mut des_col = mk_col("id", FieldType::Int, false);
        des_col.label = "字典项主键".to_string();
        let desired = mk_table("cf_t", vec![des_col]);
        let report = diff_table_to_report(&current, &desired);
        // 列结构无变更
        assert!(
            report["modifiedColumns"].as_array().unwrap().is_empty(),
            "结构相同，modifiedColumns 应为空: {report}"
        );
        // 但应报 upgrade_table（因列注释差异）
        assert_eq!(
            report["action"].as_str(),
            Some("upgrade_table"),
            "列注释差异应触发 upgrade_table: {report}"
        );
        // modifiedColumnComments 透出 from→to
        let cmts = report["modifiedColumnComments"].as_array().unwrap();
        assert_eq!(cmts.len(), 1, "应有 1 条列注释变更: {report}");
        assert_eq!(cmts[0]["name"].as_str(), Some("id"));
        assert_eq!(cmts[0]["from"].as_str(), Some(""));
        assert_eq!(cmts[0]["to"].as_str(), Some("字典项主键"));
    }

    /// cf_company 真实场景：DB 把无长度 VARCHAR 建成了 TEXT（旧行为），
    /// 设计期期望 varchar(255)（新默认值）。应报修改列 dataType TEXT→VARCHAR(255)，
    /// 且执行路径生成 ALTER COLUMN TYPE。
    #[test]
    fn varchar_default_length_triggers_alter_from_text() {
        let doc = load("fi/cmxfico/gl/cmxfico_dct_meta_v3.json");
        let base = load("base/base_dct_meta_v1.json");
        let defs = compile_dct(&doc, &base);
        let desired = defs
            .iter()
            .find(|d| d.table_name == "cf_company")
            .expect("应编译出 cf_company")
            .clone();

        // 1) 设计期 country_code 应默认 varchar(255)
        let cc = desired
            .columns
            .iter()
            .find(|c| c.name == "country_code")
            .expect("应有 country_code 列");
        assert_eq!(
            cc.length,
            Some(255),
            "无 fieldLength 的 VARCHAR 应默认 255: country_code length={:?}",
            cc.length
        );

        // 2) 模拟 DB 还原：country_code 是 TEXT（旧行为建出来的）
        let mut current = desired.clone();
        for c in &mut current.columns {
            if c.name == "country_code" {
                c.length = None; // TEXT 无长度
                c.db_type = Some("TEXT".to_string());
            }
        }
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(
            report["action"].as_str(),
            Some("upgrade_table"),
            "TEXT vs varchar(255) 应报升级: {report}"
        );
        let modified = report["modifiedColumns"].as_array().unwrap();
        let cc_mod = modified
            .iter()
            .find(|m| m["name"].as_str() == Some("country_code"))
            .expect("应报 country_code 修改");
        let changes = cc_mod["changes"].as_array().unwrap();
        assert!(
            changes
                .iter()
                .any(|d| d["field"].as_str() == Some("dataType")
                    && d["from"].as_str() == Some("TEXT")
                    && d["to"].as_str() == Some("VARCHAR(255)")),
            "应报 dataType TEXT→VARCHAR(255): {changes:?}"
        );

        // 3) 执行路径应生成 ALTER COLUMN TYPE VARCHAR(255)
        let dialect = PostgresDdlDialect::default();
        let stmts =
            DdlDiff::diff_to_ddl(&dialect, &[current], std::slice::from_ref(&desired)).unwrap();
        assert!(
            stmts
                .iter()
                .any(|s| s.contains("ALTER COLUMN \"country_code\" TYPE VARCHAR(255)")),
            "应生成 ALTER COLUMN TYPE VARCHAR(255): {stmts:?}"
        );
    }

    #[test]
    fn compile_rpt_builds_three_storage_tables() {
        // RPT 落地三表来自 base_rpt_meta.storageTables，报表模板本身不产生 DDL。
        let doc = load("fi/cmxfico/gl/cmxfico_bs_rpt_meta_v1.json");
        let base = load("base/base_rpt_meta_v1.json");
        let defs = compile_rpt(&doc, &base);
        let names: Vec<&str> = defs.iter().map(|d| d.table_name.as_str()).collect();
        for t in ["cr_report_instance", "cr_cell_value", "cr_report_snapshot"] {
            assert!(names.contains(&t), "应编译出报表落地表 {t}，实得 {names:?}");
        }

        // 实例头：本表字段 + 审计字段集展开，id 主键，唯一键四元组。
        let inst = defs
            .iter()
            .find(|d| d.table_name == "cr_report_instance")
            .expect("应有 cr_report_instance");
        let cols: Vec<&str> = inst.columns.iter().map(|c| c.name.as_str()).collect();
        for c in ["id", "tpl_code", "org_id", "period_code", "scope", "status"] {
            assert!(cols.contains(&c), "实例头缺列 {c}: {cols:?}");
        }
        // 审计字段集（reportAuditFields）展开
        assert!(
            cols.contains(&"create_by") && cols.contains(&"delete_flag"),
            "实例头应展开审计字段集: {cols:?}"
        );
        assert_eq!(
            inst.primary_keys,
            vec!["id".to_string()],
            "实例头 id 应为主键"
        );
        assert!(
            inst.indexes.iter().any(|i| i.columns
                == vec![
                    "tpl_code".to_string(),
                    "org_id".to_string(),
                    "period_code".to_string(),
                    "scope".to_string()
                ]
                && matches!(i.kind, IndexKind::Unique)),
            "实例头应有 (tpl_code,org_id,period_code,scope) 唯一索引"
        );

        // 单元格值：num_value 为 Decimal 精度列，drill_key 为 JSON。
        let cell = defs
            .iter()
            .find(|d| d.table_name == "cr_cell_value")
            .expect("应有 cr_cell_value");
        let num = cell
            .columns
            .iter()
            .find(|c| c.name == "num_value")
            .expect("单元格值应有 num_value 列");
        assert!(
            matches!(num.field_type, FieldType::Decimal),
            "num_value 应为 Decimal"
        );

        // 各表无重复列。
        for d in &defs {
            let mut seen = std::collections::HashSet::new();
            for c in &d.columns {
                assert!(
                    seen.insert(c.name.clone()),
                    "表 {} 列 {} 重复",
                    d.table_name,
                    c.name
                );
            }
        }
    }
}