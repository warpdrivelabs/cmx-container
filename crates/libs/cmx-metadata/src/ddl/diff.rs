//! 增量 DDL 生成模块
//!
//! 提供 DDL 增量比对功能，通过比对两个版本的 `TableDefine`
//! 生成 ALTER TABLE 等变更语句。
//!
//! # 功能特性
//! - 支持表级别的新增、删除、修改
//! - 支持列级别的新增、删除、修改（类型、nullable、默认值等）
//! - 支持索引的新增和删除
//! - 支持表注释的变更
//!
//! # 使用示例
//! ```ignore
//! use cmx_metadata::ddl::diff::{DdlDiff, TableChange};
//! use cmx_metadata::ddl::postgres::PostgresDdlDialect;
//!
//! let changes = DdlDiff::diff(&old_tables, &new_tables);
//! let dialect = PostgresDdlDialect::default();
//! let ddl_statements = DdlDiff::diff_to_ddl(&dialect, &old_tables, &new_tables)?;
//! ```

use super::DdlDialect;
use crate::MetadataError;
use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, TableDefine};
use std::collections::HashMap;
use tracing::info;

/// 列变更类型
///
/// 描述对列的变更操作：
/// - 新增列
/// - 删除列
/// - 修改列（类型、nullable、默认值等）
#[derive(Debug, Clone)]
pub enum ColumnChange {
    /// 新增列
    AddColumn(ColumnDefine),
    /// 删除列
    DropColumn(String),
    /// 修改列（包含旧列和新列定义）
    AlterColumn {
        old: ColumnDefine,
        new: Box<ColumnDefine>,
    },
}

/// 索引变更类型
///
/// 描述对索引的变更操作：
/// - 新增索引
/// - 删除索引
#[derive(Debug, Clone)]
pub enum IndexChange {
    /// 新增索引
    AddIndex(IndexDefine),
    /// 删除索引
    DropIndex(String),
}

/// 表级别的变更描述
///
/// 描述对表的变更操作：
/// - 新建表
/// - 删除表
/// - 修改表（包含列变更、索引变更、注释变更等）
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
///
/// 提供静态方法用于：
/// - 比对两组 `TableDefine` 生成变更列表
/// - 将变更列表转换为 DDL 语句
/// - 一步到位完成比对和 DDL 生成
pub struct DdlDiff;

impl DdlDiff {
    /// 比对两组 TableDefine，生成变更列表
    ///
    /// 比对旧版本和新版本的表定义，生成增量变更。
    /// 包括：新增的表、删除的表、修改的表（列变更、索引变更、注释变更）
    ///
    /// # 比对策略
    /// - 使用 HashMap 按表名快速查找，提高比对效率
    /// - 新增表：存在于 new 但不在 old 中
    /// - 删除表：存在于 old 但不在 new 中
    /// - 修改表：同时存在于两者中，但列/索引/注释有差异
    ///
    /// # 参数
    /// * `old` - 旧版本表定义列表
    /// * `new` - 新版本表定义列表
    ///
    /// # 返回值
    /// * `Vec<TableChange>` - 变更列表
    pub fn diff(old: &[TableDefine], new: &[TableDefine]) -> Vec<TableChange> {
        // 构建按表名索引的 HashMap，加速查找
        let old_map: HashMap<&str, &TableDefine> =
            old.iter().map(|t| (t.table_name.as_str(), t)).collect();
        let new_map: HashMap<&str, &TableDefine> =
            new.iter().map(|t| (t.table_name.as_str(), t)).collect();

        let mut changes = Vec::new();

        // ============================================
        // 检测新增的表
        // ============================================
        for new_table in new {
            if !old_map.contains_key(new_table.table_name.as_str()) {
                changes.push(TableChange::CreateTable(new_table.clone()));
            }
        }

        // ============================================
        // 检测删除的表
        // ============================================
        for old_table in old {
            if !new_map.contains_key(old_table.table_name.as_str()) {
                changes.push(TableChange::DropTable(old_table.table_name.clone()));
            }
        }

        // ============================================
        // 检测修改的表（列变更、索引变更、注释变更）
        // ============================================
        for new_table in new {
            if let Some(old_table) = old_map.get(new_table.table_name.as_str()) {
                // 比对列定义差异
                let column_changes = Self::diff_columns(&old_table.columns, &new_table.columns);
                // 比对索引定义差异
                let index_changes = Self::diff_indexes(&old_table.indexes, &new_table.indexes);
                // 比对表注释变更
                let comment_change = if old_table.comment != new_table.comment {
                    new_table.comment.clone()
                } else {
                    None
                };

                // 只有当有实质变更时才记录
                if !column_changes.is_empty()
                    || !index_changes.is_empty()
                    || comment_change.is_some()
                {
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
    ///
    /// 比对两组列定义，返回列变更列表。
    /// 包括：新增列、删除列、修改列（类型、nullable、默认值等）
    ///
    /// # 参数
    /// * `old_cols` - 旧版本列定义列表
    /// * `new_cols` - 新版本列定义列表
    ///
    /// # 返回值
    /// * `Vec<ColumnChange>` - 列变更列表
    fn diff_columns(old_cols: &[ColumnDefine], new_cols: &[ColumnDefine]) -> Vec<ColumnChange> {
        let old_map: HashMap<&str, &ColumnDefine> =
            old_cols.iter().map(|c| (c.name.as_str(), c)).collect();
        let new_map: HashMap<&str, &ColumnDefine> =
            new_cols.iter().map(|c| (c.name.as_str(), c)).collect();

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
    ///
    /// 检查列的以下属性是否发生变化：
    /// - 字段类型（field_type）
    /// - 可空性（is_nullable）
    /// - 默认值（default_value）
    /// - 长度（length）
    /// - 精度（precision）、小数位（scale）：仅对 [`FieldType::Decimal`] 比较
    ///
    /// # 精度/标度的特殊处理
    /// PostgreSQL 对 `integer`/`bigint`/`smallint`/`double precision` 等内置类型
    /// 会报告类型派生的 `numeric_precision`/`numeric_scale`（如 `bigint` 恒为 64/0），
    /// 这些并非用户定义，不应参与 diff；否则会把「设计期未定义精度」与
    /// 「数据库类型派生精度」误判为变更，产生永久假阳性。因此仅当新列类型为
    /// `Decimal`（用户显式定义精度的 `NUMERIC(p,s)`）时才比较 precision/scale。
    ///
    /// # 参数
    /// * `old` - 旧列定义
    /// * `new` - 新列定义
    ///
    /// # 返回值
    /// * `bool` - 是否发生实质性变更
    fn column_changed(old: &ColumnDefine, new: &ColumnDefine) -> bool {
        if old.field_type != new.field_type
            || old.is_nullable != new.is_nullable
            || old.default_value != new.default_value
            || old.length != new.length
        {
            return true;
        }
        // 精度/标度仅对 Decimal 有意义：integer/bigint/smallint/double 等类型的
        // numeric_precision/scale 是 PG 类型派生属性（如 bigint 恒为 64/0），不应参与 diff。
        matches!(new.field_type, FieldType::Decimal)
            && (old.precision != new.precision || old.scale != new.scale)
    }

    /// 比对索引变更
    ///
    /// 比对两组索引定义，返回索引变更列表。
    /// 包括：新增索引、删除索引
    ///
    /// # 参数
    /// * `old_idxs` - 旧版本索引定义列表
    /// * `new_idxs` - 新版本索引定义列表
    ///
    /// # 返回值
    /// * `Vec<IndexChange>` - 索引变更列表
    fn diff_indexes(old_idxs: &[IndexDefine], new_idxs: &[IndexDefine]) -> Vec<IndexChange> {
        let old_map: HashMap<&str, &IndexDefine> =
            old_idxs.iter().map(|i| (i.name.as_str(), i)).collect();
        let new_map: HashMap<&str, &IndexDefine> =
            new_idxs.iter().map(|i| (i.name.as_str(), i)).collect();

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
    ///
    /// 将变更列表转换为对应数据库方言的 DDL 语句。
    ///
    /// # 生成的 DDL 类型
    /// - `TableChange::CreateTable`：生成 CREATE TABLE + COMMENT + INDEX
    /// - `TableChange::DropTable`：记录日志（不实际生成 DROP 语句，防止数据丢失）
    /// - `TableChange::AlterTable`：
    ///   - 列变更：ADD COLUMN / ALTER COLUMN（类型/nullable/default）
    ///   - 索引变更：CREATE INDEX / DROP INDEX
    ///   - 注释变更：COMMENT ON TABLE
    ///
    /// # 参数
    /// * `dialect` - DDL 方言实现
    /// * `changes` - 变更列表
    ///
    /// # 返回值
    /// * 成功返回 DDL 语句列表
    /// * 失败返回 `MetadataError`
    pub fn changes_to_ddl(
        dialect: &dyn DdlDialect,
        changes: &[TableChange],
    ) -> Result<Vec<String>, MetadataError> {
        let mut stmts = Vec::new();

        for change in changes {
            match change {
                // ============================================
                // 新建表：生成完整 DDL
                // ============================================
                TableChange::CreateTable(table) => {
                    stmts.push(dialect.generate_create_table(table)?);
                    stmts.extend(dialect.generate_comments(table)?);
                    stmts.extend(dialect.generate_create_indexes(table)?);
                }
                // ============================================
                // 删除表：仅记录日志，不实际执行（安全考虑）
                // ============================================
                TableChange::DropTable(name) => {
                    // stmts.push(format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", name));
                    info!("表元数据更新需要删除的表: {}，实际未执行", name);
                }
                // ============================================
                // 修改表：生成列变更、索引变更、注释变更的 DDL
                // ============================================
                TableChange::AlterTable {
                    table_name,
                    schema,
                    column_changes,
                    index_changes,
                    comment_change,
                } => {
                    // 处理列变更
                    for cc in column_changes {
                        match cc {
                            // 新增列
                            ColumnChange::AddColumn(col) => {
                                stmts.push(dialect.generate_add_column(
                                    table_name,
                                    schema.as_deref(),
                                    col,
                                )?);
                            }
                            // 删除列：仅记录日志，不实际执行
                            ColumnChange::DropColumn(name) => {
                                // stmts.push(dialect.generate_drop_column(
                                //     table_name,
                                //     schema.as_deref(),
                                //     name,
                                // )?);
                                info!("表元数据更新需要删除的列: {}，实际未执行", name);
                            }
                            // 修改列（类型、nullable、默认值）
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
                    // 处理索引变更
                    for ic in index_changes {
                        match ic {
                            // 新增索引
                            IndexChange::AddIndex(idx) => {
                                let qualified = match schema {
                                    Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                                    None => format!("\"{}\"", table_name),
                                };
                                let cols = idx
                                    .columns
                                    .iter()
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
                            // 删除索引
                            IndexChange::DropIndex(name) => {
                                stmts.push(format!("DROP INDEX IF EXISTS \"{}\";", name));
                            }
                        }
                    }
                    // 处理表注释变更
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
    ///
    /// 一次性完成表定义比对和 DDL 语句生成。
    ///
    /// # 参数
    /// * `dialect` - DDL 方言实现
    /// * `old` - 旧版本表定义列表
    /// * `new` - 新版本表定义列表
    ///
    /// # 返回值
    /// * 成功返回 DDL 语句列表
    /// * 失败返回 `MetadataError`
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

    fn make_simple_table(
        name: &str,
        cols: Vec<ColumnDefine>,
        indexes: Vec<IndexDefine>,
    ) -> TableDefine {
        TableDefine {
            table_name: name.to_string(),
            display_name: name.to_string(),
            columns: cols,
            primary_keys: vec![],
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
        let old = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("name", FieldType::String, true),
            ],
            vec![],
        )];

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
        let old = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("old_col", FieldType::String, true),
            ],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert!(
                column_changes
                    .iter()
                    .any(|c| matches!(c, ColumnChange::DropColumn(n) if n == "old_col"))
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_alter_column_type() {
        let old = vec![make_simple_table(
            "t",
            vec![make_col("name", FieldType::String, true)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![make_col("name", FieldType::Text, true)],
            vec![],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert!(
                column_changes
                    .iter()
                    .any(|c| matches!(c, ColumnChange::AlterColumn { .. }))
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_add_index() {
        let old = vec![make_simple_table("t", vec![], vec![])];
        let new = vec![make_simple_table(
            "t",
            vec![],
            vec![IndexDefine {
                name: "idx_new".to_string(),
                columns: vec!["col1".to_string()],
                kind: IndexKind::Normal,
            }],
        )];

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
        let old = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("name", FieldType::String, true),
            ],
            vec![],
        )];

        let stmts = DdlDiff::diff_to_ddl(&dialect, &old, &new).unwrap();
        assert!(!stmts.is_empty());
        assert!(stmts[0].contains("ADD COLUMN"));
    }

    #[test]
    fn test_no_changes() {
        let tables = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let changes = DdlDiff::diff(&tables, &tables);
        assert!(changes.is_empty());
    }

    /// bigint 列经 PG 内省还原后携带派生精度（precision=64, scale=0），
    /// 设计期定义未设置精度。两者类型相同（同为 `Int`），不应判为变更。
    #[test]
    fn column_changed_ignores_int_precision() {
        let mut db_restored = make_col("sort_no", FieldType::Int, true);
        db_restored.precision = Some(64);
        db_restored.scale = Some(0);
        db_restored.db_type = Some("BIGINT".to_string());
        let desired = make_col("sort_no", FieldType::Int, true); // precision/scale = None

        assert!(
            !DdlDiff::column_changed(&db_restored, &desired),
            "bigint 派生精度不应触发列变更"
        );

        // 整张表 diff 也应无变更
        let old = vec![make_simple_table("cf_client", vec![db_restored.clone()], vec![])];
        let new = vec![make_simple_table("cf_client", vec![desired], vec![])];
        assert!(
            DdlDiff::diff(&old, &new).is_empty(),
            "bigint 精度假阳性：表 diff 应为空"
        );
    }

    /// Decimal 类型的精度/标度变化仍应被检出。
    #[test]
    fn column_changed_detects_decimal_precision() {
        let mut old = make_col("amount", FieldType::Decimal, true);
        old.precision = Some(10);
        old.scale = Some(2);
        let mut new = make_col("amount", FieldType::Decimal, true);
        new.precision = Some(12);
        new.scale = Some(2);
        assert!(
            DdlDiff::column_changed(&old, &new),
            "Decimal 精度变化应判为变更"
        );
    }
}
