//! i18n 伴生表生成（从 cmx-core meta/base.rs 迁移）

use cmx_core::model::cell::{ColumnDefine, FieldType, TableDefine};

/// 根据基础表定义生成多语言伴生表定义（表名后缀 `_i18n`，含 ref_id、locale 及所有 i18n 列）
pub fn derive_i18n_table_define(base: &TableDefine) -> Option<TableDefine> {
    if !base.i18n {
        return None;
    }
    let i18n_columns: Vec<ColumnDefine> = base
        .columns
        .iter()
        .filter(|c| c.i18n)
        .map(|c| ColumnDefine {
            name: c.name.clone(),
            label: c.label.clone(),
            field_type: c.field_type.clone(),
            is_primary_key: false,
            is_nullable: c.is_nullable,
            default_value: c.default_value.clone(),
            i18n: false,
            length: c.length,
            precision: c.precision,
            scale: c.scale,
            db_type: c.db_type.clone(),
            ordinal: c.ordinal,
            created_at: c.created_at,
            updated_at: c.updated_at,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: c.extensions.clone(),
        })
        .collect();
    if i18n_columns.is_empty() {
        return None;
    }
    let ref_col = ColumnDefine {
        name: "ref_id".to_string(),
        label: "主表ID".to_string(),
        field_type: FieldType::Int,
        is_primary_key: false,
        is_nullable: false,
        default_value: None,
        i18n: false,
        length: None,
        precision: None,
        scale: None,
        db_type: None,
        ordinal: None,
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    };
    let locale_col = ColumnDefine {
        name: "locale".to_string(),
        label: "语言".to_string(),
        field_type: FieldType::String,
        is_primary_key: false,
        is_nullable: false,
        default_value: None,
        i18n: false,
        length: None,
        precision: None,
        scale: None,
        db_type: None,
        ordinal: None,
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    };
    let mut columns = vec![ref_col, locale_col];
    columns.extend(i18n_columns);
    let table_name = format!("{}_i18n", base.table_name);
    let display_name = format!("{}（多语言）", base.display_name);
    Some(TableDefine {
        table_name,
        display_name,
        columns,
        primary_keys: vec!["ref_id".to_string(), "locale".to_string()],
        indexes: vec![],
        version: base.version,
        created_at: base.created_at,
        updated_at: base.updated_at,
        i18n: false,
        comment: None,
        schema: base.schema.clone(),
        tablespace: base.tablespace.clone(),
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: base.extensions.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_derive_i18n_table() {
        let base = TableDefine {
            table_name: "cmx_domain".to_string(),
            display_name: "域".to_string(),
            columns: vec![
                ColumnDefine {
                    name: "id".to_string(),
                    label: "主键".to_string(),
                    field_type: FieldType::Int,
                    is_primary_key: true,
                    is_nullable: false,
                    default_value: None,
                    i18n: false,
                    length: None, precision: None, scale: None,
                    db_type: None, ordinal: None,
                    created_at: None, updated_at: None,
                    is_foreign_key: false,
                    foreign_key_table: None, foreign_key_column: None,
                    extensions: HashMap::new(),
                },
                ColumnDefine {
                    name: "name".to_string(),
                    label: "域名称".to_string(),
                    field_type: FieldType::String,
                    is_primary_key: false,
                    is_nullable: false,
                    default_value: None,
                    i18n: true,
                    length: Some(64), precision: None, scale: None,
                    db_type: None, ordinal: None,
                    created_at: None, updated_at: None,
                    is_foreign_key: false,
                    foreign_key_table: None, foreign_key_column: None,
                    extensions: HashMap::new(),
                },
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![],
            version: 1,
            created_at: None, updated_at: None,
            i18n: true,
            comment: None, schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        let i18n = derive_i18n_table_define(&base).unwrap();
        assert_eq!(i18n.table_name, "cmx_domain_i18n");
        assert_eq!(i18n.columns.len(), 3); // ref_id, locale, name
        assert_eq!(i18n.primary_keys, vec!["ref_id", "locale"]);
    }

    #[test]
    fn test_no_i18n_table_when_flag_false() {
        let base = TableDefine {
            table_name: "test".to_string(),
            display_name: "测试".to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            version: 1,
            created_at: None, updated_at: None,
            i18n: false,
            comment: None, schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };
        assert!(derive_i18n_table_define(&base).is_none());
    }
}
