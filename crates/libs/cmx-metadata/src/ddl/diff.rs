//! 增量 DDL 生成 — 比对两个版本的 TableDefine 生成 ALTER TABLE 等语句

use std::collections::HashMap;

use cmx_core::model::cell::{ColumnDefine, IndexDefine, TableDefine};
use crate::MetadataError;
use super::DdlDialect;

/// 列变更类型
#[derive(Debug, Clone)]
pub enum ColumnChange {
    AddColumn(ColumnDefine),
    DropColumn(String),
    AlterColumn { old: ColumnDefine, new: Box<ColumnDefine> },
}

/// 索引变更类型
#[derive(Debug, Clone)]
pub enum IndexChange {
    AddIndex(IndexDefine),
    DropIndex(String),
}

/// 表级别的变更描述
#[derive(Debug, Clone)]
pub enum TableChange {
    /// 新建表
    CreateTable(TableDefine),
    /// 删除表
    DropTable(String),
    /// 修改表
    AlterTable {
        table_name: String,
        schema: Option<String>,
        column_changes: Vec<ColumnChange>,
        index_changes: Vec<IndexChange>,
        comment_change: Option<String>,
    },
}

/// 增量 DDL 工具
pub struct DdlDiff;

impl DdlDiff {
    /// 比对两组 TableDefine，生成变更列表
    pub fn diff(old: &[TableDefine], new: &[TableDefine]) -> Vec<TableChange> {
        let old_map: HashMap<&str, &TableDefine> = old.iter().map(|t| (t.table_name.as_str(), t)).collect();
        let new_map: HashMap<&str, &TableDefine> = new.iter().map(|t| (t.table_name.as_str(), t)).collect();

        let mut changes = Vec::new();

        // 新增的表
        for new_table in new {
            if !old_map.contains_key(new_table.table_name.as_str()) {
                changes.push(TableChange::CreateTable(new_table.clone()));
            }
        }

        // 删除的表
        for old_table in old {
            if !new_map.contains_key(old_table.table_name.as_str()) {
                changes.push(TableChange::DropTable(old_table.table_name.clone()));
            }
        }

        // 修改的表
        for new_table in new {
            if let Some(old_table) = old_map.get(new_table.table_name.as_str()) {
                let column_changes = Self::diff_columns(&old_table.columns, &new_table.columns);
                let index_changes = Self::diff_indexes(&old_table.indexes, &new_table.indexes);
                let comment_change = if old_table.comment != new_table.comment {
                    new_table.comment.clone()
                } else {
                    None
                };

                if !column_changes.is_empty() || !index_changes.is_empty() || comment_change.is_some() {
                    changes.push(TableChange::AlterTable {
                        table_name: new_table.table_name.clone(),
                        schema: new_table.schema.clone(),
                        column_changes,
                        index_changes,
                        comment_change,
                    });
                }
            }
        }

        changes
    }

    /// 比对列变更
    fn diff_columns(old_cols: &[ColumnDefine], new_cols: &[ColumnDefine]) -> Vec<ColumnChange> {
        let old_map: HashMap<&str, &ColumnDefine> = old_cols.iter().map(|c| (c.name.as_str(), c)).collect();
        let new_map: HashMap<&str, &ColumnDefine> = new_cols.iter().map(|c| (c.name.as_str(), c)).collect();

        let mut changes = Vec::new();

        // 新增列
        for new_col in new_cols {
            if !old_map.contains_key(new_col.name.as_str()) {
                changes.push(ColumnChange::AddColumn(new_col.clone()));
            }
        }

        // 删除列
        for old_col in old_cols {
            if !new_map.contains_key(old_col.name.as_str()) {
                changes.push(ColumnChange::DropColumn(old_col.name.clone()));
            }
        }

        // 修改列（类型/nullable/default/length 变更）
        for new_col in new_cols {
            if let Some(old_col) = old_map.get(new_col.name.as_str())
                && Self::column_changed(old_col, new_col)
            {
                changes.push(ColumnChange::AlterColumn {
                    old: (*old_col).clone(),
                    new: Box::new(new_col.clone()),
                });
            }
        }

        changes
    }

    /// 判断列是否有实质性变更
    fn column_changed(old: &ColumnDefine, new: &ColumnDefine) -> bool {
        old.field_type != new.field_type
            || old.is_nullable != new.is_nullable
            || old.default_value != new.default_value
            || old.length != new.length
            || old.precision != new.precision
            || old.scale != new.scale
    }

    /// 比对索引变更
    fn diff_indexes(old_idxs: &[IndexDefine], new_idxs: &[IndexDefine]) -> Vec<IndexChange> {
        let old_map: HashMap<&str, &IndexDefine> = old_idxs.iter().map(|i| (i.name.as_str(), i)).collect();
        let new_map: HashMap<&str, &IndexDefine> = new_idxs.iter().map(|i| (i.name.as_str(), i)).collect();

        let mut changes = Vec::new();

        for new_idx in new_idxs {
            if !old_map.contains_key(new_idx.name.as_str()) {
                changes.push(IndexChange::AddIndex(new_idx.clone()));
            }
        }

        for old_idx in old_idxs {
            if !new_map.contains_key(old_idx.name.as_str()) {
                changes.push(IndexChange::DropIndex(old_idx.name.clone()));
            }
        }

        changes
    }

    /// 将变更列表转为 DDL 语句
    pub fn changes_to_ddl(
        dialect: &dyn DdlDialect,
        changes: &[TableChange],
    ) -> Result<Vec<String>, MetadataError> {
        let mut stmts = Vec::new();

        for change in changes {
            match change {
                TableChange::CreateTable(table) => {
                    stmts.push(dialect.generate_create_table(table)?);
                    stmts.extend(dialect.generate_comments(table)?);
                    stmts.extend(dialect.generate_create_indexes(table)?);
                }
                TableChange::DropTable(name) => {
                    stmts.push(format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", name));
                }
                TableChange::AlterTable {
                    table_name,
                    schema,
                    column_changes,
                    index_changes,
                    comment_change,
                } => {
                    for cc in column_changes {
                        match cc {
                            ColumnChange::AddColumn(col) => {
                                stmts.push(dialect.generate_add_column(
                                    table_name,
                                    schema.as_deref(),
                                    col,
                                )?);
                            }
                            ColumnChange::DropColumn(name) => {
                                stmts.push(dialect.generate_drop_column(
                                    table_name,
                                    schema.as_deref(),
                                    name,
                                )?);
                            }
                            ColumnChange::AlterColumn { old, new } => {
                                stmts.extend(dialect.generate_alter_column(
                                    table_name,
                                    schema.as_deref(),
                                    old,
                                    new,
                                )?);
                            }
                        }
                    }
                    for ic in index_changes {
                        match ic {
                            IndexChange::AddIndex(idx) => {
                                let qualified = match schema {
                                    Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                                    None => format!("\"{}\"", table_name),
                                };
                                let cols = idx.columns.iter()
                                    .map(|c| format!("\"{}\"", c))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                let unique = match idx.kind {
                                    cmx_core::model::cell::IndexKind::Unique => "UNIQUE ",
                                    cmx_core::model::cell::IndexKind::Normal => "",
                                };
                                stmts.push(format!(
                                    "CREATE {}INDEX \"{}\" ON {} ({});",
                                    unique, idx.name, qualified, cols
                                ));
                            }
                            IndexChange::DropIndex(name) => {
                                stmts.push(format!("DROP INDEX IF EXISTS \"{}\";", name));
                            }
                        }
                    }
                    if let Some(comment) = comment_change {
                        let qualified = match schema {
                            Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                            None => format!("\"{}\"", table_name),
                        };
                        let escaped = comment.replace('\'', "''");
                        stmts.push(format!("COMMENT ON TABLE {} IS '{}';", qualified, escaped));
                    }
                }
            }
        }

        Ok(stmts)
    }

    /// 一步到位：比对 + 生成 DDL
    pub fn diff_to_ddl(
        dialect: &dyn DdlDialect,
        old: &[TableDefine],
        new: &[TableDefine],
    ) -> Result<Vec<String>, MetadataError> {
        let changes = Self::diff(old, new);
        Self::changes_to_ddl(dialect, &changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddl::postgres::PostgresDdlDialect;
    use cmx_core::model::cell::{FieldType, IndexDefine, IndexKind};

    fn make_simple_table(name: &str, cols: Vec<ColumnDefine>, indexes: Vec<IndexDefine>) -> TableDefine {
        TableDefine {
            table_name: name.to_string(),
            display_name: name.to_string(),
            columns: cols,
            primary_keys: vec![],
            indexes,
            version: 1,
            created_at: None, updated_at: None,
            i18n: false,
            comment: None, schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        }
    }

    fn make_col(name: &str, ft: FieldType, nullable: bool) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: name.to_string(),
            field_type: ft,
            is_primary_key: false,
            is_nullable: nullable,
            default_value: None,
            i18n: false,
            length: None, precision: None, scale: None,
            db_type: None, ordinal: None,
            created_at: None, updated_at: None,
            is_foreign_key: false,
            foreign_key_table: None, foreign_key_column: None,
            extensions: HashMap::new(),
        }
    }

    #[test]
    fn test_diff_new_table() {
        let old: Vec<TableDefine> = vec![];
        let new = vec![make_simple_table("new_table", vec![], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], TableChange::CreateTable(t) if t.table_name == "new_table"));
    }

    #[test]
    fn test_diff_drop_table() {
        let old = vec![make_simple_table("old_table", vec![], vec![])];
        let new: Vec<TableDefine> = vec![];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], TableChange::DropTable(name) if name == "old_table"));
    }

    #[test]
    fn test_diff_add_column() {
        let old = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
        ], vec![])];
        let new = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
            make_col("name", FieldType::String, true),
        ], vec![])];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert_eq!(column_changes.len(), 1);
            assert!(matches!(&column_changes[0], ColumnChange::AddColumn(c) if c.name == "name"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_drop_column() {
        let old = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
            make_col("old_col", FieldType::String, true),
        ], vec![])];
        let new = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
        ], vec![])];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert!(column_changes.iter().any(|c| matches!(c, ColumnChange::DropColumn(n) if n == "old_col")));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_alter_column_type() {
        let old = vec![make_simple_table("t", vec![
            make_col("name", FieldType::String, true),
        ], vec![])];
        let new = vec![make_simple_table("t", vec![
            make_col("name", FieldType::Text, true),
        ], vec![])];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert!(column_changes.iter().any(|c| matches!(c, ColumnChange::AlterColumn { .. })));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_add_index() {
        let old = vec![make_simple_table("t", vec![], vec![])];
        let new = vec![make_simple_table("t", vec![], vec![
            IndexDefine {
                name: "idx_new".to_string(),
                columns: vec!["col1".to_string()],
                kind: IndexKind::Normal,
            },
        ])];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert!(matches!(&index_changes[0], IndexChange::AddIndex(i) if i.name == "idx_new"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_to_ddl() {
        let dialect = PostgresDdlDialect::default();
        let old = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
        ], vec![])];
        let new = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
            make_col("name", FieldType::String, true),
        ], vec![])];

        let stmts = DdlDiff::diff_to_ddl(&dialect, &old, &new).unwrap();
        assert!(!stmts.is_empty());
        assert!(stmts[0].contains("ADD COLUMN"));
    }

    #[test]
    fn test_no_changes() {
        let tables = vec![make_simple_table("t", vec![
            make_col("id", FieldType::Int, false),
        ], vec![])];
        let changes = DdlDiff::diff(&tables, &tables);
        assert!(changes.is_empty());
    }
}
