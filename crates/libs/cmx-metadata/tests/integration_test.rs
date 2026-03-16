//! cmx-metadata 集成测试
//!
//! 基于 tests/ 目录下的真实元数据 JSON 文件，测试完整的工作流：
//! JSON 加载 → DDL 生成 → DDL 解析 → 增量 DDL diff → i18n → 配置管理

use std::path::Path;

use cmx_core::model::cell::FieldType;
use cmx_metadata::config::{load_table_defines_config_from_path, TableDefinesConfigManager};
use cmx_metadata::ddl::{table_to_pg_ddl, table_to_pg_ddl_roundtrip, tables_to_pg_ddl, DdlDialect};
use cmx_metadata::ddl::diff::{ColumnChange, DdlDiff, TableChange};
use cmx_metadata::ddl::postgres::PostgresDdlDialect;
use cmx_metadata::i18n::derive_i18n_table_define;
use cmx_metadata::loader::{load_table_defines_from_path, table_defines_from_str};
use cmx_metadata::parser::{pg_ddl_to_table_define, pg_ddl_to_table_defines, DdlParser};
use cmx_metadata::parser::postgres::PostgresDdlParser;

/// 测试数据目录
fn tests_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").leak()
}

// ==========================================
// loader 集成测试
// ==========================================

#[test]
fn test_load_tables_from_file() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();
    assert_eq!(tables.len(), 3, "应加载 3 张表");
    assert_eq!(tables[0].table_name, "cmx_domain");
    assert_eq!(tables[1].table_name, "cmx_application");
    assert_eq!(tables[2].table_name, "cmx_module");
}

#[test]
fn test_loaded_table_columns() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // cmx_domain 应有 9 列
    let domain = &tables[0];
    assert_eq!(domain.columns.len(), 9);
    assert_eq!(domain.primary_keys, vec!["id"]);
    assert_eq!(
        domain.comment.as_deref(),
        Some("企业应用域，如财务域、供应链域、人力资源域等；为「域-应用-模块」三层结构的顶层")
    );

    // 验证列属性
    let id_col = domain.columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.is_primary_key);
    assert!(!id_col.is_nullable);
    assert_eq!(id_col.field_type, FieldType::Int);
    assert_eq!(id_col.db_type.as_deref(), Some("BIGINT"));

    let code_col = domain.columns.iter().find(|c| c.name == "code").unwrap();
    assert_eq!(code_col.field_type, FieldType::String);
    assert_eq!(code_col.length, Some(32));
    assert!(!code_col.i18n);

    let name_col = domain.columns.iter().find(|c| c.name == "name").unwrap();
    assert!(name_col.i18n, "name 列应标记为 i18n");
}

#[test]
fn test_loaded_table_indexes() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // cmx_domain 有 2 个索引
    let domain = &tables[0];
    assert_eq!(domain.indexes.len(), 2);
    assert_eq!(domain.indexes[0].name, "uk_cmx_domain_code");
    assert_eq!(domain.indexes[0].columns, vec!["code"]);

    // cmx_application 有 3 个索引
    let app = &tables[1];
    assert_eq!(app.indexes.len(), 3);
    let uk_idx = app
        .indexes
        .iter()
        .find(|i| i.name == "uk_cmx_application_domain_code")
        .unwrap();
    assert_eq!(uk_idx.columns, vec!["domain_id", "code"]);
}

#[test]
fn test_loaded_table_foreign_keys() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // cmx_application.domain_id 是外键
    let app = &tables[1];
    let fk_col = app.columns.iter().find(|c| c.name == "domain_id").unwrap();
    assert!(fk_col.is_foreign_key);
    assert_eq!(fk_col.foreign_key_table.as_deref(), Some("cmx_domain"));
    assert_eq!(fk_col.foreign_key_column.as_deref(), Some("id"));

    // cmx_module.application_id 是外键
    let module = &tables[2];
    let fk_col = module
        .columns
        .iter()
        .find(|c| c.name == "application_id")
        .unwrap();
    assert!(fk_col.is_foreign_key);
    assert_eq!(
        fk_col.foreign_key_table.as_deref(),
        Some("cmx_application")
    );
}

#[test]
fn test_table_defines_from_str_with_file_content() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let content = std::fs::read_to_string(&path).unwrap();
    let tables = table_defines_from_str(&content).unwrap();
    assert_eq!(tables.len(), 3);
}

// ==========================================
// config 集成测试
// ==========================================

#[test]
fn test_load_config_from_file() {
    let path = tests_dir().join("domain_app_module_config.json");
    let config = load_table_defines_config_from_path(&path).unwrap();
    assert_eq!(config.name, "domain_app_module");
    assert_eq!(config.files, vec!["domain_app_module_tables.json"]);
    assert!(config.depends_on.is_empty());
    assert_eq!(config.priority, Some(0));
    assert!(config.description.is_some());
}

#[test]
fn test_config_manager_load_all_tables() {
    let config_path = tests_dir().join("domain_app_module_config.json");
    let manager = TableDefinesConfigManager::from_config_paths(&[&config_path]).unwrap();

    assert_eq!(manager.config_count(), 1);
    assert!(manager.get_config_by_name("domain_app_module").is_some());

    let tables = manager.load_all_tables(tests_dir()).unwrap();
    assert_eq!(tables.len(), 3);
    assert_eq!(tables[0].table_name, "cmx_domain");
}

#[test]
fn test_config_manager_load_by_name() {
    let config_path = tests_dir().join("domain_app_module_config.json");
    let manager = TableDefinesConfigManager::from_config_paths(&[&config_path]).unwrap();

    let tables = manager
        .load_tables_by_config_name(tests_dir(), "domain_app_module")
        .unwrap();
    assert_eq!(tables.len(), 3);

    // 不存在的配置名应报错
    let err = manager.load_tables_by_config_name(tests_dir(), "nonexistent");
    assert!(err.is_err());
}

#[test]
fn test_config_manager_sorted_configs() {
    let config_path = tests_dir().join("domain_app_module_config.json");
    let manager = TableDefinesConfigManager::from_config_paths(&[&config_path]).unwrap();

    let sorted = manager.sorted_configs().unwrap();
    assert_eq!(sorted.len(), 1);
    assert_eq!(sorted[0].name, "domain_app_module");
}

// ==========================================
// DDL 生成集成测试
// ==========================================

#[test]
fn test_generate_ddl_for_loaded_tables() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let ddl = tables_to_pg_ddl(&tables).unwrap();

    // 每张表都应有 CREATE TABLE
    assert!(ddl.contains("CREATE TABLE"), "DDL 应包含 CREATE TABLE");
    assert!(ddl.contains("cmx_domain"), "DDL 应包含 cmx_domain");
    assert!(
        ddl.contains("cmx_application"),
        "DDL 应包含 cmx_application"
    );
    assert!(ddl.contains("cmx_module"), "DDL 应包含 cmx_module");

    // 索引
    assert!(ddl.contains("CREATE UNIQUE INDEX"), "DDL 应包含唯一索引");
    assert!(
        ddl.contains("uk_cmx_domain_code"),
        "DDL 应包含 uk_cmx_domain_code 索引"
    );

    // COMMENT
    assert!(ddl.contains("COMMENT ON TABLE"), "DDL 应包含表注释");
    assert!(ddl.contains("COMMENT ON COLUMN"), "DDL 应包含列注释");
}

#[test]
fn test_generate_single_table_ddl() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let ddl = table_to_pg_ddl(&tables[0]).unwrap();
    assert!(ddl.contains("CREATE TABLE"));
    assert!(ddl.contains("cmx_domain"));
    assert!(ddl.contains("\"id\""));
    assert!(ddl.contains("\"code\""));
    assert!(ddl.contains("PRIMARY KEY"));
}

#[test]
fn test_generate_ddl_roundtrip_mode() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // roundtrip 模式应优先使用 db_type
    let ddl = table_to_pg_ddl_roundtrip(&tables[0]).unwrap();
    assert!(
        ddl.contains("BIGINT"),
        "roundtrip 模式应使用 db_type BIGINT"
    );
    assert!(
        ddl.contains("VARCHAR(32)"),
        "roundtrip 模式应使用 db_type VARCHAR(32)"
    );
}

#[test]
fn test_generate_ddl_with_default_value() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let ddl = table_to_pg_ddl(&tables[0]).unwrap();
    // sort_order 有 default_value "0"
    assert!(ddl.contains("DEFAULT 0"), "DDL 应包含默认值");
}

#[test]
fn test_generate_ddl_with_schema() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // 所有表都指定了 schema: "public"
    let ddl = table_to_pg_ddl(&tables[0]).unwrap();
    assert!(
        ddl.contains("\"public\".\"cmx_domain\""),
        "DDL 应包含 schema 限定的表名"
    );
}

// ==========================================
// DDL 解析 + round-trip 集成测试
// ==========================================

#[test]
fn test_ddl_roundtrip_single_table() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();
    let original = &tables[0]; // cmx_domain

    // 生成 DDL → 解析回 TableDefine
    let ddl = table_to_pg_ddl_roundtrip(original).unwrap();
    let parsed = pg_ddl_to_table_define(&ddl).unwrap();

    assert_eq!(parsed.table_name, original.table_name);
    assert_eq!(parsed.columns.len(), original.columns.len());
    assert_eq!(parsed.primary_keys, original.primary_keys);
    assert_eq!(parsed.schema, original.schema);
}

#[test]
fn test_ddl_roundtrip_all_tables() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let ddl = tables_to_pg_ddl(&tables).unwrap();
    let parsed = pg_ddl_to_table_defines(&ddl).unwrap();

    assert_eq!(
        parsed.len(),
        tables.len(),
        "解析回的表数量应与原始一致"
    );
    for (orig, back) in tables.iter().zip(parsed.iter()) {
        assert_eq!(back.table_name, orig.table_name);
        assert_eq!(back.columns.len(), orig.columns.len());
    }
}

#[test]
fn test_ddl_roundtrip_preserves_column_types() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();
    let original = &tables[0];

    let dialect = PostgresDdlDialect {
        prefer_db_type: true,
    };
    let ddl = dialect.generate_full_ddl(original).unwrap();
    let parser = PostgresDdlParser;
    let parsed = parser.parse_create_table(&ddl).unwrap();

    // 检查解析后保留了 db_type
    let id_col = parsed.columns.iter().find(|c| c.name == "id").unwrap();
    assert_eq!(id_col.db_type.as_deref(), Some("BIGINT"));

    let code_col = parsed.columns.iter().find(|c| c.name == "code").unwrap();
    assert!(code_col.db_type.as_deref().unwrap().contains("VARCHAR"));
}

// ==========================================
// DDL diff 集成测试
// ==========================================

#[test]
fn test_diff_add_column_to_loaded_table() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let mut old = tables[0].clone(); // cmx_domain
    let new_table = old.clone();

    // 从旧表删除最后一列（updated_at），模拟"新表加了一列"
    old.columns.pop();

    let changes = DdlDiff::diff(&[old], &[new_table]);
    assert_eq!(changes.len(), 1);

    if let TableChange::AlterTable {
        column_changes, ..
    } = &changes[0]
    {
        let added: Vec<_> = column_changes
            .iter()
            .filter_map(|c| match c {
                ColumnChange::AddColumn(col) => Some(col.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(added.contains(&"updated_at"), "应检测到新增 updated_at 列");
    } else {
        panic!("应为 AlterTable 变更");
    }

    let dialect = PostgresDdlDialect::default();
    let stmts = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
    assert!(!stmts.is_empty());
    let joined = stmts.join("\n");
    assert!(
        joined.contains("ALTER TABLE"),
        "增量 DDL 应包含 ALTER TABLE"
    );
    assert!(
        joined.contains("ADD COLUMN"),
        "增量 DDL 应包含 ADD COLUMN"
    );
}

#[test]
fn test_diff_drop_column() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let old = tables[0].clone();
    let mut new_table = old.clone();
    // 从新表删除 tags 列
    new_table.columns.retain(|c| c.name != "tags");

    let changes = DdlDiff::diff(&[old], &[new_table]);
    assert_eq!(changes.len(), 1);

    if let TableChange::AlterTable {
        column_changes, ..
    } = &changes[0]
    {
        let dropped: Vec<_> = column_changes
            .iter()
            .filter_map(|c| match c {
                ColumnChange::DropColumn(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(dropped.contains(&"tags"));
    } else {
        panic!("应为 AlterTable 变更");
    }

    let dialect = PostgresDdlDialect::default();
    let stmts = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
    let joined = stmts.join("\n");
    assert!(joined.contains("DROP COLUMN"));
}

#[test]
fn test_diff_no_changes() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let changes = DdlDiff::diff(&tables, &tables);
    assert!(changes.is_empty(), "相同表集合不应有变更");
}

// ==========================================
// i18n 集成测试
// ==========================================

#[test]
fn test_i18n_not_generated_for_non_i18n_table() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // 测试数据中所有表 i18n = false
    for table in &tables {
        assert!(!table.i18n);
        let result = derive_i18n_table_define(table);
        assert!(
            result.is_none(),
            "非 i18n 表不应生成伴生表: {}",
            table.table_name
        );
    }
}

#[test]
fn test_i18n_generated_when_flag_set() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    // 手动设置 i18n = true
    let mut domain = tables[0].clone();
    domain.i18n = true;

    let i18n = derive_i18n_table_define(&domain).unwrap();
    assert_eq!(i18n.table_name, "cmx_domain_i18n");
    assert_eq!(i18n.display_name, "域（多语言）");

    // 应包含 ref_id + locale + i18n 标记的列（name, description）
    let col_names: Vec<&str> = i18n.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(col_names.contains(&"ref_id"));
    assert!(col_names.contains(&"locale"));
    assert!(col_names.contains(&"name"), "name 列标记了 i18n");
    assert!(
        col_names.contains(&"description"),
        "description 列标记了 i18n"
    );

    assert_eq!(i18n.primary_keys, vec!["ref_id", "locale"]);
    assert_eq!(i18n.schema, domain.schema);
}

#[test]
fn test_i18n_table_ddl_generation() {
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let mut domain = tables[0].clone();
    domain.i18n = true;

    let i18n = derive_i18n_table_define(&domain).unwrap();
    let ddl = table_to_pg_ddl(&i18n).unwrap();

    assert!(ddl.contains("cmx_domain_i18n"));
    assert!(ddl.contains("ref_id"));
    assert!(ddl.contains("locale"));
    assert!(ddl.contains("PRIMARY KEY"));
}

// ==========================================
// 端到端集成测试：config → loader → DDL → parser → diff
// ==========================================

#[test]
fn test_end_to_end_config_to_ddl_to_parse() {
    // 1. 通过配置管理器加载表定义
    let config_path = tests_dir().join("domain_app_module_config.json");
    let manager = TableDefinesConfigManager::from_config_paths(&[&config_path]).unwrap();
    let tables = manager.load_all_tables(tests_dir()).unwrap();
    assert_eq!(tables.len(), 3);

    // 2. 为每张表生成 DDL
    let dialect = PostgresDdlDialect {
        prefer_db_type: true,
    };
    let full_ddl = dialect.generate_full_ddl_for_tables(&tables).unwrap();
    assert!(!full_ddl.is_empty());

    // 3. 解析回 TableDefine
    let parser = PostgresDdlParser;
    let parsed = parser.parse_ddl(&full_ddl).unwrap();
    assert_eq!(parsed.len(), 3);

    // 4. 与原始表对比 — 列数应一致
    for (orig, back) in tables.iter().zip(parsed.iter()) {
        assert_eq!(back.table_name, orig.table_name);
        assert_eq!(
            back.columns.len(),
            orig.columns.len(),
            "表 {} 的列数不一致",
            orig.table_name
        );
    }

    // 5. diff 应无变更（相同表集合）
    let changes = DdlDiff::diff(&tables, &tables);
    assert!(changes.is_empty(), "相同表集合 diff 应为空");
}

#[test]
fn test_end_to_end_schema_evolution() {
    // 模拟 schema 演进：v1 → v2（新增列、删除列）
    let path = tests_dir().join("domain_app_module_tables.json");
    let tables = load_table_defines_from_path(&path).unwrap();

    let v1 = tables[0].clone(); // cmx_domain
    let mut v2 = v1.clone();

    // v2: 删除 tags 列，新增 status 列
    v2.columns.retain(|c| c.name != "tags");
    v2.columns.push(cmx_core::model::cell::ColumnDefine {
        name: "status".to_string(),
        label: "状态".to_string(),
        field_type: FieldType::String,
        is_primary_key: false,
        is_nullable: true,
        default_value: Some("active".to_string()),
        i18n: false,
        length: Some(16),
        precision: None,
        scale: None,
        db_type: Some("VARCHAR(16)".to_string()),
        ordinal: Some(10),
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    });

    // 生成增量 DDL
    let changes = DdlDiff::diff(&[v1], &[v2]);
    assert_eq!(changes.len(), 1);

    if let TableChange::AlterTable {
        column_changes, ..
    } = &changes[0]
    {
        let dropped: Vec<_> = column_changes
            .iter()
            .filter_map(|c| match c {
                ColumnChange::DropColumn(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        let added: Vec<_> = column_changes
            .iter()
            .filter_map(|c| match c {
                ColumnChange::AddColumn(col) => Some(col.name.as_str()),
                _ => None,
            })
            .collect();

        assert!(dropped.contains(&"tags"), "应包含删除 tags 列");
        assert!(added.contains(&"status"), "应包含新增 status 列");
    } else {
        panic!("应为 AlterTable 变更");
    }

    let dialect = PostgresDdlDialect::default();
    let stmts = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
    let ddl = stmts.join("\n");

    assert!(ddl.contains("DROP COLUMN"), "应包含 DROP COLUMN tags");
    assert!(ddl.contains("ADD COLUMN"), "应包含 ADD COLUMN status");
    assert!(ddl.contains("DEFAULT"), "新列应包含默认值");
}
