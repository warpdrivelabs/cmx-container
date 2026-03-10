/// SQLite CRUD 仓库实现

use futures::future::LocalBoxFuture;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::cell::DataValue;
use crate::error::Result;
use crate::transaction::{execute_sql_by_ids, query_sql_by_ids};
use super::{
    build_where_clause, data_value_to_sql_literal, extract_count_from_dataset,
    Condition, CrudRepository, PageResult,
};

/// SQLite CRUD 仓库
///
/// 实现了 [`CrudRepository`] trait，所有操作均通过 db_id 查找 SQLite 连接池执行。
/// SQLite 常用于测试环境和嵌入式场景，使用内存数据库时 URL 为 `sqlite::memory:`。
#[derive(Debug, Clone, Default)]
pub struct SqliteCrudRepository;

fn build_insert(table: &str, values: &[(String, DataValue)]) -> String {
    let cols: Vec<&str> = values.iter().map(|(c, _)| c.as_str()).collect();
    let vals: Vec<String> = values.iter().map(|(_, v)| data_value_to_sql_literal(v)).collect();
    format!("INSERT INTO {} ({}) VALUES ({})", table, cols.join(", "), vals.join(", "))
}

fn build_update_by_id(table: &str, id_col: &str, id: &DataValue, values: &[(String, DataValue)]) -> String {
    let set_clause: Vec<String> = values.iter()
        .map(|(c, v)| format!("{} = {}", c, data_value_to_sql_literal(v)))
        .collect();
    format!("UPDATE {} SET {} WHERE {} = {}", table, set_clause.join(", "), id_col, data_value_to_sql_literal(id))
}

fn build_update_by_condition(table: &str, conditions: &[Condition], values: &[(String, DataValue)]) -> String {
    let set_clause: Vec<String> = values.iter()
        .map(|(c, v)| format!("{} = {}", c, data_value_to_sql_literal(v)))
        .collect();
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("UPDATE {} SET {}", table, set_clause.join(", "))
    } else {
        format!("UPDATE {} SET {} WHERE {}", table, set_clause.join(", "), where_clause)
    }
}

fn build_count(table: &str, conditions: &[Condition]) -> String {
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("SELECT COUNT(*) AS count FROM {}", table)
    } else {
        format!("SELECT COUNT(*) AS count FROM {} WHERE {}", table, where_clause)
    }
}

/// SQLite 分页使用 LIMIT ... OFFSET ...
fn build_select_page(table: &str, conditions: &[Condition], page: u64, page_size: u64) -> String {
    let where_clause = build_where_clause(conditions);
    let offset = (page.saturating_sub(1)) * page_size;
    if where_clause.is_empty() {
        format!("SELECT * FROM {} LIMIT {} OFFSET {}", table, page_size, offset)
    } else {
        format!("SELECT * FROM {} WHERE {} LIMIT {} OFFSET {}", table, where_clause, page_size, offset)
    }
}

impl CrudRepository for SqliteCrudRepository {
    fn find_by_id<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, id_col: &'a str, id: &'a DataValue, dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            let sql = format!("SELECT * FROM {} WHERE {} = {}", table, id_col, data_value_to_sql_literal(id));
            query_sql_by_ids(db_id, txn_id, &sql, dataset_id).await
        })
    }

    fn find_all<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            query_sql_by_ids(db_id, txn_id, &format!("SELECT * FROM {}", table), dataset_id).await
        })
    }

    fn find_list<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, conditions: &'a [Condition], dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            let where_clause = build_where_clause(conditions);
            let sql = if where_clause.is_empty() {
                format!("SELECT * FROM {}", table)
            } else {
                format!("SELECT * FROM {} WHERE {}", table, where_clause)
            };
            query_sql_by_ids(db_id, txn_id, &sql, dataset_id).await
        })
    }

    fn find_page<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, conditions: &'a [Condition],
        page: u64, page_size: u64, dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<PageResult>> {
        Box::pin(async move {
            let count_ds = query_sql_by_ids(db_id, txn_id, &build_count(table, conditions), "__count__").await?;
            let total = extract_count_from_dataset(&count_ds);
            let data = query_sql_by_ids(db_id, txn_id, &build_select_page(table, conditions, page, page_size), dataset_id).await?;
            Ok(PageResult { data, total, page, page_size })
        })
    }

    fn count<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let ds = query_sql_by_ids(db_id, txn_id, &build_count(table, conditions), "__count__").await?;
            Ok(extract_count_from_dataset(&ds))
        })
    }

    fn insert<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            execute_sql_by_ids(db_id, txn_id, &build_insert(table, values)).await
        })
    }

    fn update_by_id<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, id_col: &'a str, id: &'a DataValue, values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            execute_sql_by_ids(db_id, txn_id, &build_update_by_id(table, id_col, id, values)).await
        })
    }

    fn update_by_condition<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, conditions: &'a [Condition], values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            execute_sql_by_ids(db_id, txn_id, &build_update_by_condition(table, conditions, values)).await
        })
    }

    fn delete_by_id<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, id_col: &'a str, id: &'a DataValue,
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = format!("DELETE FROM {} WHERE {} = {}", table, id_col, data_value_to_sql_literal(id));
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }

    fn delete_by_condition<'a>(
        &'a self, db_id: &'a str, txn_id: Option<&'a str>,
        table: &'a str, conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let where_clause = build_where_clause(conditions);
            let sql = if where_clause.is_empty() {
                format!("DELETE FROM {}", table)
            } else {
                format!("DELETE FROM {} WHERE {}", table, where_clause)
            };
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }
}
