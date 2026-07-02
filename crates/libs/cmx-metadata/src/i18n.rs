//! i18n 伴生表生成模块
//!
//! 提供从基础表定义生成多语言伴生表定义的功能。
//! 伴生表的表名后缀为 `_i18n`，包含 `ref_id` 和 `locale` 列，
//! 以及所有标记为 `i18n: true` 的列。
//!
//! 本模块是从 cmx-core 迁移过来的。

use cmx_core::model::cell::{ColumnDefine, FieldType, TableDefine};

/// 根据基础表定义生成多语言伴生表定义
///
/// 如果基础表的 `i18n` 字段为 `false`，或者没有标记为 `i18n` 的列，
/// 则返回 `None`。
///
/// 生成的伴生表包含：
/// - `ref_id` 列：关联到主表的 ID
/// - `locale` 列：语言标识（如 "zh_CN", "en_US"）
/// - 所有 `i18n: true` 的列
///
/// 主键为 `ref_id` 和 `locale` 的组合。
///
/// # 参数
/// * `base` - 基础表定义
///
/// # 返回值
/// * 成功返回 `Some(TableDefine)` - 伴生表定义
/// * 返回 `None` - 如果基础表不需要多语言支持
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
            create_time: c.create_time,
            update_time: c.update_time,
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
        create_time: None,
        update_time: None,
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
        create_time: None,
        update_time: None,
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
        create_time: base.create_time,
        update_time: base.update_time,
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
                },
                ColumnDefine {
                    name: "name".to_string(),
                    label: "域名称".to_string(),
                    field_type: FieldType::String,
                    is_primary_key: false,
                    is_nullable: false,
                    default_value: None,
                    i18n: true,
                    length: Some(64),
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
                },
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![],
            version: 1,
            create_time: None,
            update_time: None,
            i18n: true,
            comment: None,
            schema: None,
            tablespace: None,
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
        };
        assert!(derive_i18n_table_define(&base).is_none());
    }
}
