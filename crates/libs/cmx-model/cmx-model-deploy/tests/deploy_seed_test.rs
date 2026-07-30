//! Task 4 集成测试：deploy_seed_menu 模块。
//!
//! 当前仅覆盖 `infer_conflict_columns`（4 个用例）。
//! `compile_all_definitions_for_module` 涉及文件系统 + list_definitions，
//! 留待 Task 6/7 通过更完整的集成测试覆盖。

use cmx_core::model::cell::{ColumnDefine, FieldType};
use cmx_core::model::meta::table::{IndexDefine, IndexKind, TableDefine};
use cmx_model_deploy::deploy_seed_menu::infer_conflict_columns;

/// 构造最小可用的 ColumnDefine。
fn mk_col(name: &str) -> ColumnDefine {
    ColumnDefine {
        name: name.to_string(),
        label: name.to_string(),
        field_type: FieldType::String,
        is_primary_key: false,
        is_nullable: false,
        default_value: None,
        i18n: false,
        length: Some(50),
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

/// 构造最小可用的 IndexDefine。
fn mk_index(kind: IndexKind, columns: Vec<&str>) -> IndexDefine {
    IndexDefine {
        name: format!("idx_{}", columns.join("_")),
        columns: columns.iter().map(|s| s.to_string()).collect(),
        kind,
    }
}

/// 构造 TableDefine：默认带一个 code 列（cmxfico 兜底用例依赖它存在）。
fn mk_table(name: &str, indexes: Vec<IndexDefine>, pks: Vec<String>) -> TableDefine {
    TableDefine {
        table_name: name.to_string(),
        display_name: name.to_string(),
        columns: vec![mk_col("code")],
        primary_keys: pks,
        indexes,
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

#[test]
fn test_infer_conflict_prefers_single_col_unique_index() {
    let def = mk_table(
        "t1",
        vec![mk_index(IndexKind::Unique, vec!["code"])],
        vec![],
    );
    assert_eq!(infer_conflict_columns(&def), vec!["code".to_string()]);
}

#[test]
fn test_infer_conflict_falls_back_to_composite_unique() {
    let def = mk_table(
        "t1",
        vec![mk_index(
            IndexKind::Unique,
            vec!["client", "coa"],
        )],
        vec![],
    );
    assert_eq!(
        infer_conflict_columns(&def),
        vec!["client".to_string(), "coa".to_string()]
    );
}

#[test]
fn test_infer_conflict_falls_back_to_primary_key() {
    let def = mk_table("t1", vec![], vec!["id".to_string()]);
    assert_eq!(infer_conflict_columns(&def), vec!["id".to_string()]);
}

#[test]
fn test_infer_conflict_falls_back_to_code_default() {
    let def = mk_table("t1", vec![], vec![]);
    assert_eq!(infer_conflict_columns(&def), vec!["code".to_string()]);
}
